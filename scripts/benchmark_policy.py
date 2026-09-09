"""Evidence checks and paired analysis for topology-controlled benchmark cohorts."""
from collections import defaultdict
import math
import random
from statistics import median


PAIR_FIELDS = ("Case", "Mode", "Stage", "Workers", "Repeat", "MaxNodes", "TimeoutSeconds",
               "Hotspots", "AstraDiagnostics", "ProcessorAffinity", "CacheBytes", "Comparison", "Cohort", "CaseFile")


def validate_pairs(schedule):
    paired = [job for job in schedule if job.get("PairId")]
    if not paired:
        return {}
    if len(paired) != len(schedule):
        raise ValueError("Do not mix paired and unpaired jobs in a schedule")
    groups = defaultdict(list)
    for job in paired:
        groups[job["PairId"]].append(job)
    for pair, jobs in groups.items():
        if len(jobs) != 2 or {j.get("PairRole") for j in jobs} != {"reference", "candidate"}:
            raise ValueError(f"Pair must have one reference and one candidate: {pair}")
        if jobs[0]["Variant"] == jobs[1]["Variant"] or any(jobs[0].get(k) != jobs[1].get(k) for k in PAIR_FIELDS):
            raise ValueError(f"Unmatched pair settings: {pair}")
    # Pair members must stay adjacent even when pair order is randomized.
    for at in range(0, len(schedule), 2):
        if schedule[at]["PairId"] != schedule[at + 1]["PairId"]:
            raise ValueError("Pair members are not adjacent")
    return groups


def validate_placement(job, row, topology):
    expected = (job.get("ProcessorAffinity") or "").lower()
    if expected != (row.get("processor_affinity") or "").lower():
        raise ValueError("Processor affinity differs from the schedule")
    if not expected and not job.get("PairId"):
        return
    if row.get("affinity_before_resume", "").lower() != "true":
        raise ValueError("Missing before-resume affinity evidence")
    if row.get("affinity_observed") != (expected or topology["available_mask"]):
        raise ValueError("Observed affinity differs from requested placement")
    if not 0 <= float(row["affinity_applied_s"]) <= float(row["process_wall_s"]):
        raise ValueError("Invalid affinity setup time")
    mask = int(expected or topology["available_mask"], 16)
    if int(job["Workers"]) > mask.bit_count():
        raise ValueError("Worker count exceeds selected logical CPUs")
    if job.get("CacheBytes") is not None:
        matching = [c for c in topology["relationships"] if c["relation"] == 2 and c["level"] == 3
                    and c["cache_bytes"] == job["CacheBytes"]
                    and c["groups"] == [{"group": 0, "mask": expected}]]
        if not matching:
            raise ValueError("Selected cache domain differs from the recorded topology")
    for source, target in (("PairId", "pair_id"), ("PairRole", "pair_role"), ("Comparison", "comparison")):
        if job.get(source, "") != row.get(target, ""):
            raise ValueError(f"Changed {source}")


def paired_summary(rows):
    pairs = defaultdict(list)
    for row in rows:
        if row.get("pair_id"):
            pairs[(row["run_directory"], row["pair_id"])].append(row)
    groups = defaultdict(list)
    for pair, entries in pairs.items():
        if len(entries) != 2 or {r["pair_role"] for r in entries} != {"reference", "candidate"}:
            raise ValueError(f"Missing paired result: {pair}")
        before = next(r for r in entries if r["pair_role"] == "reference")
        after = next(r for r in entries if r["pair_role"] == "candidate")
        fields = ("comparison", "case", "mode", "stage", "workers", "max_nodes", "timeout_s", "processor_affinity", "hotspot_recording")
        if any(before.get(k) != after.get(k) for k in fields):
            raise ValueError(f"Mismatched paired result: {pair}")
        key = tuple(before.get(k, "") for k in fields) + (before["variant"], after["variant"])
        groups[key].append((before, after))
    output = []
    for key, entries in sorted(groups.items()):
        item = dict(zip(fields + ("reference_variant", "candidate_variant"), key), pairs=len(entries))
        complete = all(r["completion"] == "optimal" for pair in entries for r in pair)
        item["all_completed"] = complete
        for metric in ("wall_s", "first_valid_s", "process_cpu_s"):
            ratios = []
            if complete:
                for before, after in entries:
                    a, b = before.get(metric), after.get(metric)
                    if a in (None, "") or b in (None, "") or not 0 < float(a) or not 0 < float(b):
                        break
                    ratios.append(float(b) / float(a))
            if len(ratios) != len(entries) or not all(math.isfinite(v) for v in ratios):
                item[metric] = None
                continue
            rng = random.Random(902044)
            boot = sorted(median(rng.choices(ratios, k=len(ratios))) for _ in range(5000))
            item[metric] = dict(candidate_over_reference=ratios,
                median_change_pct=100 * (median(ratios) - 1),
                bootstrap_95_pct=[100 * (boot[125] - 1), 100 * (boot[4874] - 1)])
        output.append(item)
    return output


def result_comparison_errors(result, baseline, mode, policy="ordered"):
    """Ordering freedom never removes full enumeration or objective checks."""
    if policy not in ("ordered", "any_optimum"):
        raise ValueError(f"Unknown result policy: {policy}")
    errors = []
    if result.get("status") != baseline.get("status"):
        errors.append("Optimum differs")
    if policy == "ordered" or mode not in ("optimal", "one_min_nl"):
        if result.get("layout_keys") != baseline.get("layout_keys"):
            errors.append("Full layout set differs")
        if "solutions" in baseline and result.get("solutions") != baseline["solutions"]:
            errors.append("Saved solution objects differ")
    if policy == "ordered" and result.get("preferred_key") != baseline.get("preferred_key"):
        errors.append("Preferred witness differs")
    return errors
