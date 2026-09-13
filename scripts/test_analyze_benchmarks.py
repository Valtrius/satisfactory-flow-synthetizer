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
from unittest.mock import patch
from benchmark_policy import paired_summary, validate_pairs, validate_placement, diagnostics_enabled


class AnalyzerTests(unittest.TestCase):
    def test_any_optimum_policy_retains_objective_and_full_enumeration_checks(self):
        from benchmark_policy import result_comparison_errors
        a = dict(status="Optimal(N=7, L=8)", layout_keys=["a"], preferred_key="a", solutions=[{"graph": "a"}])
        b = dict(status="Optimal(N=7, L=8)", layout_keys=["b"], preferred_key="b", solutions=[{"graph": "b"}])
        self.assertTrue(result_comparison_errors(a, b, "optimal"))
        self.assertEqual(result_comparison_errors(a, b, "optimal", "any_optimum"), [])
        self.assertTrue(result_comparison_errors(a, dict(b, status="Optimal(N=7, L=9)"), "optimal", "any_optimum"))
        for mode in ["all", "minimum_links"]:
            self.assertTrue(result_comparison_errors(a, b, mode, "any_optimum"))
            self.assertEqual(result_comparison_errors(a, dict(a, preferred_key="b"), mode, "any_optimum"), [])
            self.assertTrue(result_comparison_errors(a, dict(a, solutions=[{"graph": "changed"}]), mode, "any_optimum"))
        with self.assertRaises(ValueError):
            result_comparison_errors(a, b, "optimal", "unknown")

    def test_pair_policy_rejects_unmatched_settings_and_separated_members(self):
        jobs = [dict(Case="fixture", Mode="optimal", Stage="p1", Workers=16, Repeat=1,
                     MaxNodes=2, TimeoutSeconds=5, Hotspots="off", ProcessorAffinity="ffff",
                     PairId="one", PairRole=role, Variant=variant)
                for role, variant in [("reference", "before"), ("candidate", "variables")]]
        self.assertEqual(len(validate_pairs(jobs)), 1)
        for field, value in [("ProcessorAffinity", "ffff0000"), ("Workers", 32),
                             ("Hotspots", "on"), ("PairRole", "reference"),
                             ("Diagnostics", True), ("AstraDiagnostics", True)]:
            changed = [dict(j) for j in jobs]
            changed[1][field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                validate_pairs(changed)
        second = [dict(j, PairId="two") for j in jobs]
        with self.assertRaisesRegex(ValueError, "adjacent"):
            validate_pairs([jobs[0], second[0], jobs[1], second[1]])

    def test_recorded_and_current_diagnostics_keep_matching_instrumentation(self):
        for key in ("Diagnostics", "diagnostics_enabled", "AstraDiagnostics", "astra_diagnostics"):
            with self.subTest(key=key):
                self.assertTrue(diagnostics_enabled({key: True}))
                self.assertFalse(diagnostics_enabled({key: False}))
        self.assertFalse(diagnostics_enabled({}))

    def test_placement_policy_checks_startup_mask_topology_and_worker_count(self):
        job = dict(ProcessorAffinity="ffff", Workers=16, PairId="one", PairRole="reference",
                   Comparison="variables", CacheBytes=96 * 2**20)
        row = dict(processor_affinity="ffff", affinity_observed="ffff", affinity_before_resume="True",
                   affinity_applied_s="0.01", process_wall_s="10", pair_id="one",
                   pair_role="reference", comparison="variables")
        topology = dict(available_mask="ffffffff", relationships=[dict(relation=2, level=3,
                        cache_bytes=96 * 2**20, groups=[dict(group=0, mask="ffff")])])
        validate_placement(job, row, topology)
        for field, value in [("affinity_observed", "ffff0000"), ("affinity_before_resume", "False"),
                             ("processor_affinity", ""), ("pair_role", "candidate")]:
            with self.subTest(field=field), self.assertRaises(ValueError):
                validate_placement(job, dict(row, **{field: value}), topology)
        with self.assertRaises(ValueError):
            validate_placement(dict(job, Workers=32), row, topology)
        with self.assertRaises(ValueError):
            validate_placement(dict(job, CacheBytes=32 * 2**20), row, topology)

    def test_paired_summary_separates_cpu_masks_and_rejects_incomplete_speedups(self):
        rows = []
        for mask, ratio in [("ffff", 0.9), ("ffff0000", 1.1)]:
            for repeat in range(5):
                for role, variant, wall in [("reference", "before", 100), ("candidate", "variables", 100 * ratio)]:
                    rows.append(dict(run_directory="fixture", pair_id=f"{mask}-{repeat}", pair_role=role,
                        comparison="variables", case="fixture", mode="optimal", stage="p1", workers="16",
                        max_nodes="2", timeout_s="120", processor_affinity=mask, hotspot_recording="False",
                        variant=variant, completion="optimal", wall_s=wall, first_valid_s=wall, process_cpu_s=wall))
        summary = paired_summary(rows)
        self.assertEqual(len(summary), 2)
        self.assertAlmostEqual(summary[0]["wall_s"]["median_change_pct"], -10)
        self.assertAlmostEqual(summary[1]["wall_s"]["median_change_pct"], 10)
        rows[0]["completion"] = "timed_out"
        summary = paired_summary(rows)
        self.assertIsNone(summary[0]["wall_s"])
        self.assertFalse(summary[0]["all_completed"])

    def test_runner_preserves_watchdog_failure_and_runs_the_next_job(self):
        # A synthetic child exercises process handling without running a solver benchmark.
        with tempfile.TemporaryDirectory(prefix="solver watchdog ") as directory:
            root = Path(directory)
            source = root / "fixture.cs"
            source.write_text(r'''
using System;
using System.IO;
using System.Reflection;
using System.Threading;

public static class Fixture {
    public static void Main(string[] args) {
        string executable = Assembly.GetExecutingAssembly().Location;
        string directory = Path.GetDirectoryName(executable);
        string variant = new DirectoryInfo(directory).Name;
        string marker = Path.Combine(Directory.GetParent(directory).FullName, "started");
        if (!File.Exists(marker)) {
            File.WriteAllText(marker, "first job");
            Thread.Sleep(60000);
            return;
        }
        string result = "{\"case\":\"fixture\",\"binary_origin\":\"" + variant
            + "\",\"mode\":\"" + args[4]
            + "\",\"stage\":\"baseline\",\"comparison_protocol\":\"layout-v1\",\"workers\":" + args[1]
            + ",\"max_nodes\":" + args[2] + ",\"timeout_s\":" + args[0]
            + ",\"hotspot_recording\":false,\"problem\":{\"maxLinkRate\":\"1200\"},\"status\":\"fixture_success\",\"layouts\":0,\"wall_s\":0.001}";
        File.WriteAllText(args[3], result);
    }
}
''')
            compile_env = os.environ.copy()
            compile_env.update(SFS_FIXTURE_SOURCE=str(source), SFS_FIXTURE_OUTPUT=str(root / "profile_solver.exe"))
            subprocess.run(
                ["powershell", "-NoProfile", "-Command", "Add-Type -TypeDefinition (Get-Content -LiteralPath $env:SFS_FIXTURE_SOURCE -Raw) -OutputAssembly $env:SFS_FIXTURE_OUTPUT -OutputType ConsoleApplication"],
                check=True, capture_output=True, timeout=30, env=compile_env,
            )
            for variant in ("before", "after"):
                (root / variant).mkdir()
                shutil.copy2(root / "profile_solver.exe", root / variant / "profile_solver.exe")
            (root / "case.json").write_text(json.dumps({"name": "fixture", "problem": {"inputs": ["1"], "outputs": ["1"], "maxLinkRate": "1200"}}))
            jobs = [{"Case": "fixture", "CaseFile": "case.json", "Mode": "one_min_nl", "Stage": "baseline", "Variant": variant, "Workers": 1, "Repeat": 1, "TimeoutSeconds": 1, "MaxNodes": 1, "Cohort": "stress", "Hotspots": "off"} for variant in ("before", "after")]
            manifest = root / "manifest.json"
            manifest.write_text(json.dumps({"RunnerProtocol":"layout-v1", "jobs": jobs}))
            variant_map = root / "variants.json"
            variant_map.write_text(json.dumps({v:str(root/v) for v in ("before","after")}))
            output = root / "results"
            # Execute a frozen script outside the repository, just like the launcher.
            shutil.copy2(Path(__file__).with_name("run-benchmark-screen.ps1"), root / "run-benchmark-screen.ps1")
            shutil.copy2(Path(__file__).with_name("benchmark-affinity.ps1"), root / "benchmark-affinity.ps1")
            process = subprocess.run(["pwsh", "-NoProfile", "-File", str(root / "run-benchmark-screen.ps1"), "-RepositoryRoot", str(Path(__file__).resolve().parent.parent), "-JobManifest", str(manifest), "-VariantBinaryMap", str(variant_map), "-OutputDirectory", str(output), "-CancellationGraceSeconds", "1"], capture_output=True, text=True, timeout=30)
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
            self.assertEqual(completed[0]["hotspot_recording"], "False")
            sample_file = output / (Path(completed[0]["result_file"]).stem + ".process-samples.csv")
            self.assertTrue(sample_file.is_file())
            result = json.loads(Path(completed[0]["result_file"]).read_text())
            self.assertEqual(result["binary_origin"], completed[0]["variant"])
            hashes = json.loads((output / "binaries.json").read_text(encoding="utf-8-sig"))
            self.assertEqual({item["variant"] for item in hashes}, {"before", "after"})

    def analyze(self, mutation=None, *, stress=False, allow=False, watchdog=False, no_results=False, corrupt_binary=False, corrupt_snapshot=False, stage="baseline", variants=("before", "after")):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "results"
            root.mkdir()
            if corrupt_snapshot:
                (root.parent / "source.rs").write_text("changed")
                (root.parent / "frozen-hashes.json").write_text(json.dumps({"source.rs": hashlib.sha256(b"original").hexdigest()}))
            rows = []
            for variant in variants:
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
                        "stage": stage, "workers": 1, "wall_s": 2 if variant == "before" else 1,
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
                [sys.executable, str(Path(__file__).with_name("analyze-benchmarks.py")), str(root), "--output", str(output)] + (["--allow-incomplete"] if allow else []),
                capture_output=True, text=True, check=False,
            )
            return process.returncode, json.loads(output.read_text())

    def test_modes_may_have_different_preferred_witnesses(self):
        status, summary = self.analyze()
        self.assertEqual(status, 0)
        self.assertEqual(len(summary["summary"]), 4)
        self.assertTrue(all(s["kernel_speedup_same_stage"] == 2 for s in summary["summary"] if s["variant"] == "after"))

    def test_preserved_before_is_a_reference_without_baseline_scheduling(self):
        status, summary = self.analyze(stage="p1")
        self.assertEqual(status, 0, summary["failures"])
        self.assertTrue(all(s["speedup_vs_same_workers"] is None for s in summary["summary"]))
        self.assertTrue(all(s["kernel_speedup_same_stage"] == 2 for s in summary["summary"] if s["variant"] == "after"))

    def test_nonbaseline_reference_still_requires_an_anchor_and_exact_agreement(self):
        status, summary = self.analyze(stage="p1", variants=("after",))
        self.assertNotEqual(status, 0)
        self.assertTrue(any("Missing completed baseline" in f for f in summary["failures"]))
        def mutate(variant, _mode, result):
            if variant == "after":
                result["solutions"] = [{"graph": "changed physical solution"}]
        status, summary = self.analyze(mutate, stage="p1")
        self.assertNotEqual(status, 0)
        self.assertTrue(any("Saved solution objects differ" in f for f in summary["failures"]))

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

    def test_changed_frozen_source_fails_verification(self):
        status, summary = self.analyze(corrupt_snapshot=True)
        self.assertNotEqual(status, 0)
        self.assertTrue(any("Frozen artifact hash mismatch" in failure for failure in summary["failures"]))

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



if __name__ == "__main__":
    unittest.main()
