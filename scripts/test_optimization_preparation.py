"""Proof and scheduling regressions for isolated optimization experiments."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import shutil
import subprocess
import unittest
from benchmark_policy import validate_pairs


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    loaded = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(loaded)
    return loaded


auditor = module("root_audit", "audit-optimization-results.py")
matrix = module("optimization_matrix", "make-optimization-campaign.py")


def diagnostic_result():
    roots = [dict(branch=branch, nodes=2, links=1, root=root, profile=0,
                  state="exhausted", wall_s=1.0, check_s=0.9)
             for branch in (0, 1) for root in (0, 1)]
    return dict(roots=roots, workers=32, mode="all_min_nl", diagnostics_enabled=True,
                diagnostics=[dict(name="solver.portfolio_proof_owner", value=dict(value="1"))],
                outcome=dict(result=dict(proof=dict(rootPartitionsExhausted=2))))


class OptimizationPreparationTests(unittest.TestCase):
    @unittest.skipUnless(shutil.which("pwsh"), "PowerShell 7 is required")
    def test_campaign_runs_sequentially_and_preserves_a_failed_suite(self):
        # Stub suites exercise orchestration without running any solver or benchmark.
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            starter = Path(__file__).with_name("start-optimization-campaign.ps1")
            shutil.copy2(starter, root / starter.name)
            suites = [{"name": name} for name in ("first", "failed", "last")]
            (root / "campaign.json").write_text(json.dumps(dict(suites=suites)))
            paths = ["campaign.json", starter.name]
            for suite in suites:
                child = root / suite["name"]
                child.mkdir()
                (child / "frozen-hashes.json").write_text("{}")
                paths.append(f"{suite['name']}/frozen-hashes.json")
                code = '''param([string]$PreparedSuite, [switch]$Run, [switch]$NoDialog)
$eventFile = Join-Path (Split-Path $PreparedSuite -Parent) 'events.txt'
$label = Split-Path $PreparedSuite -Leaf
"start $label" | Add-Content -LiteralPath $eventFile
Start-Sleep -Milliseconds 30
"end $label" | Add-Content -LiteralPath $eventFile
if ($label -eq 'failed') { exit 1 }
'verified stub' | Set-Content -LiteralPath (Join-Path $PreparedSuite 'BENCHMARK-FINISHED.txt')
'''
                (child / "start-optimization-suite.ps1").write_text(code)
            (root / "campaign-hashes.json").write_text(json.dumps({path: hashlib.sha256((root/path).read_bytes()).hexdigest() for path in paths}))
            result = subprocess.run(["pwsh", "-NoProfile", "-File", str(root/starter.name), "-PreparedCampaign", str(root), "-Run", "-NoDialog"], capture_output=True, text=True, timeout=30, check=False)
            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            self.assertEqual((root / "events.txt").read_text(encoding="utf-8-sig").splitlines(),
                             [f"{event} {name}" for name in ("first", "failed", "last") for event in ("start", "end")])
            outcomes = json.loads((root / "suite-outcomes.json").read_text(encoding="utf-8-sig"))
            self.assertEqual([o["verified"] for o in outcomes], [True, False, True])
            self.assertIn("FINISHED WITH FAILURES", (root / "CAMPAIGN-STATUS.txt").read_text(encoding="utf-8-sig"))
            suites[0]["max_scheduled_seconds"] = 28800
            (root / "campaign.json").write_text(json.dumps(dict(suites=suites)))
            (root / "campaign-hashes.json").write_text(json.dumps({path: hashlib.sha256((root/path).read_bytes()).hexdigest() for path in paths}))
            result = subprocess.run(["pwsh", "-NoProfile", "-File", str(root/starter.name), "-PreparedCampaign", str(root), "-Run", "-NoDialog"], capture_output=True, text=True, timeout=30, check=False)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual(len((root / "events.txt").read_text(encoding="utf-8-sig").splitlines()), 6)
            self.assertEqual(len(json.loads((root / "remaining-suites.json").read_text(encoding="utf-8-sig"))), 3)
            self.assertEqual(json.loads((root / "suite-outcomes.json").read_text(encoding="utf-8-sig")), [])
            self.assertIn("PAUSED", (root / "CAMPAIGN-STATUS.txt").read_text(encoding="utf-8-sig"))

    def test_counts_only_the_returned_independent_proof_owner(self):
        result = diagnostic_result()
        self.assertEqual(auditor.audit(result)[0], [])
        result["outcome"]["result"]["proof"]["rootPartitionsExhausted"] = 4
        self.assertTrue(auditor.audit(result)[0])

    def test_cancelled_and_first_optimum_roots_are_not_exhausted(self):
        result = diagnostic_result()
        result["roots"][-1]["state"] = "optimum"
        self.assertTrue(auditor.audit(result)[0])
        result["outcome"]["result"]["proof"]["rootPartitionsExhausted"] = 1
        self.assertEqual(auditor.audit(result)[0], [])
        result["roots"][-1]["state"] = "cancelled"
        self.assertEqual(auditor.audit(result)[0], [])

    def test_duplicate_completions_cannot_fabricate_exhaustion(self):
        result = diagnostic_result()
        result["roots"][1] = copy.deepcopy(result["roots"][0])
        self.assertIn("Duplicate root completion identity", auditor.audit(result)[0])

    def test_refinement_is_confined_to_minimum_link_enumeration(self):
        result = diagnostic_result()
        result["roots"][0]["second_source"] = 1
        self.assertEqual(auditor.audit(result)[0], [])
        for mode in ("one_min_nl", "all_min_n"):
            result["mode"] = mode
            self.assertTrue(auditor.audit(result)[0])

    def test_missing_owner_and_mixed_instrumentation_are_rejected(self):
        result = diagnostic_result()
        result["diagnostics"] = []
        self.assertTrue(auditor.audit(result)[0])
        result["diagnostics_enabled"] = False
        self.assertTrue(auditor.audit(result)[0])

    def test_generated_pairs_match_budgets_and_cover_required_scopes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "matrix"
            campaign = matrix.generate(root)
            self.assertEqual(len(campaign["corpus"]) + len(campaign["additional_cases"]), 17)
            count = 0
            for suite in campaign["suites"]:
                manifest = json.loads((root / suite["manifest"]).read_text())
                jobs = manifest["jobs"]
                self.assertEqual(len(validate_pairs(jobs)), len(jobs) // 2)
                self.assertEqual(manifest["MaxScheduledSeconds"], sum(j["TimeoutSeconds"] + 15 for j in jobs))
                identities = [(j["Case"], j["Mode"], j["Variant"], j["Workers"], j["Repeat"]) for j in jobs]
                self.assertEqual(len(identities), len(set(identities)))
                self.assertTrue(all((root / j["CaseFile"]).is_file() for j in jobs))
                self.assertTrue(all(j["Diagnostics"] == suite["diagnostics"] for j in jobs))
                if suite["family"] == "partitions":
                    self.assertEqual({j["Mode"] for j in jobs}, set(matrix.MODES))
                    self.assertEqual(len({j["Case"] for j in jobs if j["Mode"] == "all_min_nl"}), 17)
                if suite["family"] == "workers":
                    self.assertEqual({j["Workers"] for j in jobs}, {1, 2, 8, 16, 32})
                count += len(jobs)
            self.assertEqual(count, campaign["total_jobs"])

    def test_confirmation_requires_known_finalists_and_has_six_repeats(self):
        with tempfile.TemporaryDirectory() as directory:
            result = matrix.generate(Path(directory) / "confirm", ["pairs-both"])
            self.assertEqual(result["suites"][0]["repeats"], 6)
            self.assertEqual(result["suites"][1]["repeats"], 4)
            with self.assertRaises(ValueError):
                matrix.generate(Path(directory) / "invalid", ["not-a-variant"])


if __name__ == "__main__":
    unittest.main()
