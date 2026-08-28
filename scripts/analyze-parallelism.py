"""Compare native results per solve mode, including before/after kernel runs."""
import argparse
import csv
import json
from collections import defaultdict
from pathlib import Path
from statistics import median

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("directories", type=Path, nargs="+")
parser.add_argument("--output", type=Path, required=True)
args = parser.parse_args()
rows = []
failures = []
for directory in args.directories:
    directory_rows = []
    with (directory / "results.csv").open(encoding="utf-8-sig", newline="") as source:
        for row in csv.DictReader(source):
            row["result"] = json.loads(Path(row["result_file"]).read_text(encoding="utf-8-sig"))
            row.setdefault("mode", "all")
            row.setdefault("variant", "after")
            rows.append(row)
            directory_rows.append(row)
    schedule_path = directory / "schedule.json"
    if schedule_path.exists():
        schedule = json.loads(schedule_path.read_text(encoding="utf-8-sig"))
        if isinstance(schedule, dict):
            schedule = [schedule]
        if len(schedule) != len(directory_rows):
            failures.append(f"Unfinished schedule: {directory}, {len(directory_rows)}/{len(schedule)} runs")
    identities = [(r["case"], r["mode"], r["variant"], r["stage"], r["workers"], r.get("repeat")) for r in directory_rows]
    if len(set(identities)) != len(identities):
        failures.append(f"Duplicate benchmark identities: {directory}")

baselines = {}
for row in rows:
    if row["stage"] == "baseline":
        key = row["case"], row["mode"]
        if key not in baselines or row["variant"] == "before":
            baselines[key] = row["result"]

groups = defaultdict(list)
for row in rows:
    result = row["result"]
    baseline = baselines.get((row["case"], row["mode"]))
    if baseline is None:
        failures.append(f"Missing baseline for {row['case']} {row['mode']}")
        continue
    if not result["status"].startswith("Optimal("):
        failures.append(f"Incomplete or failed run: {row['result_file']}")
    if result["status"] != baseline["status"]:
        failures.append(f"Status/optimum mismatch: {row['result_file']}")
    if result["layout_keys"] != baseline["layout_keys"]:
        failures.append(f"Full layout-set mismatch: {row['result_file']}")
    if result.get("preferred_key") != baseline.get("preferred_key"):
        failures.append(f"Preferred witness mismatch: {row['result_file']}")
    if result["layouts"] != len(result["layout_keys"]) or result["layout_keys"] != sorted(set(result["layout_keys"])):
        failures.append(f"Malformed result-set accounting: {row['result_file']}")
    if result.get("mode", "all") != row["mode"]:
        failures.append(f"Mode mismatch: {row['result_file']}")
    if "validated" in result and not result["validated"]:
        failures.append(f"Unvalidated witness: {row['result_file']}")
    if row["mode"] == "optimal":
        if result["layouts"] != 1 or result["layout_keys"] != [result["preferred_key"]]:
            failures.append(f"Find-optimal did not return exactly its terminal witness: {row['result_file']}")
        exhaustive = baselines.get((row["case"], "all"))
        if exhaustive and (result["status"] != exhaustive["status"] or result["preferred_key"] not in exhaustive["layout_keys"]):
            failures.append(f"Optimal witness/optimum disagrees with full enumeration: {row['result_file']}")
    groups[row["case"], row["mode"], row["variant"], row["stage"], int(row["workers"])].append(row)

summary = []
for (case, mode, variant, stage, workers), samples in sorted(groups.items()):
    if len({row.get("hotspot_recording", "legacy") for row in samples}) != 1:
        failures.append(f"Mixed profiling settings: {case} {mode} {variant} {stage} w{workers}")
    wall = [float(row["wall_s"]) for row in samples]
    baseline_samples = groups.get((case, mode, variant, "baseline", workers), [])
    baseline_wall = median(float(row["wall_s"]) for row in baseline_samples) if baseline_samples else None
    single_samples = groups.get((case, mode, variant, stage, 1), [])
    single_wall = median(float(row["wall_s"]) for row in single_samples) if single_samples and all(row["status"].startswith("Optimal(") for row in single_samples) else None
    diagnostics = [{item["name"]: int(item["value"]["value"]) for item in (row["result"]["diagnostics"] or []) if item["value"]["type"] == "integer"} for row in samples]
    before = groups.get((case, mode, "before", stage, workers), [])
    before_wall = median(float(row["wall_s"]) for row in before) if before else None
    first_valid = [float(row["first_valid_s"]) for row in samples if row.get("first_valid_s")]
    summary.append({
        "case": case, "mode": mode, "variant": variant, "stage": stage, "workers": workers, "runs": len(samples),
        "median_wall_s": median(wall), "min_wall_s": min(wall), "max_wall_s": max(wall),
        "speedup_vs_same_workers": baseline_wall / median(wall) if baseline_wall else None,
        "same_stage_speedup_from_one_worker": single_wall / median(wall) if single_wall else None,
        "parallel_efficiency": single_wall / median(wall) / workers if single_wall else None,
        "kernel_speedup_same_stage": before_wall / median(wall) if before_wall else None,
        "median_first_valid_s": median(first_valid) if first_valid else None,
        "median_process_cpu_s": median(float(row["process_cpu_s"]) for row in samples),
        "median_cpu_utilization": median(float(row["cpu_utilization"]) for row in samples),
        "max_sampled_peak_working_set_bytes": max(int(row["sampled_peak_working_set_bytes"]) for row in samples),
        "layouts": samples[0]["result"]["layouts"],
        "median_root_partitions": median(item.get("custom.root_partitions", 0) for item in diagnostics),
        "median_donated_tasks": median(item.get("custom.donated_tasks", 0) for item in diagnostics),
        "median_shared_cache_hits": median(item.get("custom.shared_cache_hits", 0) for item in diagnostics),
        "median_partition_planning_s": median(item.get("custom.partition_planning_ns", 0) for item in diagnostics) / 1e9,
        "median_states": median(item.get("custom.canonical_states_retained", 0) for item in diagnostics),
        "median_decisions": median(item.get("custom.raw_structural_decisions", 0) for item in diagnostics),
        "median_hotspots_ns": {key: median(row["result"].get("hotspots", {}).get(key, 0) for row in samples) for key in samples[0]["result"].get("hotspots", {})},
    })
args.output.parent.mkdir(parents=True, exist_ok=True)
args.output.write_text(json.dumps({"failures": failures, "summary": summary}, indent=2))
for item in summary:
    print(f"{item['case']} {item['mode']} {item['variant']} {item['stage']} w{item['workers']}: {item['median_wall_s']:.3f}s [{item['min_wall_s']:.3f}, {item['max_wall_s']:.3f}], n={item['runs']}")
if failures:
    raise SystemExit("\n".join(failures))
print(f"All {len(rows)} runs match their mode-specific baseline. Optimal witnesses agree with available full-enumeration baselines.")
