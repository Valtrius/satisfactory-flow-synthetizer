"""Generate the experiment 62 remaining-port L-bound A/B/C screen."""

from __future__ import annotations

import json
from pathlib import Path

CCD96 = 100_663_296
CCD32 = 33_554_432
GRACE = 15
COMPARISONS = (
    ("under_l_places", "places"),
    ("forced_remaining_l", "forced"),
)
SEED = 62


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


def pair_jobs(cell: dict, repeat: int, comparison: str, candidate: str) -> list[dict]:
    pair_id = f"{comparison}-{cell['stem']}-{cell['mode']}-r{repeat}"
    order = ("before", candidate) if repeat % 2 == 1 else (candidate, "before")
    jobs = []
    for variant in order:
        jobs.append(
            job(
                case=f"{comparison}_{cell['stem']}",
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


OFF_CELLS = [
    {
        "stem": "r115_ccd96",
        "file": "cases/cyclic115.json",
        "mode": "all",
        "workers": 16,
        "timeout": 90,
        "max_nodes": 10,
        "affinity": "ffff",
        "cache": CCD96,
        "pairs": 3,
    },
    {
        "stem": "r115_ccd32",
        "file": "cases/cyclic115.json",
        "mode": "all",
        "workers": 16,
        "timeout": 100,
        "max_nodes": 10,
        "affinity": "ffff0000",
        "cache": CCD32,
        "pairs": 3,
    },
    {
        "stem": "r238_ccd96",
        "file": "cases/cyclic238.json",
        "mode": "optimal",
        "workers": 16,
        "timeout": 25,
        "max_nodes": 10,
        "affinity": "ffff",
        "cache": CCD96,
        "pairs": 3,
    },
    {
        "stem": "r238_ccd32",
        "file": "cases/cyclic238.json",
        "mode": "optimal",
        "workers": 16,
        "timeout": 25,
        "max_nodes": 10,
        "affinity": "ffff0000",
        "cache": CCD32,
        "pairs": 3,
    },
    {
        "stem": "r36_allcpu",
        "file": "cases/acyclic36.json",
        "mode": "optimal",
        "workers": 32,
        "timeout": 50,
        "max_nodes": 9,
        "pairs": 3,
    },
]


def main() -> None:
    jobs: list[dict] = []
    for comparison, candidate in COMPARISONS:
        for cell in OFF_CELLS:
            for repeat in range(1, cell["pairs"] + 1):
                jobs.extend(pair_jobs(cell, repeat, comparison, candidate))
    search = sum(int(job["TimeoutSeconds"]) for job in jobs)
    cleanup = len(jobs) * GRACE
    budget = 7200
    assert search + cleanup <= budget, (search, cleanup, search + cleanup)
    manifest = {
        "description": (
            "Experiment62: isolated remaining-port under-L placement vs isolated forced remaining-L "
            "lower bound, each vs production p1 after experiment60. Two comparisons share the same "
            "before binary: under_l_places (before vs places) and forced_remaining_l (before vs forced). "
            "Candidates are not combined. places also checks under-L in prepare_applied_child, "
            "search_state, and therefore prefix replay/planner. forced adds current L plus remaining "
            "node-port lower bound > expected L before prepare. Three AB/BA pairs on 115 all and 238 "
            "optimal at both CCDs, and 36 optimal all-CPU. Hotspot-off timing. 60 jobs. 258 omitted. "
            f"Search {search}s + cleanup {cleanup}s = {search + cleanup}s under two hours."
        ),
        "MaxScheduledSeconds": budget,
        "jobs": jobs,
    }
    out = Path(__file__).resolve().parents[1] / "benchmarks/custom/l-bound-abc-screen.json"
    out.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(f"wrote {out} jobs={len(jobs)} search={search} cleanup={cleanup} total={search + cleanup}")


if __name__ == "__main__":
    main()
