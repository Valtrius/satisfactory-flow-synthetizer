"""Regression checks for mode-aware benchmark verification."""
import csv
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


class AnalyzerTests(unittest.TestCase):
    def analyze(self, mutation=None):
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
                    })
            with (root / "results.csv").open("w", newline="") as stream:
                writer = csv.DictWriter(stream, fieldnames=rows[0].keys())
                writer.writeheader()
                writer.writerows(rows)
            output = root / "summary.json"
            process = subprocess.run(
                [sys.executable, str(Path(__file__).with_name("analyze-parallelism.py")), str(root), "--output", str(output)],
                capture_output=True, text=True, check=False,
            )
            return process.returncode, json.loads(output.read_text())

    def test_modes_may_have_different_preferred_witnesses(self):
        status, summary = self.analyze()
        self.assertEqual(status, 0)
        self.assertEqual(len(summary["summary"]), 4)
        self.assertTrue(all(s["kernel_speedup_same_stage"] == 2 for s in summary["summary"] if s["variant"] == "after"))

    def test_missing_layout_fails_even_with_unchanged_preferred(self):
        def mutate(variant, mode, result):
            if variant == "after" and mode == "all":
                result["layout_keys"] = ["a"]
        status, _ = self.analyze(mutate)
        self.assertNotEqual(status, 0)

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


if __name__ == "__main__":
    unittest.main()
