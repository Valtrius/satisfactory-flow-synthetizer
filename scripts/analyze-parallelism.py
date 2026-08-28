"""Compare native results per solve mode, including before/after kernel runs."""
import argparse
import csv
import hashlib
import json
from collections import Counter, defaultdict
from fractions import Fraction
from pathlib import Path
from statistics import median


def activity_summary(trace, wall_s):
    """Wall-clock spans can overlap; these are task counts, not CPU utilization."""
    if trace is None:
        return None
    records = trace["records"]
    if not isinstance(trace["dropped"], int) or trace["dropped"] < 0:
        raise ValueError("invalid dropped activity count")
    by_kind = defaultdict(list)
    for record in records:
        start, end = record["start_ns"], record["end_ns"]
        if not all(isinstance(t, int) for t in (start, end)) or not 0 <= start <= end:
            raise ValueError("invalid activity interval")
        by_kind[record["kind"]].append(record)

    def intervals(items):
        events = defaultdict(int)
        for item in items:
            events[item["start_ns"]] += 1
            events[item["end_ns"]] -= 1
        active = peak = busy = 0
        previous = 0
        for time, delta in sorted(events.items()):
            if active:
                busy += time - previous
            active += delta
            peak = max(peak, active)
            previous = time
        return {"count": len(items), "sum_s": sum(r["end_ns"] - r["start_ns"] for r in items) / 1e9,
                "union_s": busy / 1e9, "peak_overlap": peak}

    phases = {kind: intervals(items) for kind, items in sorted(by_kind.items())}
    roots = by_kind["root"]
    root_sum = sum(r["end_ns"] - r["start_ns"] for r in roots) / 1e9
    cancellations = by_kind["cancel_requested"]
    cancel_ns = min(r["start_ns"] for r in cancellations) if cancellations else None
    return {
        "complete": trace["dropped"] == 0, "dropped": trace["dropped"],
        "phase_intervals": phases,
        "mean_active_roots_over_observed_wall": root_sum / wall_s if wall_s else None,
        "longest_roots": sorted(roots, key=lambda r: r["end_ns"] - r["start_ns"], reverse=True)[:10],
        "cancel_requested_s": cancel_ns / 1e9 if cancel_ns is not None else None,
        "roots_active_at_cancel": sum(r["start_ns"] <= cancel_ns < r["end_ns"] for r in roots) if cancel_ns is not None else None,
        "post_cancel_tail_s_by_phase": {
            kind: max(0, max(r["end_ns"] for r in items) - cancel_ns) / 1e9
            for kind, items in sorted(by_kind.items()) if items and kind != "cancel_requested"
        } if cancel_ns is not None else None,
    }


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("directories", type=Path, nargs="+")
parser.add_argument("--output", type=Path, required=True)
parser.add_argument("--allow-incomplete", action="store_true", help="Accept verified capped runs only in explicit stress cohorts")
args = parser.parse_args()
rows = []
failures = []
for directory in args.directories:
    directory_rows = []
    failed_jobs_path = directory / "failed-jobs.json"
    if failed_jobs_path.exists():
        for job in json.loads(failed_jobs_path.read_text(encoding="utf-8-sig")):
            failures.append(f"Failed job: {job['name']}: {job['reason']}")
    csv_path = directory / "results.csv"
    if csv_path.exists():
        with csv_path.open(encoding="utf-8-sig", newline="") as source:
            for row in csv.DictReader(source):
                row["result"] = json.loads(Path(row["result_file"]).read_text(encoding="utf-8-sig"))
                row.setdefault("mode", "all")
                row.setdefault("variant", "after")
                rows.append(row)
                directory_rows.append(row)
    else:
        failures.append(f"No solver results: {directory}")
    hashes_path = directory / "binaries.json"
    if hashes_path.exists():
        hashes = json.loads(hashes_path.read_text(encoding="utf-8-sig"))
        if isinstance(hashes, dict):
            hashes = [hashes]
        for binary in hashes:
            try:
                digest = hashlib.sha256(Path(binary["path"]).read_bytes()).hexdigest()
                if digest.lower() != binary["sha256"].lower():
                    failures.append(f"Binary hash mismatch: {binary['path']}")
            except OSError as error:
                failures.append(f"Cannot verify binary: {binary['path']}: {error}")
        for row in directory_rows:
            if not any(b["variant"] == row["variant"] and b["case"] in (row["case"], "profile_case") for b in hashes):
                failures.append(f"Missing binary provenance: {row['result_file']}")
    schedule_path = directory / "schedule.json"
    if schedule_path.exists():
        schedule = json.loads(schedule_path.read_text(encoding="utf-8-sig"))
        if isinstance(schedule, dict):
            schedule = [schedule]
        if len(schedule) != len(directory_rows):
            failures.append(f"Unfinished schedule: {directory}, {len(directory_rows)}/{len(schedule)} runs")
        planned = Counter((str(j["Case"]), str(j.get("Mode", "all")), str(j.get("Variant", "after")), str(j["Stage"]), str(j["Workers"]), str(j["Repeat"])) for j in schedule)
        actual = Counter((r["case"], r["mode"], r["variant"], r["stage"], r["workers"], r.get("repeat")) for r in directory_rows)
        if planned != actual:
            failures.append(f"Schedule identity mismatch: {directory}")
        by_id = {(r["case"], r["mode"], r["variant"], r["stage"], r["workers"], r.get("repeat")): r for r in directory_rows}
        for job in schedule:
            identity = tuple(str(job.get(k, default)) for k, default in [("Case", ""), ("Mode", "all"), ("Variant", "after"), ("Stage", ""), ("Workers", ""), ("Repeat", "")])
            row = by_id.get(identity)
            if not row or "CaseFile" not in job:
                continue
            expected = json.loads((directory / "cases" / Path(job["CaseFile"]).name).read_text(encoding="utf-8-sig"))
            result = row["result"]
            def exact_problem(problem):
                return ([Fraction(v) for v in problem["inputs"]], [Fraction(v) for v in problem["outputs"]], Fraction(problem["maxLinkRate"]))
            if exact_problem(result["problem"]) != exact_problem(expected["problem"]):
                failures.append(f"Wrong exact problem: {row['result_file']}")
            for field, source in [("max_nodes", "MaxNodes"), ("timeout_s", "TimeoutSeconds")]:
                if str(result.get(field)) != str(job[source]) or str(row.get(field)) != str(job[source]):
                    failures.append(f"Wrong {field}: {row['result_file']}")
            if row.get("cohort") != job["Cohort"]:
                failures.append(f"Wrong cohort: {row['result_file']}")
            if "Hotspots" in job and (result.get("hotspot_recording") is not (job["Hotspots"] == "on") or row.get("hotspot_recording", "").lower() != str(job["Hotspots"] == "on").lower()):
                failures.append(f"Wrong hotspot setting: {row['result_file']}")
    identities = [(r["case"], r["mode"], r["variant"], r["stage"], r["workers"], r.get("repeat")) for r in directory_rows]
    if len(set(identities)) != len(identities):
        failures.append(f"Duplicate benchmark identities: {directory}")

