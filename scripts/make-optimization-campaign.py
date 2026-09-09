"""Generate matched screens from explicit cases and policies; this never launches solves."""
import argparse
import json
from pathlib import Path
import shutil

REPO = Path(__file__).resolve().parents[1]
MODES = ("one_min_nl", "all_min_nl", "all_min_n")
CAPS = dict(tiny=2, acyclic24=7, acyclic36=9, cyclic10=14, cyclic115=7,
            cyclic238=8, cyclic65=6, medium258=12, **{"mixed-huge": 3})
EXTRA = {
    "rational-fifths": (["5/7"], ["2/7", "2/7", "1/7"], "6/7", 3),
    "surplus-multi": (["2", "1"], ["1", "1"], "3", 2),
    "capacity-mixed": (["2", "3"], ["1", "4"], "5", 2),
    "cyclic-sevenths": (["7"], ["3", "2", "2"], "9", 5),
    "surplus-fifths": (["5"], ["2", "1"], "6", 3),
    "four-outputs": (["8"], ["1", "1", "2", "4"], "8", 4),
    "ratio97": (["97"], ["61", "36"], "1200", 12),
    "scaled258": (["258/7"], ["195/7", "9"], "1200/7", 12),
}
HARD = [("cyclic10", "one_min_nl"), ("medium258", "all_min_nl"),
        ("medium258", "one_min_nl"), ("acyclic36", "all_min_n"),
        ("cyclic115", "all_min_n"), ("cyclic238", "all_min_n")]
PARTITION_GUARDS = [("cyclic10", "one_min_nl"), ("medium258", "one_min_nl"), ("acyclic36", "all_min_n")]


def timeout(case, mode, family):
    if case in ("cyclic10", "ratio97"):
        return 180 if family != "workers" else 120
    if case in ("medium258", "scaled258") and mode == "all_min_nl":
        return 120
    if case in CAPS and case not in ("tiny", "mixed-huge"):
        return 30
    return 20


def generate(output, finalists=None):
    output.mkdir(parents=True, exist_ok=False)
    cases = output / "cases"
    cases.mkdir()
    for name in CAPS:
        shutil.copy2(REPO / "benchmarks/cases" / f"{name}.json", cases)
    for name, (inputs, outputs, capacity, _) in EXTRA.items():
        (cases / f"{name}.json").write_text(json.dumps(dict(name=name, problem=dict(inputs=inputs, outputs=outputs, maxLinkRate=capacity)), indent=2) + "\n")
    caps = dict(CAPS, **{name: row[3] for name, row in EXTRA.items()})
    suites = []

    def screen(name, candidate, scopes, workers=(32,), repeats=2, family="screen", diagnostics=False):
        jobs = []
        for case, mode in scopes:
            for worker in workers:
                for repeat in range(1, repeats + 1):
                    pair = f"{name}-{case}-{mode}-w{worker}-r{repeat}"
                    for role, variant in (("reference", "baseline"), ("candidate", candidate)):
                        jobs.append(dict(Case=case, CaseFile=f"cases/{case}.json", Mode=mode,
                                         Stage="baseline", Workers=worker, Repeat=repeat,
                                         Variant=variant, MaxNodes=caps[case],
                                         TimeoutSeconds=timeout(case, mode, family), Hotspots="off",
                                         Cohort="reference" if case in ("tiny", "surplus-multi", "capacity-mixed") else "stress",
                                         Diagnostics=diagnostics, Comparison=name, PairId=pair, PairRole=role))
        allowance = sum(j["TimeoutSeconds"] + 15 for j in jobs)
        if allowance > 28800:
            raise ValueError(f"Split suite exceeding eight scheduled hours: {name}")
        manifest = dict(RunnerProtocol="layout-v1", ResultPolicy="any_optimum", MaxScheduledSeconds=allowance, jobs=jobs)
        (output / f"{name}.json").write_text(json.dumps(manifest, indent=2) + "\n")
        suites.append(dict(name=name, candidate=candidate, family=family, jobs=len(jobs), pairs=len(jobs)//2,
                           repeats=repeats, diagnostics=diagnostics, max_scheduled_seconds=allowance,
                           manifest=f"{name}.json"))

    if finalists:
        allowed = json.loads((REPO / "benchmarks/optimization/variants.json").read_text())
        for candidate in finalists:
            if candidate == "baseline" or candidate not in allowed:
                raise ValueError(f"Invalid finalist: {candidate}")
            scopes = [(name, "all_min_nl") for name in caps] + PARTITION_GUARDS if candidate.startswith("pairs-") else HARD + [(name, "all_min_nl") for name in EXTRA]
            screen(f"confirm-{candidate}", candidate, scopes, repeats=6, family="confirmation")
            screen(f"confirm-workers-{candidate}", candidate,
                   [("cyclic10", "one_min_nl"), ("medium258", "all_min_nl"), ("acyclic36", "all_min_n")],
                   workers=(1, 2, 8, 16), repeats=4, family="workers")
    else:
        # Scope-gated candidates are checked on every corpus case and independent additions.
        for candidate in ("pairs-both", "pairs-sparse", "pairs-boolean"):
            screen(candidate, candidate, [(case, "all_min_nl") for case in caps] + PARTITION_GUARDS, family="partitions")
        for candidate in ("sparse25", "sparse75"):
            screen(candidate, candidate, HARD + [("tiny", mode) for mode in MODES] + [("ratio97", "one_min_nl")], family="allocation")
        for candidate in ("delay-sparse250", "delay-boolean250"):
            screen(candidate, candidate, HARD + [(case, mode) for case in ("tiny", "surplus-multi", "capacity-mixed") for mode in MODES], family="startup")
        # Same total workers within every pair; cross-worker timings are descriptive only.
        for candidate in ("sparse-only", "boolean-only"):
            screen(f"workers-{candidate}", candidate,
                   [("cyclic10", "one_min_nl"), ("medium258", "all_min_nl"), ("acyclic36", "all_min_n")],
                   workers=(1, 2, 8, 16, 32), repeats=2, family="workers")
        # Diagnostics are separate paired observations, never folded into timing cohorts.
        for candidate in ("pairs-both", "sparse25", "sparse75"):
            screen(f"roots-{candidate}", candidate,
                   [("cyclic10", "one_min_nl"), ("medium258", "all_min_nl"), ("acyclic36", "all_min_n")],
                   repeats=1, family="diagnostics", diagnostics=True)
    if not finalists:
        priority = ["pairs-both", "sparse25", "sparse75", "pairs-sparse", "pairs-boolean",
                    "delay-sparse250", "delay-boolean250", "workers-sparse-only", "workers-boolean-only",
                    "roots-pairs-both", "roots-sparse25", "roots-sparse75"]
        suites.sort(key=lambda suite: priority.index(suite["name"]))
    campaign = dict(suites=suites, cancellation_grace_seconds=15, prepared_only=True,
                    total_jobs=sum(s["jobs"] for s in suites), total_pairs=sum(s["pairs"] for s in suites),
                    max_scheduled_seconds=sum(s["max_scheduled_seconds"] for s in suites),
                    corpus=list(CAPS), additional_cases=list(EXTRA),
                    independent_holdouts=[name for name in EXTRA if name != "scaled258"],
                    scale_invariance_cases=["scaled258"])
    (output / "campaign.json").write_text(json.dumps(campaign, indent=2) + "\n")
    return campaign


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--finalists", nargs="+")
    args = parser.parse_args()
    print(json.dumps(generate(args.output, args.finalists), indent=2))
