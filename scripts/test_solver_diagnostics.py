"""Proof-diagnostic audits and bounded benchmark controller regressions."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import shutil
import subprocess
import unittest


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    loaded = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(loaded)
    return loaded


auditor = module("root_audit", "audit-optimization-results.py")
ROOT = Path(__file__).resolve().parents[1]
SCRIPT_TEST_ROOT = ROOT / "target" / "script-tests"


def temporary_campaign():
    # Hosted Windows runners can treat scripts created under the user TEMP tree
    # differently from scripts under the checked-out workspace. Real frozen
    # campaigns also live under the repository target tree, so test the same
    # filesystem context here.
    SCRIPT_TEST_ROOT.mkdir(parents=True, exist_ok=True)
    return tempfile.TemporaryDirectory(dir=SCRIPT_TEST_ROOT)


def campaign_debug(result, root):
    parts = [f"stdout:\n{result.stdout}", f"stderr:\n{result.stderr}"]
    for path in [root / "CAMPAIGN-STATUS.txt", *sorted(root.glob("*/runner.stderr.log"))]:
        if path.is_file():
            parts.append(f"{path.relative_to(root)}:\n{path.read_text(encoding='utf-8-sig')}")
    return "\n".join(parts)


def diagnostic_result():
    roots = [dict(branch=branch, nodes=2, links=1, root=root, profile=0,
                  state="exhausted", wall_s=1.0, check_s=0.9)
             for branch in (0, 1) for root in (0, 1)]
    return dict(roots=roots, workers=32, mode="all_min_nl", diagnostics_enabled=True,
                diagnostics=[dict(name="solver.portfolio_proof_owner", value=dict(value="1"))],
                outcome=dict(result=dict(proof=dict(rootPartitionsExhausted=2))))


class SolverDiagnosticTests(unittest.TestCase):

    def test_adaptive_cover_selects_parent_or_every_child_without_double_counting(self):
        result = diagnostic_result()
        common = dict(branch=1, nodes=2, links=1, profile=0, source=0, parent_root=0,
                      child_count=2, state="exhausted", wall_s=1.0, check_s=0.9, start_s=2.0)
        result["roots"] = [dict(common, root=0, second_source=None, proof_committed=False),
                           dict(common, root=1, second_source=0, proof_committed=True, refinement_trigger_s=1.0),
                           dict(common, root=2, second_source=1, proof_committed=True, refinement_trigger_s=1.0)]
        self.assertEqual(auditor.audit(result)[0], [])
        result["roots"][0]["proof_committed"] = True
        self.assertIn("Parent and children both counted in the proof", auditor.audit(result)[0])
        result["roots"][0]["proof_committed"] = False
        result["roots"][2]["proof_committed"] = False
        self.assertIn("Incomplete children counted as a full parent cover", auditor.audit(result)[0])
        result["roots"][1]["proof_committed"] = False
        result["roots"][0]["state"] = "cancelled"
        result["outcome"]["result"]["proof"]["rootPartitionsExhausted"] = 0
        self.assertEqual(auditor.audit(result)[0], [])
        result["roots"][1]["refinement_trigger_s"] = 3.0
        self.assertIn("Child started before its optimum trigger", auditor.audit(result)[0])

    def test_refinement_grace_is_audited_against_child_start(self):
        result = diagnostic_result()
        result["roots"] = [dict(branch=1, nodes=2, links=1, root=0, profile=0,
                                source=0, parent_root=0, child_count=1, state="cancelled",
                                wall_s=1., check_s=.9, start_s=0., proof_committed=False),
                           dict(branch=1, nodes=2, links=1, root=1, profile=0,
                                source=0, parent_root=0, child_count=1, second_source=0,
                                state="exhausted", wall_s=1., check_s=.9, start_s=1.25,
                                proof_committed=True, refinement_trigger_s=1., refinement_grace_ms=250)]
        result["outcome"]["result"]["proof"]["rootPartitionsExhausted"] = 1
        self.assertEqual(auditor.audit(result)[0], [])
        result["roots"][1]["start_s"] = 1.24
        self.assertIn("Child started before its refinement grace elapsed", auditor.audit(result)[0])


    @unittest.skipUnless(shutil.which("pwsh"), "PowerShell 7 is required")
    def test_campaign_runs_sequentially_and_preserves_a_failed_suite(self):
        # Stub suites exercise orchestration without running any solver or benchmark.
        with temporary_campaign() as directory:
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
            event_file = root / "events.txt"
            self.assertTrue(event_file.is_file(), campaign_debug(result, root))
            self.assertEqual(event_file.read_text(encoding="utf-8-sig").splitlines(),
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

    @unittest.skipUnless(shutil.which("pwsh"), "PowerShell 7 is required")
    def test_session_budget_stops_a_stalled_owned_suite(self):
        with temporary_campaign() as directory:
            root = Path(directory)
            suite = root / "stalled"
            suite.mkdir()
            starter = Path(__file__).with_name("start-optimization-campaign.ps1")
            shutil.copy2(starter, root / starter.name)
            (root / "campaign.json").write_text(json.dumps(dict(
                suites=[dict(name="stalled", max_scheduled_seconds=0)],
                runtime_budget_seconds=3, suite_overhead_seconds=0)))
            (suite / "frozen-hashes.json").write_text("{}")
            (suite / "start-optimization-suite.ps1").write_text('''param([string]$PreparedSuite, [switch]$Run, [switch]$NoDialog)
'started' | Set-Content -LiteralPath (Join-Path $PreparedSuite 'started.txt')
Start-Sleep -Seconds 30
'must not finish' | Set-Content -LiteralPath (Join-Path $PreparedSuite 'BENCHMARK-FINISHED.txt')
''')
            files = ["campaign.json", starter.name, "stalled/frozen-hashes.json"]
            (root / "campaign-hashes.json").write_text(json.dumps({path: hashlib.sha256((root/path).read_bytes()).hexdigest() for path in files}))
            result = subprocess.run(["pwsh", "-NoProfile", "-File", str(root/starter.name), "-PreparedCampaign", str(root), "-Run", "-NoDialog"], capture_output=True, text=True, timeout=15, check=False)
            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            self.assertTrue((suite / "started.txt").is_file(), campaign_debug(result, root))
            self.assertFalse((suite / "BENCHMARK-FINISHED.txt").exists())
            outcome = json.loads((root / "suite-outcomes.json").read_text(encoding="utf-8-sig"))[0]
            self.assertFalse(outcome["verified"])
            self.assertEqual(outcome["reason"], "session_budget")
            self.assertIn("Session time budget reached", (root / "CAMPAIGN-STATUS.txt").read_text(encoding="utf-8-sig"))

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


if __name__ == "__main__":
    unittest.main()