def case_key(row):
    return row["case"], row["mode"], row.get("max_nodes", "legacy")


def classify(row):
    result = row["result"]
    outcome = result.get("outcome")
    if outcome is None:
        return "optimal" if result["status"].startswith("Optimal(") else "invalid"
    kind, value = outcome["kind"], outcome["result"]
    if kind == "optimal":
        return "optimal" if result["status"] == f"Optimal(N={value['nodeCount']}, L={value['linkCount']})" else "invalid"
    if kind == "globally_unsat":
        return "globally_unsat" if result["status"].startswith("GloballyUnsat(") else "invalid"
    if kind != "incomplete" or not result["status"].startswith("Incomplete("):
        return "invalid"
    reason = value["reason"]["kind"]
    proof = value["proof"]
    exhausted = proof.get("nodeCountsExhaustedThrough")
    # ResourceLimit is only a completed bounded proof if every requested N was exhausted.
    if reason == "resource_limit" and exhausted is not None and exhausted >= int(row["max_nodes"]) and value.get("bestKnown") is None:
        return "bounded_exhausted"
    if reason == "cancelled" and result.get("deadline_fired") is True:
        return "timed_out"
    return "invalid"


baselines = {}
for row in rows:
    row["completion"] = classify(row)
    if row["stage"] == "baseline" and row["completion"] == "optimal":
        key = case_key(row)
        if key not in baselines or row["variant"] == "before":
            baselines[key] = row["result"]
