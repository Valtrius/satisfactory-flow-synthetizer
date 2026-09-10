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


def generate(output, finalists=None, followup=False, promotion=False):
    if sum((bool(finalists), followup, promotion)) > 1:
        raise ValueError("Choose finalists, follow-up, or promotion")
    output.mkdir(parents=True, exist_ok=False)
    cases = output / "cases"
    cases.mkdir()
    for name in CAPS:
        shutil.copy2(REPO / "benchmarks/cases" / f"{name}.json", cases)
    for name, (inputs, outputs, capacity, _) in EXTRA.items():
        (cases / f"{name}.json").write_text(json.dumps(dict(name=name, problem=dict(inputs=inputs, outputs=outputs, maxLinkRate=capacity)), indent=2) + "\n")
    caps = dict(CAPS, **{name: row[3] for name, row in EXTRA.items()})
    suites = []

    def screen(name, candidate, scopes, workers=(32,), repeats=2, family="screen", diagnostics=False, limit_s=None):
        if any(worker < 8 for worker in workers):
            raise ValueError("Performance campaigns require at least eight total workers")
        jobs = []
        for case, mode in scopes:
            seconds = limit_s[(case, mode)] if isinstance(limit_s, dict) else limit_s or timeout(case, mode, family)
            for worker in workers:
                for repeat in range(1, repeats + 1):
                    pair = f"{name}-{case}-{mode}-w{worker}-r{repeat}"
                    for role, variant in (("reference", "baseline"), ("candidate", candidate)):
                        jobs.append(dict(Case=case, CaseFile=f"cases/{case}.json", Mode=mode,
                                         Stage="baseline", Workers=worker, Repeat=repeat,
                                         Variant=variant, MaxNodes=caps[case],
                                         TimeoutSeconds=seconds, Hotspots="off",
                                         Cohort="reference" if case in ("tiny", "surplus-multi", "capacity-mixed") else "stress",
                                         Diagnostics=diagnostics, Comparison=name, PairId=pair, PairRole=role))
        allowance = sum(j["TimeoutSeconds"] + 15 for j in jobs)
        if allowance + 120 > 10800:
            # Keep complete paired repeats together when broader confirmation
            # templates need more than one bounded suite.
            if len(scopes) > 1:
                middle = len(scopes) // 2
                for part, subset in enumerate((scopes[:middle], scopes[middle:]), 1):
                    screen(f"{name}-part{part}", candidate, subset, workers, repeats, family, diagnostics, limit_s)
                return
            if len(workers) > 1:
                for worker in workers:
                    screen(f"{name}-w{worker}", candidate, scopes, (worker,), repeats, family, diagnostics, limit_s)
                return
            raise ValueError(f"Split suite exceeding three hours including verification: {name}")
        manifest = dict(RunnerProtocol="layout-v1", ResultPolicy="any_optimum", MaxScheduledSeconds=allowance, jobs=jobs)
        (output / f"{name}.json").write_text(json.dumps(manifest, indent=2) + "\n")
        suites.append(dict(name=name, candidate=candidate, family=family, jobs=len(jobs), pairs=len(jobs)//2,
                           repeats=repeats, diagnostics=diagnostics, max_scheduled_seconds=allowance,
                           manifest=f"{name}.json"))

    if promotion:
        # Both runners start from the promoted descending-order revision. The
        # candidate adds only the Boolean minimum-link partition policy.
        screen("promotion-guards8", "pairs-boolean", [("cyclic10", "one_min_nl")],
               workers=(8,), limit_s=45)
        guards16 = [("cyclic10", "one_min_nl"), ("ratio97", "one_min_nl"), ("acyclic36", "all_min_nl")]
        screen("promotion-guards16", "pairs-boolean", guards16, workers=(16,),
               limit_s=dict(zip(guards16, (45, 20, 15), strict=True)))
        screen("promotion-guards32", "pairs-boolean",
               [("acyclic36", "all_min_nl"), ("acyclic36", "all_min_n"),
                ("acyclic24", "all_min_nl"), ("cyclic65", "all_min_nl")], limit_s=15)
        screen("promotion-confirm258", "pairs-boolean", [("medium258", "all_min_nl")],
               repeats=6, limit_s=90, family="confirmation")
        screen("promotion-confirm97", "pairs-boolean", [("ratio97", "all_min_nl")],
               repeats=6, limit_s=600, family="confirmation")
    elif followup:
        key_scopes = [("cyclic10", "one_min_nl"), ("medium258", "all_min_nl"),
                      ("acyclic36", "all_min_nl"), ("acyclic36", "all_min_n")]
        # Six repeats where the completed discovery showed a worker-budget cliff.
        for candidate in ("order-outside-in", "order-reverse"):
            screen(f"confirm16-{candidate}", candidate, key_scopes, workers=(16,), repeats=6, family="workers")
        partition_core = [("medium258", "all_min_nl"), ("acyclic36", "all_min_nl"),
                          ("acyclic24", "all_min_nl"), ("cyclic115", "all_min_nl")] + PARTITION_GUARDS
        screen("adaptive-boolean-core", "adaptive-boolean", partition_core, family="partitions")
        screen("confirm-pairs-boolean-core", "pairs-boolean", partition_core, repeats=6, family="confirmation")
        screen("confirm-delay-sparse250-hard", "delay-sparse250", HARD, repeats=6, family="confirmation")
        small = [(case, mode) for case in ("tiny", "surplus-multi", "capacity-mixed", "rational-fifths", "cyclic-sevenths") for mode in MODES]
        screen("confirm-delay-sparse250-small", "delay-sparse250", small, repeats=6, family="confirmation")
        for candidate in ("order-outside-in", "order-reverse"):
            screen(f"workers-{candidate}", candidate, key_scopes, workers=(8,12,24,32), family="workers")
            holds = [(case,"all_min_nl") for case in caps if case not in ("cyclic10","ratio97","medium258","scaled258","acyclic36")]
            screen(f"coverage-{candidate}", candidate, holds + [("ratio97","one_min_nl")], workers=(16,), family="workers")
        remaining = [(case,"all_min_nl") for case in caps if case not in ("cyclic10","ratio97","medium258","acyclic36","acyclic24","cyclic115")]
        screen("adaptive-boolean-coverage", "adaptive-boolean", remaining, family="partitions")
        screen("confirm-pairs-boolean-coverage", "pairs-boolean", remaining, repeats=6, family="confirmation")
        screen("workers-adaptive-boolean", "adaptive-boolean", key_scopes, workers=(8,16), family="workers")
        for workers in (8,16):
            screen(f"workers{workers}-pairs-boolean", "pairs-boolean", key_scopes, workers=(workers,), repeats=4, family="workers")
        for workers in ((8,16),):
            screen(f"workers{workers[0]}-{workers[1]}-delay-sparse250", "delay-sparse250", HARD[:2]+[("acyclic36","all_min_n")], workers=workers, repeats=4, family="workers")
        # Diagnostic runs are separate from every completion-time comparison.
        for candidate in ("pairs-boolean", "adaptive-boolean"):
            screen(f"roots-{candidate}", candidate, key_scopes, repeats=1, family="diagnostics", diagnostics=True)
        for candidate in ("order-outside-in", "order-reverse"):
            screen(f"roots-{candidate}", candidate, key_scopes[:2], workers=(8,16), repeats=1, family="workers", diagnostics=True)
        unresolved = [("cyclic10","all_min_nl"),("ratio97","all_min_nl")]
        screen("adaptive-boolean-capped", "adaptive-boolean", unresolved, family="partitions")
        # Long enumeration is last; the campaign guard preserves the queue if it cannot fit.
        screen("long-enumeration-pairs-boolean", "pairs-boolean", unresolved, limit_s=600, family="long-enumeration")
        screen("roots-long-enumeration-pairs-boolean", "pairs-boolean", unresolved, repeats=1, limit_s=600, family="diagnostics", diagnostics=True)
    elif finalists:
        allowed = json.loads((REPO / "benchmarks/optimization/variants.json").read_text())
        for candidate in finalists:
            if candidate == "baseline" or candidate not in allowed:
                raise ValueError(f"Invalid finalist: {candidate}")
            scopes = [(name, "all_min_nl") for name in caps] + PARTITION_GUARDS if candidate.startswith("pairs-") else HARD + [(name, "all_min_nl") for name in EXTRA]
            screen(f"confirm-{candidate}", candidate, scopes, repeats=6, family="confirmation")
            screen(f"confirm-workers-{candidate}", candidate,
                   [("cyclic10", "one_min_nl"), ("medium258", "all_min_nl"), ("acyclic36", "all_min_n")],
                   workers=(8, 16, 32), repeats=4, family="workers")
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
                   workers=(8, 16, 32), repeats=2, family="workers")
        # Diagnostics are separate paired observations, never folded into timing cohorts.
        for candidate in ("pairs-both", "sparse25", "sparse75"):
            screen(f"roots-{candidate}", candidate,
                   [("cyclic10", "one_min_nl"), ("medium258", "all_min_nl"), ("acyclic36", "all_min_n")],
                   repeats=1, family="diagnostics", diagnostics=True)
    if not finalists and not followup and not promotion:
        priority = ["pairs-both", "sparse25", "sparse75", "pairs-sparse", "pairs-boolean",
                    "delay-sparse250", "delay-boolean250", "workers-sparse-only", "workers-boolean-only",
                    "roots-pairs-both", "roots-sparse25", "roots-sparse75"]
        suites.sort(key=lambda suite: priority.index(suite["name"]))
    if followup:
        priority = ["confirm16-order-outside-in", "confirm16-order-reverse", "adaptive-boolean-core",
                    "roots-order-outside-in", "roots-order-reverse", "roots-adaptive-boolean", "roots-pairs-boolean",
                    "confirm-pairs-boolean-core", "confirm-delay-sparse250-hard"]
        suites.sort(key=lambda suite: priority.index(suite["name"]) if suite["name"] in priority else len(priority))
    campaign = dict(suites=suites, cancellation_grace_seconds=15, prepared_only=True,
                    runtime_budget_seconds=10800, suite_overhead_seconds=120,
                    minimum_workers=8,
                    total_jobs=sum(s["jobs"] for s in suites), total_pairs=sum(s["pairs"] for s in suites),
                    max_scheduled_seconds=sum(s["max_scheduled_seconds"] for s in suites),
                    corpus=list(CAPS), additional_cases=list(EXTRA),
                    independent_holdouts=[name for name in EXTRA if name != "scaled258"],
                    scale_invariance_cases=["scaled258"])
    if promotion:
        campaign["purpose"] = "Qualify Boolean partitions on the promoted descending-order baseline"
        campaign["max_session_seconds"] = campaign["max_scheduled_seconds"] + len(suites) * campaign["suite_overhead_seconds"]
        if campaign["max_session_seconds"] > campaign["runtime_budget_seconds"]:
            raise ValueError("Promotion queue exceeds the three-hour session budget")
    (output / "campaign.json").write_text(json.dumps(campaign, indent=2) + "\n")
    return campaign


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--finalists", nargs="+")
    parser.add_argument("--followup", action="store_true")
    parser.add_argument("--promotion", action="store_true")
    args = parser.parse_args()
    print(json.dumps(generate(args.output, args.finalists, args.followup, args.promotion), indent=2))
