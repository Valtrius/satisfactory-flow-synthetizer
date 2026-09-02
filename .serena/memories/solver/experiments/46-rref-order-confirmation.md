# 46. Focused RREF confirmation and longer 258 control

Date: 2026-09-02. State: prepared and plan-validated; not launched.
Approved follow-up to `mem:solver/experiments/46-rref-order-results`.
The original 40-job screen and failure marker remain unchanged.

## Questions and controls

Repeat the small RREF gains on 115/238, add completed RREF/258 coverage, and
recheck the adverse variable-discovery/258 cell without its former 320s cap.
No new solver implementation. RREF remains active/unpromoted in a79ecf7.
Variables and accounting remain permanent. Scheduler and production affinity
unchanged. No cyclic/acyclic label informs solver behavior.

Reuse all four retained executables, with their source/build metadata unchanged.
Reference/candidate definitions isolate RREF from accounting and variables.
No new Rust tests/build necessary for this manifest-only follow-up.

## Plan and time limit

Manifest benchmarks/custom/rref-order-confirmation-screening.json.
Map target/parallelism-ladder/rref-order-variants-20260902/variants.json.
Intended run target/parallelism-ladder/rref-order-confirmation-20260902.
20 jobs, ten adjacent pairs, exact AB/BA balance in each cell.
All capacity 1200, p1, hotspots off, 16 workers, required optimal completions.

| Comparison          | Workload          | Placement        | Pairs | Per-job cap | Max N |
| ------------------- | ----------------- | ---------------- | ----: | ----------: | ----: |
| accounting -> rref  | 115 all           | CCD96 / ffff     |     2 |         60s |    10 |
| accounting -> rref  | 238 optimal       | CCD32 / ffff0000 |     4 |         15s |    10 |
| accounting -> rref  | 258 minimum_links | CCD96 / ffff     |     2 |        260s |    12 |
| before -> variables | 258 minimum_links | CCD32 / ffff0000 |     2 |        400s |    12 |

Search caps total 3000s; 15s cleanup per job adds 300s.
MaxScheduledSeconds=3300, 55 minutes. Five minutes remain within the user's
one-hour limit for startup, process-launch overhead and verification.
The 400s cap gives the previously capped comparison 80 additional seconds.
This is a new run, not a resumed partial search or revised old cap.
Unrestricted accounting and 36 repeats are omitted to respect the budget.

## Identity and validation

Frozen binaries and every metadata-listed source hash reverified before planning:
before 8b2dd2768976469ba74617cf5e0d31e72324cf2b41e831f8473777dc837d2ea2;
variables b19b601f29e8f9bed6b3427a89152c376466ab68408e0b201d87ad384f594acf;
accounting 253b8245c9106211ee1a0a44f006bffc246138b0cb59ef2e895bfb4f1b6b1573;
rref a431c5ce82fe435c75831a7d4403e84e9c5c6e53353ba04bcde05a40e00678d7.

Frozen 20-job plan passes current topology/cache-domain/worker/pair checks.
Independent schedule check confirms adjacency, exact order balance and 3300s
search+cleanup sum. Evidence: target/exp46-confirmation-plan.log and
target/parallelism-ladder/rref-order-confirmation-plan-20260902/input.
The preceding 221 solver-core/330 workspace tests, strict Clippy, 41 tooling
tests and 12 real-executable smokes apply to these unchanged binaries/tools;
they were not rerun or claimed as new validation in this preparation turn.

## Interpretation and next step

No timing result yet. Keep placements and instrumentation separate. New 258/CCD96
RREF pairs measure accounting versus rref, not before versus rref. The CCD32
variable control has a new cap; preserve its cohort separately from the original
failed 320s run. All reference-cohort jobs must finish; preserve any cap/failure.

Two-pair cells remain small. Combine conclusions with prior exact matched
evidence carefully, without silently pooling raw times across sessions or
excluding adverse samples. No promotion or regression dismissal before analysis.
Freeze and launch with completion/failure dialog, then end the turn.
Manifest/results/handoff documentation being committed; no push.
