"""Generate the experiment 52 tail-help topology-controlled screen manifest."""

from __future__ import annotations

import json
import random
from pathlib import Path

CCD96 = 100_663_296
CCD32 = 33_554_432
GRACE = 15
COMPARISON = "tail_help"
PROFILE_COMPARISON = "tail_help_profile"
SEED = 52


def job(
    *,
    case: str,
    case_file: str,
    mode: str,
    variant: str,
    workers: int,
    repeat: int,
    timeout: int,
    max_nodes: int,
    pair_id: str,
    role: str,
    hotspots: str,
    comparison: str,
    cohort: str = "reference",
    affinity: str | None = None,
    cache: int | None = None,
) -> dict:
    record = {
        "Case": case,
        "CaseFile": case_file,
        "Mode": mode,
        "Stage": "p1",
        "Variant": variant,
        "Workers": workers,
        "Repeat": repeat,
        "TimeoutSeconds": timeout,
        "MaxNodes": max_nodes,
        "Cohort": cohort,
        "Hotspots": hotspots,
        "Comparison": comparison,
        "PairId": pair_id,
        "PairRole": role,
    }
    if affinity is not None:
        record["ProcessorAffinity"] = affinity
    if cache is not None:
        record["CacheBytes"] = cache
    return record


def pair_jobs(cell: dict, repeat: int, comparison: str) -> list[dict]:
    pair_id = f"{comparison}-{cell['stem']}-{cell['mode']}-r{repeat}"
    order = ("before", "after") if repeat % 2 == 1 else ("after", "before")
    jobs = []
    for variant in order:
        jobs.append(
            job(
                case=cell["case"],
                case_file=cell["file"],
                mode=cell["mode"],
                variant=variant,
                workers=cell["workers"],
                repeat=repeat,
                timeout=cell["timeout"],
                max_nodes=cell["max_nodes"],
                pair_id=pair_id,
                role="reference" if variant == "before" else "candidate",
                hotspots=cell.get("hotspots", "off"),
                comparison=comparison,
                cohort=cell.get("cohort", "reference"),
                affinity=cell.get("affinity"),
                cache=cell.get("cache"),
            )
        )
    return jobs


def expand(cell: dict, comparison: str) -> list[list[dict]]:
    return [pair_jobs(cell, repeat, comparison) for repeat in range(1, cell["pairs"] + 1)]


OFF_CELLS = [
    {
        "case": "tail_r115_ccd96",
        "stem": "r115_ccd96",
        "file": "cases/cyclic115.json",
        "mode": "all",
        "workers": 16,
        "timeout": 75,
        "max_nodes": 10,
        "affinity": "ffff",
        "cache": CCD96,
        "pairs": 16,
    },
    {
        "case": "tail_r238_ccd32",
        "stem": "r238_ccd32",
        "file": "cases/cyclic238.json",
        "mode": "optimal",
        "workers": 16,
        "timeout": 20,
        "max_nodes": 10,
        "affinity": "ffff0000",
        "cache": CCD32,
        "pairs": 16,
    },
    {
        "case": "tail_r258_ccd96",
        "stem": "r258_ccd96",
        "file": "cases/medium258.json",
        "mode": "minimum_links",
        "workers": 16,
        "timeout": 250,
        "max_nodes": 10,
        "affinity": "ffff",
        "cache": CCD96,
        "pairs": 16,
    },
    {
        "case": "tail_r36_allcpu",
        "stem": "r36_allcpu",
        "file": "cases/acyclic36.json",
        "mode": "optimal",
        "workers": 32,
        "timeout": 30,
        "max_nodes": 9,
        "pairs": 16,
    },
    {
        "case": "tail_r115_ccd32",
        "stem": "r115_ccd32",
        "file": "cases/cyclic115.json",
        "mode": "all",
        "workers": 16,
        "timeout": 90,
        "max_nodes": 10,
        "affinity": "ffff0000",
        "cache": CCD32,
        "pairs": 8,
    },
    {
        "case": "tail_r238_ccd96",
        "stem": "r238_ccd96",
        "file": "cases/cyclic238.json",
        "mode": "optimal",
        "workers": 16,
        "timeout": 20,
        "max_nodes": 10,
        "affinity": "ffff",
        "cache": CCD96,
        "pairs": 8,
    },
    {
        "case": "tail_tiny_ccd96",
        "stem": "tiny_ccd96",
        "file": "cases/tiny.json",
        "mode": "all",
        "workers": 16,
        "timeout": 5,
        "max_nodes": 2,
        "affinity": "ffff",
        "cache": CCD96,
        "pairs": 8,
    },
    {
        "case": "tail_huge_ccd96",
        "stem": "huge_ccd96",
        "file": "cases/mixed-huge.json",
        "mode": "all",
        "workers": 16,
        "timeout": 5,
        "max_nodes": 10,
        "affinity": "ffff",
        "cache": CCD96,
        "pairs": 8,
    },
]