# A stress baseline may time out while another configuration completes.
for row in rows:
    if row.get("cohort") == "stress" and row["completion"] == "optimal":
        baselines.setdefault(case_key(row), row["result"])

problems = {}
known_nodes = {}
for row in rows:
    outcome = row["result"].get("outcome")
    if outcome:
        value = outcome["result"]
        witness = value if outcome["kind"] == "optimal" else value.get("bestKnown")
        if witness:
            known_nodes[row["case"]] = min(known_nodes.get(row["case"], witness["nodeCount"]), witness["nodeCount"])
groups = defaultdict(list)
for row in rows:
    result = row["result"]
    baseline = baselines.get(case_key(row))
    stress = args.allow_incomplete and row.get("cohort") == "stress"
    if row["completion"] == "invalid" or (not stress and row["completion"] != "optimal"):
        failures.append(f"Incomplete or failed run: {row['result_file']}")
    if baseline is None and not stress:
        failures.append(f"Missing completed baseline for {case_key(row)}")
    if row["status"] != result["status"] or result.get("mode", "all") != row["mode"]:
        failures.append(f"CSV/JSON status or mode mismatch: {row['result_file']}")
    if "hotspot_recording" in result and row.get("hotspot_recording", "").lower() != str(result["hotspot_recording"]).lower():
        failures.append(f"CSV/JSON hotspot setting mismatch: {row['result_file']}")
    for field in ("stage", "workers"):
        if field in result and str(result[field]) != str(row[field]):
            failures.append(f"CSV/JSON {field} mismatch: {row['result_file']}")
    if result["layouts"] != len(result["layout_keys"]) or result["layout_keys"] != sorted(set(result["layout_keys"])):
        failures.append(f"Malformed result-set accounting: {row['result_file']}")
    if result.get("validated", True) is not True:
        failures.append(f"Unvalidated witness: {row['result_file']}")
    if "problem" in result:
        problem = json.dumps(result["problem"], sort_keys=True)
        previous = problems.setdefault(row["case"], problem)
        if previous != problem:
            failures.append(f"Mixed exact problems: {row['case']}")
    if row["completion"] == "optimal":
        if baseline and any(result.get(k) != baseline.get(k) for k in ("status", "layout_keys", "preferred_key")):
            failures.append(f"Optimum or full layout-set/preferred witness mismatch: {row['result_file']}")
        if baseline and "solutions" in baseline and result.get("solutions") != baseline["solutions"]:
            failures.append(f"Saved solution objects differ: {row['result_file']}")
        if row["mode"] == "optimal" and (result["layouts"] != 1 or result["layout_keys"] != [result["preferred_key"]]):
            failures.append(f"Find-optimal did not return exactly its terminal witness: {row['result_file']}")
        exhaustive = baselines.get((row["case"], "all", row.get("max_nodes", "legacy")))
        if row["mode"] == "optimal" and exhaustive and (result["status"] != exhaustive["status"] or result["preferred_key"] not in exhaustive["layout_keys"]):
            failures.append(f"Optimal witness/optimum disagrees with full enumeration: {row['result_file']}")
    elif result.get("preferred_key") is not None:
        failures.append(f"Incomplete run claims an optimal preferred witness: {row['result_file']}")
    if row["completion"] == "timed_out" and row["mode"] == "all" and baseline:
        if not set(result["layout_keys"]).issubset(baseline["layout_keys"]):
            failures.append(f"Partial enumeration contains a non-reference layout: {row['result_file']}")
    outcome = result.get("outcome")
    if outcome:
        value = outcome["result"]
        best = value.get("bestKnown") if outcome["kind"] == "incomplete" else (value if outcome["kind"] == "optimal" else None)
        exhausted = value["proof"].get("nodeCountsExhaustedThrough")
        if best and exhausted is not None and best["nodeCount"] <= exhausted:
            failures.append(f"Witness contradicts exhausted bound: {row['result_file']}")
        if row["case"] in known_nodes and (row["completion"] == "globally_unsat" or exhausted is not None and known_nodes[row["case"]] <= exhausted):
            failures.append(f"Proof contradicts another run's validated witness: {row['result_file']}")
        if outcome["kind"] == "incomplete" and row["mode"] == "optimal":
            expected_keys = [bytes(best["canonicalGraphKey"]).hex()] if best else []
            if result["layout_keys"] != expected_keys:
                failures.append(f"Incomplete optimal run lost its best-known witness: {row['result_file']}")
        if row["completion"] in ("globally_unsat", "bounded_exhausted") and result["layouts"]:
            failures.append(f"UNSAT proof has layouts: {row['result_file']}")
    groups[row["case"], row["mode"], row["variant"], row["stage"], int(row["workers"]), row.get("max_nodes", "legacy"), row.get("timeout_s", "legacy")].append(row)

