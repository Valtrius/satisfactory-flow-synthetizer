"""Regression checks for mode-aware benchmark verification."""
import csv
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


class AnalyzerTests(unittest.TestCase):
    def fixed_fixture(self):
        spec = importlib.util.spec_from_file_location("hard_profile", Path(__file__).with_name("hard-profile.py"))
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        profile = {"splitter2": 1, "splitter3": 0, "merger2": 1, "merger3": 0}
        request = {"node_count": 2, "link_count": 1, "mode": "all", "stage": "baseline", "workers": 1}
        problem = {"inputs": ["2", "3"], "outputs": ["1", "4"], "maxLinkRate": "1200"}
        job = {"id": "fixture", "request": request, "problem": problem, "expected_profiles": [profile]}
        result = {"schema_version": 1, "kind": "fixed_obligation", "scope": "selected_profiles",
                  "request": dict(request), "problem": problem, "expected_profiles": [profile],
                  "validated": True, "status": "exhausted", "deadline_fired": False, "wall_s": 0.1,
                  "hotspots": {}, "activity": {"active": [], "records": [], "dropped": 0},
                  "profiles": [{"profile": profile, "exhausted": True, "incomplete_reason": None,
                                "roots": 2, "roots_exhausted": 2, "solutions": [
                                    {"canonicalGraphKey": [1], "nodeCount": 2, "linkCount": 1}]}]}
        return module, job, result

    def test_fixed_work_rejects_wrong_scope_selection_and_partial_proof(self):
        mutations = [
            lambda r: r.update(scope="global"),
            lambda r: r["request"].update(link_count=2),
            lambda r: r.update(profiles=[]),
            lambda r: r["profiles"][0].update(roots_exhausted=1),
            lambda r: r["profiles"][0]["solutions"][0].update(nodeCount=3),
            lambda r: r.update(validated=False),
        ]
        for mutation in mutations:
            module, job, result = self.fixed_fixture()
            self.assertEqual(len(module.validate_result(job, result)), 1)
            mutation(result)
            with self.assertRaises(ValueError):
                module.validate_result(job, result)

    def test_fixed_work_timeout_is_incomplete_and_controls_must_exhaust(self):
        module, job, result = self.fixed_fixture()
        result.update(status="incomplete", deadline_fired=True)
        result["profiles"][0].update(exhausted=False, roots_exhausted=1,
                                     incomplete_reason={"kind": "cancelled"})
        self.assertEqual(len(module.validate_result(job, result)), 1)
        result["deadline_fired"] = False
        with self.assertRaises(ValueError):
            module.validate_result(job, result)
        result["deadline_fired"] = True
        job["request"]["must_exhaust"] = True
        result["request"]["must_exhaust"] = True
        with self.assertRaises(ValueError):
            module.validate_result(job, result)

    def test_fixed_work_verifier_compares_exact_sets_and_process_identity(self):
        module, job, result = self.fixed_fixture()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "results").mkdir()
            binary = root / "fixture.exe"
            binary.write_bytes(b"fixture binary")
            module.write(root / "binary.json", {"path": str(binary), "sha256": module.digest(binary)})
            jobs = [dict(job, id=f"fixture-{i}") for i in range(2)]
            module.write(root / "schedule.json", jobs)
            for j in jobs:
                module.write(root / "results" / (j["id"] + ".json"), result)
            with (root / "process-metrics.csv").open("w", newline="") as stream:
                writer = csv.DictWriter(stream, fieldnames=["id", "watchdog_killed", "exit_code"])
                writer.writeheader()
                writer.writerows({"id": j["id"], "watchdog_killed": False, "exit_code": 0} for j in jobs)
            module.verify(root)
            result["profiles"][0]["solutions"] = []
            module.write(root / "results/fixture-1.json", result)
            with self.assertRaisesRegex(ValueError, "witness set|results disagree"):
                module.verify(root)
            module.write(root / "results/fixture-0.json", result)
            binary.write_bytes(b"changed")
            with self.assertRaisesRegex(ValueError, "Binary hash mismatch"):
                module.verify(root)

    @unittest.skipUnless(os.name == "nt", "Windows profiling executable")
    def test_fixed_work_executable_returns_valid_local_proof_and_cancellation(self):
        binary = Path(__file__).resolve().parent.parent / "target/release/examples/profile_obligation.exe"
        if not binary.exists():
            self.skipTest("Build the bench-internals profile_obligation example first")
        module, _, _ = self.fixed_fixture()
        with tempfile.TemporaryDirectory(prefix="fixed work contract ") as directory:
            root = Path(directory)
            for name, inputs, outputs, nodes, links, seconds in [
                ("tiny", ["2", "3"], ["1", "4"], 2, 1, 10),
                ("cancel", ["36"], ["11", "9", "7", "5", "3", "1"], 9, 12, 1),
            ]:
                case = {"problem": {"inputs": inputs, "outputs": outputs, "maxLinkRate": "1200"}}
                module.write(root / "case.json", case)
                request = {"case_file": "case.json", "node_count": nodes, "link_count": links,
                           "mode": "all", "stage": "p1", "workers": 2, "timeout_s": seconds,
                           "profile": None, "hotspots": True}
                request_file = root / (name + ".request.json")
                result_file = root / (name + ".json")
                module.write(request_file, request)
                listing = subprocess.run([str(binary), "--list", str(request_file)], check=True,
                                         capture_output=True, text=True, timeout=10)
                expected = json.loads(listing.stdout)
                subprocess.run([str(binary), str(request_file), str(result_file)], check=True,
                               capture_output=True, text=True, timeout=20)
                result = module.read(result_file)
                job = dict(request=request, problem=expected["problem"], expected_profiles=expected["profiles"])
                module.validate_result(job, result)
                self.assertEqual(result["status"], "exhausted" if name == "tiny" else "incomplete")
                self.assertEqual(result["deadline_fired"], name == "cancel")
                self.assertEqual(result["activity"]["active"], [])
                self.assertTrue(result_file.with_suffix(".heartbeat.log").exists())

    @unittest.skipUnless(os.name == "nt" and shutil.which("pwsh") and shutil.which("rustc"), "Windows runner integration")
    def test_runner_preserves_watchdog_failure_and_runs_the_next_job(self):
        # A synthetic child exercises process handling without running a solver benchmark.
        with tempfile.TemporaryDirectory(prefix="custom watchdog ") as directory:
            root = Path(directory)
            source = root / "fixture.rs"
            source.write_text(r'''
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let executable = std::env::current_exe().unwrap();
    let variant = executable.parent().unwrap().file_name().unwrap().to_str().unwrap();
    let marker = executable.parent().unwrap().parent().unwrap().join("started");
    if !marker.exists() {
        std::fs::write(marker, "first job").unwrap();
        std::thread::sleep(std::time::Duration::from_secs(60));
        return;
    }
    let result = format!(r#"{{"case":"fixture","binary_origin":"{}","mode":"{}","stage":"{}","workers":{},"max_nodes":{},"timeout_s":{},"hotspot_recording":{},"problem":{{"maxLinkRate":"1200"}},"status":"fixture_success","layouts":0,"wall_s":0.001}}"#,
        variant, args[7], args[5], args[2], args[3], args[1], args[8] == "on");
    std::fs::write(&args[6], result).unwrap();
}
''')
            subprocess.run(["rustc", str(source), "-o", str(root / "profile_case.exe")], check=True, capture_output=True, timeout=30)
            for variant in ("before", "after"):
                (root / variant).mkdir()
                shutil.copy2(root / "profile_case.exe", root / variant / "profile_case.exe")
            (root / "case.json").write_text(json.dumps({"name": "fixture", "problem": {"inputs": ["1"], "outputs": ["1"], "maxLinkRate": "1200"}}))
            jobs = [{"Case": "fixture", "CaseFile": "case.json", "Mode": "optimal", "Stage": "baseline", "Variant": variant, "Workers": 1, "Repeat": 1, "TimeoutSeconds": 1, "MaxNodes": 1, "Cohort": "stress", "Hotspots": "on"} for variant in ("before", "after")]
            manifest = root / "manifest.json"
            manifest.write_text(json.dumps({"jobs": jobs}))
            output = root / "results"
            # Execute a frozen script outside the repository, just like the launcher.
            shutil.copy2(Path(__file__).with_name("parallelism-ladder.ps1"), root / "parallelism-ladder.ps1")
            process = subprocess.run(["pwsh", "-NoProfile", "-File", str(root / "parallelism-ladder.ps1"), "-RepositoryRoot", str(Path(__file__).resolve().parent.parent), "-JobManifest", str(manifest), "-BinaryDirectory", str(root / "after"), "-ReferenceBinaryDirectory", str(root / "before"), "-OutputDirectory", str(output), "-CancellationGraceSeconds", "1"], capture_output=True, text=True, timeout=30)
            self.assertEqual(process.returncode, 0, process.stdout + process.stderr)
            failed = json.loads((output / "failed-jobs.json").read_text(encoding="utf-8-sig"))
            self.assertEqual(len(failed), 1)
            self.assertEqual(failed[0]["reason"], "cancellation_watchdog")
            self.assertFalse(failed[0]["solver_result_available"])
            self.assertFalse((output / (failed[0]["name"] + ".json")).exists())
            with (output / "results.csv").open(encoding="utf-8-sig", newline="") as stream:
                completed = list(csv.DictReader(stream))
            self.assertEqual(len(completed), 1)
            self.assertEqual(completed[0]["status"], "fixture_success")
            self.assertNotEqual(completed[0]["variant"], failed[0]["variant"])
            self.assertEqual(completed[0]["hotspot_recording"], "True")
            result = json.loads(Path(completed[0]["result_file"]).read_text())
            self.assertEqual(result["binary_origin"], completed[0]["variant"])
            hashes = json.loads((output / "binaries.json").read_text(encoding="utf-8-sig"))
            self.assertEqual({item["variant"] for item in hashes}, {"before", "after"})

    def analyze(self, mutation=None, *, stress=False, allow=False, watchdog=False, no_results=False, corrupt_binary=False):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            rows = []
            for variant in ("before", "after"):
                for mode in ("all", "optimal"):
                    result = {
                        "mode": mode, "status": "Optimal(N=2, L=1)",
                        "layout_keys": ["a", "b"] if mode == "all" else ["b"],
                        "layouts": 2 if mode == "all" else 1,
                        "preferred_key": "a" if mode == "all" else "b",
                        "validated": True, "diagnostics": [],
                        "max_nodes": 2, "timeout_s": 1,
                        "hotspot_recording": False,
                        "solutions": [{"graph": "same physical solution"}],
                    }
                    if mutation:
                        mutation(variant, mode, result)
                    path = root / f"{variant}-{mode}.json"
                    path.write_text(json.dumps(result))
                    rows.append({
                        "case": "fixture", "mode": mode, "variant": variant,
                        "stage": "baseline", "workers": 1, "wall_s": 2 if variant == "before" else 1,
                        "status": result["status"], "result_file": str(path),
                        "process_cpu_s": 1, "cpu_utilization": 1,
                        "sampled_peak_working_set_bytes": 1024,
                        "max_nodes": 2, "timeout_s": 1,
                        "cohort": "stress" if stress else "reference",
                        "hotspot_recording": str(result["hotspot_recording"]),
                    })
            with (root / "results.csv").open("w", newline="") as stream:
                writer = csv.DictWriter(stream, fieldnames=rows[0].keys())
                writer.writeheader()
                writer.writerows(rows)
            if corrupt_binary:
                binary = root / "fixture.exe"
                binary.write_bytes(b"changed executable")
                (root / "binaries.json").write_text(json.dumps([
                    {"variant": variant, "case": "profile_case", "path": str(binary),
                     "sha256": hashlib.sha256(b"original executable").hexdigest()}
                    for variant in ("before", "after")
                ]))
            if watchdog:
                (root / "failed-jobs.json").write_text(json.dumps([
                    {"name": "fixture-watchdog", "reason": "cancellation_watchdog"}
                ]))
            if no_results:
                (root / "results.csv").unlink()
            output = root / "summary.json"
            process = subprocess.run(
                [sys.executable, str(Path(__file__).with_name("analyze-parallelism.py")), str(root), "--output", str(output)] + (["--allow-incomplete"] if allow else []),
                capture_output=True, text=True, check=False,
            )
            return process.returncode, json.loads(output.read_text())

    def test_modes_may_have_different_preferred_witnesses(self):
        status, summary = self.analyze()
        self.assertEqual(status, 0)
        self.assertEqual(len(summary["summary"]), 4)
        self.assertTrue(all(s["kernel_speedup_same_stage"] == 2 for s in summary["summary"] if s["variant"] == "after"))

    def test_watchdog_failure_is_not_accepted_as_a_stress_timeout(self):
        status, summary = self.analyze(watchdog=True, stress=True, allow=True)
        self.assertNotEqual(status, 0)
        self.assertEqual(len(summary["summary"]), 4)
        self.assertTrue(any("cancellation_watchdog" in failure for failure in summary["failures"]))

    def test_all_failed_jobs_still_produce_a_failure_summary(self):
        status, summary = self.analyze(watchdog=True, no_results=True, allow=True)
        self.assertNotEqual(status, 0)
        self.assertEqual(summary["summary"], [])
        self.assertTrue(any("No solver results" in failure for failure in summary["failures"]))

    def test_missing_layout_fails_even_with_unchanged_preferred(self):
        def mutate(variant, mode, result):
            if variant == "after" and mode == "all":
                result["layout_keys"] = ["a"]
        status, _ = self.analyze(mutate)
        self.assertNotEqual(status, 0)

    def test_changed_physical_solution_fails_even_with_identical_keys(self):
        def mutate(variant, _mode, result):
            if variant == "after":
                result["solutions"] = [{"graph": "changed physical solution"}]
        status, summary = self.analyze(mutate)
        self.assertNotEqual(status, 0)
        self.assertTrue(any("Saved solution objects differ" in f for f in summary["failures"]))

    def test_changed_binary_fails_verification(self):
        status, summary = self.analyze(corrupt_binary=True)
        self.assertNotEqual(status, 0)
        self.assertTrue(any("Binary hash mismatch" in f for f in summary["failures"]))

    def test_different_instrumentation_does_not_produce_a_speedup(self):
        def mutate(variant, _mode, result):
            result["hotspot_recording"] = variant == "before"
        status, summary = self.analyze(mutate)
        self.assertEqual(status, 0)
        self.assertTrue(all(item["kernel_speedup_same_stage"] is None for item in summary["summary"] if item["variant"] == "after"))

    def test_optimal_witness_must_belong_to_exhaustive_set(self):
        def mutate(variant, mode, result):
            if mode == "optimal":
                result["layout_keys"] = ["missing"]
                result["preferred_key"] = "missing"
        status, _ = self.analyze(mutate)
        self.assertNotEqual(status, 0)

    def test_incomplete_is_not_an_optimal_sample(self):
        def mutate(variant, mode, result):
            if variant == "after" and mode == "optimal":
                result["status"] = "Incomplete(Cancelled)"
        status, _ = self.analyze(mutate)
        self.assertNotEqual(status, 0)

    @staticmethod
    def timeout(_variant, _mode, result):
        result.update(status="Incomplete(Cancelled)", layout_keys=[], layouts=0,
                      preferred_key=None, deadline_fired=True,
                      outcome={"kind": "incomplete", "result": {
                          "reason": {"kind": "cancelled"}, "bestKnown": None,
                          "proof": {"nodeCountsExhaustedThrough": 1}}})

    def test_stress_timeouts_have_no_completion_time_or_speedup(self):
        status, data = self.analyze(self.timeout, stress=True, allow=True)
        self.assertEqual(status, 0)
        for item in data["summary"]:
            self.assertEqual(item["completion_counts"], {"timed_out": 1})
            self.assertIsNone(item["median_wall_s"])
            self.assertIsNone(item["kernel_speedup_same_stage"])
            self.assertIsNone(item["speedup_vs_same_workers"])

    def test_allow_incomplete_does_not_weaken_reference_gate(self):
        status, _ = self.analyze(self.timeout, allow=True)
        self.assertNotEqual(status, 0)

    def test_stress_cancel_without_deadline_is_failure(self):
        def mutate(variant, mode, result):
            self.timeout(variant, mode, result)
            result["deadline_fired"] = False
        status, _ = self.analyze(mutate, stress=True, allow=True)
        self.assertNotEqual(status, 0)

    def test_bounded_exhaustion_is_not_global_unsat_or_optimal(self):
        def mutate(variant, mode, result):
            self.timeout(variant, mode, result)
            result["status"] = "Incomplete(ResourceLimit)"
            result["outcome"]["result"]["reason"] = {"kind": "resource_limit"}
            result["outcome"]["result"]["proof"]["nodeCountsExhaustedThrough"] = 2
        status, data = self.analyze(mutate, stress=True, allow=True)
        self.assertEqual(status, 0)
        self.assertTrue(all(s["completion_counts"] == {"bounded_exhausted": 1} and s["median_wall_s"] is None for s in data["summary"]))

    def test_resource_limit_without_exhaustion_is_failure(self):
        def mutate(variant, mode, result):
            self.timeout(variant, mode, result)
            result["outcome"]["result"]["reason"] = {"kind": "resource_limit"}
        status, _ = self.analyze(mutate, stress=True, allow=True)
        self.assertNotEqual(status, 0)

    def test_stress_completed_sets_still_must_match(self):
        def mutate(variant, mode, result):
            if variant == "after" and mode == "all":
                result["layout_keys"] = ["a"]
                result["layouts"] = 1
        status, _ = self.analyze(mutate, stress=True, allow=True)
        self.assertNotEqual(status, 0)

    def test_manifest_plan_has_bounded_budgets_and_no_single_worker(self):
        repository = Path(__file__).resolve().parent.parent
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "plan"
            process = subprocess.run([
                "pwsh", "-NoProfile", "-File", str(repository / "scripts/parallelism-ladder.ps1"),
                "-JobManifest", "benchmarks/custom/screening.json", "-PlanOnly", "-OutputDirectory", str(output),
            ], cwd=repository, capture_output=True, text=True, check=False)
            self.assertEqual(process.returncode, 0, process.stderr)
            jobs = json.loads((output / "schedule.json").read_text(encoding="utf-8-sig"))
            self.assertEqual(len(jobs), 24)
            self.assertTrue(all(j["Workers"] in (16, 32) for j in jobs))
            self.assertLessEqual(sum(j["TimeoutSeconds"] for j in jobs), 92 * 60)
            self.assertTrue(all(Path(j["CasePath"]).is_file() for j in jobs))

    @unittest.skipUnless(shutil.which("pwsh"), "PowerShell manifest validation")
    def test_manifest_rejects_ambiguous_binary_selection(self):
        repository = Path(__file__).resolve().parent.parent
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "case.json").write_text(json.dumps({"problem": {"maxLinkRate": "1200"}}))
            base = {"Case": "fixture", "CaseFile": "case.json", "Mode": "optimal", "Stage": "baseline", "Workers": 16, "Repeat": 1, "TimeoutSeconds": 1, "MaxNodes": 1, "Cohort": "stress"}
            for index, (jobs, reference, expected) in enumerate([
                ([dict(base, Variant="before")], False, "Before jobs require"),
                ([dict(base, Variant="unknown")], True, "Invalid binary variant"),
                ([base, base], True, "Duplicate job identities"),
            ]):
                with self.subTest(expected=expected):
                    manifest = root / f"manifest-{index}.json"
                    manifest.write_text(json.dumps({"jobs": jobs}))
                    command = ["pwsh", "-NoProfile", "-File", str(repository / "scripts/parallelism-ladder.ps1"), "-JobManifest", str(manifest), "-PlanOnly", "-OutputDirectory", str(root / f"plan-{index}")]
                    if reference:
                        command += ["-ReferenceBinaryDirectory", str(root)]
                    process = subprocess.run(command, cwd=repository, capture_output=True, text=True, timeout=15)
                    self.assertNotEqual(process.returncode, 0)
                    self.assertIn(expected, process.stderr)

    @unittest.skipUnless(shutil.which("pwsh"), "PowerShell manifest validation")
    def test_repeated_manifest_preserves_selected_variants_and_hard_caps(self):
        repository = Path(__file__).resolve().parent.parent
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "plan"
            process = subprocess.run([
                "pwsh", "-NoProfile", "-File", str(repository / "scripts/parallelism-ladder.ps1"),
                "-JobManifest", "benchmarks/custom/repeat-scheduling-screening.json", "-PlanOnly",
                "-ReferenceBinaryDirectory", str(Path(directory) / "reference"), "-OutputDirectory", str(output),
            ], cwd=repository, capture_output=True, text=True, timeout=15)
            self.assertEqual(process.returncode, 0, process.stderr)
            jobs = json.loads((output / "schedule.json").read_text(encoding="utf-8-sig"))
            self.assertEqual(len(jobs), 62)
            self.assertEqual(sum(j["Variant"] == "before" for j in jobs), 24)
            self.assertTrue(all(j["Variant"] == "after" for j in jobs if j["Cohort"] == "stress"))
            self.assertTrue(all(j["Workers"] in (16, 32) and j["Hotspots"] == "off" for j in jobs))
            self.assertLessEqual(sum(j["TimeoutSeconds"] for j in jobs), 62 * 60)
            self.assertTrue(all(Path(j["CasePath"]).is_file() for j in jobs))

    def test_activity_overlap_and_cancellation_tail(self):
        def mutate(_variant, _mode, result):
            result["activity"] = {"dropped": 0, "records": [
                {"kind": "root", "start_ns": 0, "end_ns": 500_000_000},
                {"kind": "root", "start_ns": 250_000_000, "end_ns": 750_000_000},
                {"kind": "root", "start_ns": 750_000_000, "end_ns": 1_000_000_000},
                {"kind": "group", "start_ns": 0, "end_ns": 1_250_000_000},
                {"kind": "cancel_requested", "start_ns": 400_000_000, "end_ns": 400_000_000},
            ]}
        status, data = self.analyze(mutate)
        self.assertEqual(status, 0)
        activity = next(s for s in data["summary"] if s["variant"] == "after")["activity"][0]
        self.assertEqual(activity["phase_intervals"]["root"], {"count": 3, "sum_s": 1.25, "union_s": 1.0, "peak_overlap": 2})
        self.assertEqual(activity["mean_active_roots_over_observed_wall"], 1.25)
        self.assertEqual(activity["roots_active_at_cancel"], 2)
        self.assertEqual(activity["post_cancel_tail_s_by_phase"], {"root": 0.6, "group": 0.85})

    def test_activity_truncation_is_explicit(self):
        def mutate(_variant, _mode, result):
            result["activity"] = {"dropped": 5, "records": []}
        status, data = self.analyze(mutate)
        self.assertEqual(status, 0)
        self.assertTrue(all(not s["activity"][0]["complete"] for s in data["summary"]))

    def test_reversed_activity_interval_fails_verification(self):
        def mutate(_variant, _mode, result):
            result["activity"] = {"dropped": 0, "records": [{"kind": "root", "start_ns": 2, "end_ns": 1}]}
        status, data = self.analyze(mutate)
        self.assertNotEqual(status, 0)
        self.assertTrue(any("Malformed activity trace" in f for f in data["failures"]))


if __name__ == "__main__":
    unittest.main()
