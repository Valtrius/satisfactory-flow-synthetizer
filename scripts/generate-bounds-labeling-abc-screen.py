"""Generate the experiment 58 isolated dirty-bounds vs color-labeling A/B/C screen."""

from __future__ import annotations

import json
from pathlib import Path

CCD96 = 100_663_296
CCD32 = 33_554_432
GRACE = 15
COMPARISONS = (
    ("dirty_bounds", "bounds"),
    ("color_labeling", "labeling"),
)
SEED = 58


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
        "stem": "r258_ccd96",
        "file": "cases/medium258.json",
        "mode": "minimum_links",
        "workers": 16,
        "timeout": 300,
        "max_nodes": 12,
        "affinity": "ffff",
        "cache": CCD96,
        "pairs": 2,
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
    {
        "stem": "r115_ccd32",
        "file": "cases/cyclic115.json",
        "mode": "all",
        "workers": 16,
        "timeout": 100,
        "max_nodes": 10,
        "affinity": "ffff0000",
        "cache": CCD32,
        "pairs": 2,
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
        "pairs": 2,
    },
    {
        "stem": "tiny_ccd96",
        "file": "cases/tiny.json",
        "mode": "all",
        "workers": 16,
        "timeout": 5,
        "max_nodes": 2,
        "affinity": "ffff",
        "cache": CCD96,
        "pairs": 1,
    },
]

STRESS_CELLS = [
    {
        "stem": "diag36_ccd96",
        "file": "cases/acyclic36.json",
        "mode": "all",
        "workers": 16,
        "timeout": 90,
        "max_nodes": 9,
        "affinity": "ffff",
        "cache": CCD96,
        "pairs": 1,
        "cohort": "stress",
    },
    {
        "stem": "diag10_ccd32",
        "file": "cases/cyclic10.json",
        "mode": "optimal",
        "workers": 16,
        "timeout": 90,
        "max_nodes": 11,
        "affinity": "ffff0000",
        "cache": CCD32,
        "pairs": 1,
        "cohort": "stress",
    },
]


def main() -> None:
    jobs: list[dict] = []
    for comparison, candidate in COMPARISONS:
        for cell in OFF_CELLS + STRESS_CELLS:
            for repeat in range(1, cell["pairs"] + 1):
                jobs.extend(pair_jobs(cell, repeat, comparison, candidate))
    search = sum(int(job["TimeoutSeconds"]) for job in jobs)
    cleanup = len(jobs) * GRACE
    budget = 7200
    assert search + cleanup <= budget, (search, cleanup, search + cleanup)
    manifest = {
        "description": (
            "Experiment58: isolated dirty-component bound scan and isolated BaseColor/labeling-cache "
            "candidates vs production p1 after experiment54. Two comparisons share the same before "
            "binary: dirty_bounds (before vs bounds) and color_labeling (before vs labeling). Candidates "
            "are not combined. Three pairs on primary 115/238/36, two on 258 and cross-cache, one tiny "
            "and one stress pair each for hard 36-all and 10-optimal. Hotspot-off timing. 72 jobs. "
            f"Search {search}s + cleanup {cleanup}s = {search + cleanup}s under two hours."
        ),
        "MaxScheduledSeconds": budget,
        "jobs": jobs,
    }
    out = Path(__file__).resolve().parents[1] / "benchmarks/custom/bounds-labeling-abc-screen.json"
    out.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(f"wrote {out} jobs={len(jobs)} search={search} cleanup={cleanup} total={search + cleanup}")


if __name__ == "__main__":
    main()