summary = []
def comparable_optimal_wall(samples, reference):
    """Only compare completed timings with identical instrumentation settings."""
    if not samples or not reference or not all(row["completion"] == "optimal" for row in samples + reference):
        return None
    if len({row.get("hotspot_recording", "legacy") for row in samples + reference}) != 1:
        return None
    return median(float(row["wall_s"]) for row in reference)


for (case, mode, variant, stage, workers, max_nodes, timeout_s), samples in sorted(groups.items()):
    if len({row.get("hotspot_recording", "legacy") for row in samples}) != 1:
        failures.append(f"Mixed profiling settings: {case} {mode} {variant} {stage} w{workers}")
    observed_wall = [float(row["wall_s"]) for row in samples]
    activity = []
    for row in samples:
        try:
            activity.append(activity_summary(row["result"].get("activity"), float(row["wall_s"])))
        except (KeyError, TypeError, ValueError) as error:
            failures.append(f"Malformed activity trace: {row['result_file']}: {error}")
    all_optimal = all(row["completion"] == "optimal" for row in samples)
    wall = observed_wall if all_optimal else []
    completion_counts = dict(Counter(row["completion"] for row in samples))
    baseline_samples = groups.get((case, mode, variant, "baseline", workers, max_nodes, timeout_s), [])
    baseline_wall = comparable_optimal_wall(samples, baseline_samples)
    single_samples = groups.get((case, mode, variant, stage, 1, max_nodes, timeout_s), [])
    single_wall = comparable_optimal_wall(samples, single_samples)
    diagnostics = [{item["name"]: int(item["value"]["value"]) for item in (row["result"]["diagnostics"] or []) if item["value"]["type"] == "integer"} for row in samples]
    before = groups.get((case, mode, "before", stage, workers, max_nodes, timeout_s), [])
    before_wall = comparable_optimal_wall(samples, before)
    first_valid = [float(row["first_valid_s"]) for row in samples if row.get("first_valid_s")]
    summary.append({
        "case": case, "mode": mode, "variant": variant, "stage": stage, "workers": workers, "runs": len(samples),
        "max_nodes": max_nodes, "timeout_s": timeout_s, "completion_counts": completion_counts,
        "median_wall_s": median(wall) if wall else None, "min_wall_s": min(wall) if wall else None, "max_wall_s": max(wall) if wall else None,
        "median_observed_wall_s": median(observed_wall),
        "activity": activity,
        "median_bounded_exhaustion_s": median(observed_wall) if all(r["completion"] == "bounded_exhausted" for r in samples) else None,
        "proof_progress": [{"completion": r["completion"], "proof": (r["result"].get("outcome") or {}).get("result", {}).get("proof"), "last_progress": r["result"].get("last_progress"), "layouts": r["result"]["layouts"]} for r in samples],
        "speedup_vs_same_workers": baseline_wall / median(wall) if baseline_wall and wall else None,
        "same_stage_speedup_from_one_worker": single_wall / median(wall) if single_wall and wall else None,
        "parallel_efficiency": single_wall / median(wall) / workers if single_wall and wall else None,
        "kernel_speedup_same_stage": before_wall / median(wall) if before_wall and wall else None,
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
    print(f"{item['case']} {item['mode']} {item['variant']} {item['stage']} w{item['workers']}: {item['median_observed_wall_s']:.3f}s observed, {item['completion_counts']}, n={item['runs']}")
if failures:
    raise SystemExit("\n".join(failures))
print(f"Verified {len(rows)} records. Completed results match available references; capped stress runs remain explicitly incomplete.")