ON_CELLS = [
    {
        "case": "tail_diag115_ccd96",
        "stem": "diag115_ccd96",
        "file": "cases/cyclic115.json",
        "mode": "all",
        "workers": 16,
        "timeout": 80,
        "max_nodes": 10,
        "affinity": "ffff",
        "cache": CCD96,
        "pairs": 1,
        "hotspots": "on",
    },
    {
        "case": "tail_diag238_ccd32",
        "stem": "diag238_ccd32",
        "file": "cases/cyclic238.json",
        "mode": "optimal",
        "workers": 16,
        "timeout": 20,
        "max_nodes": 10,
        "affinity": "ffff0000",
        "cache": CCD32,
        "pairs": 1,
        "hotspots": "on",
    },
    {
        "case": "tail_diag36_ccd96",
        "stem": "diag36_ccd96",
        "file": "cases/acyclic36.json",
        "mode": "all",
        "workers": 16,
        "timeout": 90,
        "max_nodes": 9,
        "affinity": "ffff",
        "cache": CCD96,
        "pairs": 1,
        "hotspots": "on",
        "cohort": "stress",
    },
    {
        "case": "tail_diag10_ccd32",
        "stem": "diag10_ccd32",
        "file": "cases/cyclic10.json",
        "mode": "optimal",
        "workers": 16,
        "timeout": 90,
        "max_nodes": 11,
        "affinity": "ffff0000",
        "cache": CCD32,
        "pairs": 1,
        "hotspots": "on",
        "cohort": "stress",
    },
]


def main() -> None:
    pairs: list[list[dict]] = []
    for cell in OFF_CELLS:
        pairs.extend(expand(cell, COMPARISON))
    for cell in ON_CELLS:
        pairs.extend(expand(cell, PROFILE_COMPARISON))
    rng = random.Random(SEED)
    rng.shuffle(pairs)
    jobs = [job for pair in pairs for job in pair]
    search_s = sum(job["TimeoutSeconds"] for job in jobs)
    cleanup_s = len(jobs) * GRACE
    budget = search_s + cleanup_s
    description = (
        "Experiment52: paired topology-controlled comparison of production p1 after "
        "experiment51 skip+borrowed-verdict versus the same tree plus tail-only sibling "
        "help with private deferred caches. before keeps current p1 (workers exit when "
        "the claim queue is empty). after donates DFS siblings only once every whole "
        "root is claimed, without shared_state_cache or public work_stealing. Sixteen "
        "paired repeats on primary 115/all CCD96, 238/optimal CCD32, 258/minimum_links "
        "CCD96 and 36/optimal unrestricted; eight on cross-cache 115 CCD32 and 238 CCD96 "
        "plus tiny/huge controls. Hotspot-OFF whole-solve timing is primary; four ON "
        "diagnostic pairs are separate. Stress 36-all and 10-optimal may cap, with no "
        "completion speedup claim. All other jobs must finish and match full public "
        "results, layout-key sets, preferred witnesses and proof. AB/BA balanced within "
        "OFF cells; fixed-seed random pair order. User authorized up to eight hours. "
        f"{len(jobs)} jobs: {search_s}s search + {cleanup_s}s cleanup = {budget}s "
        f"({budget / 3600:.2f}h) at 15s cancellation grace. Retain every adverse pair; "
        "small effects require uncertainty reporting and no automatic promotion."
    )
    payload = {
        "description": description,
        "MaxScheduledSeconds": budget,
        "jobs": jobs,
    }
    out = Path(__file__).resolve().parents[1] / "benchmarks" / "custom" / "tail-help-screen.json"
    out.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    print(f"wrote {out} jobs={len(jobs)} budget={budget}s")


if __name__ == "__main__":
    main()
