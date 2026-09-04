"""Generate the experiment 57 hotspot-on cost-mix profile of production after 54."""

from __future__ import annotations

import json
from pathlib import Path

CCD96 = 100_663_296
CCD32 = 33_554_432
GRACE = 15


def job(
    *,
    case: str,
    case_file: str,
    mode: str,
    workers: int,
    repeat: int,
    timeout: int,
    max_nodes: int,
    cohort: str = "reference",
    pair_role: str | None = "reference",
    affinity: str | None = None,
    cache: int | None = None,
) -> dict:
    record = {
        "Case": case,
        "CaseFile": case_file,
        "Mode": mode,
        "Stage": "p1",
        "Variant": "after",
        "Workers": workers,
        "Repeat": repeat,
        "TimeoutSeconds": timeout,
        "MaxNodes": max_nodes,
        "Cohort": cohort,
        "Hotspots": "on",
    }
    if pair_role is not None:
        record["PairRole"] = pair_role
    if affinity is not None:
        record["ProcessorAffinity"] = affinity
    if cache is not None:
        record["CacheBytes"] = cache
    return record


CELLS = [
    {
        "case": "post54_r115_ccd96",
        "file": "cases/cyclic115.json",
        "mode": "all",
        "workers": 16,
        "timeout": 90,
        "max_nodes": 10,
        "affinity": "ffff",
        "cache": CCD96,
        "repeats": 2,
    },
    {
        "case": "post54_r238_ccd32",
        "file": "cases/cyclic238.json",
        "mode": "optimal",
        "workers": 16,
        "timeout": 25,
        "max_nodes": 10,
        "affinity": "ffff0000",
        "cache": CCD32,
        "repeats": 2,
    },
    {
        "case": "post54_r258_ccd96",
        "file": "cases/medium258.json",
        "mode": "minimum_links",
        "workers": 16,
        "timeout": 450,
        "max_nodes": 12,
        "affinity": "ffff",
        "cache": CCD96,
        "repeats": 2,
    },
    {
        "case": "post54_r36_allcpu",
        "file": "cases/acyclic36.json",
        "mode": "optimal",
        "workers": 32,
        "timeout": 50,
        "max_nodes": 9,
        "repeats": 2,
    },
    {
        "case": "post54_r115_ccd32",
        "file": "cases/cyclic115.json",
        "mode": "all",
        "workers": 16,
        "timeout": 100,
        "max_nodes": 10,
        "affinity": "ffff0000",
        "cache": CCD32,
        "repeats": 1,
    },
    {
        "case": "post54_r238_ccd96",
        "file": "cases/cyclic238.json",
        "mode": "optimal",
        "workers": 16,
        "timeout": 25,
        "max_nodes": 10,
        "affinity": "ffff",
        "cache": CCD96,
        "repeats": 1,
    },
]

STRESS = [
    {
        "case": "post54_diag36_ccd96",
        "file": "cases/acyclic36.json",
        "mode": "all",
        "workers": 16,
        "timeout": 90,
        "max_nodes": 9,
        "affinity": "ffff",
        "cache": CCD96,
        "repeats": 1,
        "cohort": "stress",
    },
    {
        "case": "post54_diag10_ccd32",
        "file": "cases/cyclic10.json",
        "mode": "optimal",
        "workers": 16,
        "timeout": 90,
        "max_nodes": 11,
        "affinity": "ffff0000",
        "cache": CCD32,
        "repeats": 1,
        "cohort": "stress",
    },
]


def main() -> None:
    jobs: list[dict] = []
    for cell in CELLS:
        for repeat in range(1, cell["repeats"] + 1):
            jobs.append(
                job(
                    case=cell["case"],
                    case_file=cell["file"],
                    mode=cell["mode"],
                    workers=cell["workers"],
                    repeat=repeat,
                    timeout=cell["timeout"],
                    max_nodes=cell["max_nodes"],
                    affinity=cell.get("affinity"),
                    cache=cell.get("cache"),
                )
            )
    for cell in STRESS:
        jobs.append(
            job(
                case=cell["case"],
                case_file=cell["file"],
                mode=cell["mode"],
                workers=cell["workers"],
                repeat=1,
                timeout=cell["timeout"],
                max_nodes=cell["max_nodes"],
                cohort="stress",
                pair_role=None,
                affinity=cell.get("affinity"),
                cache=cell.get("cache"),
            )
        )
    search = sum(int(item["TimeoutSeconds"]) for item in jobs)
    cleanup = len(jobs) * GRACE
    budget = 1800
    total = search + cleanup
    assert total <= budget, (search, cleanup, total)
    manifest = {
        "description": (
            "Experiment57: hotspot-on cost-mix profile of production p1 after experiment54 "
            "commit d6902a4. Single current binary as Variant after. PairRole reference on "
            "completed cells so the analyzer treats them as exact-result baselines; do not use "
            "Variant before. Not an optimization A/B. Hotspot-on enables nested timers and "
            "activity traces. Completed 115 all, 238 optimal, 258 minimum-links and 36 optimal "
            "must finish and match public results. Stress 36-all and 10-optimal may cap; no "
            "completion-speedup claim. Do not compare these walls with hotspot-off timings. "
            "Question: after Bareiss-forward, does sparse preparation still own enough wall to "
            "justify prep-template reuse? 12 jobs: "
            f"{search}s search + {cleanup}s cleanup = {total}s at {GRACE}s grace, inside one hour."
        ),
        "MaxScheduledSeconds": budget,
        "jobs": jobs,
    }
    out = Path(__file__).resolve().parents[1] / "benchmarks/custom/post54-cost-profile.json"
    out.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(f"wrote {out} jobs={len(jobs)} search={search} cleanup={cleanup} total={total}")


if __name__ == "__main__":
    main()
