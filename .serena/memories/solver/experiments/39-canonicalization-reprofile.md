# 39. Canonicalization reprofile after kernel promotions

Date: 2026-09-01. State: completed and verified.
Related: original purpose profile (`mem:solver/experiments/24-canonical-purpose-profile`),
deferred state keys (`mem:solver/experiments/29-deferred-state-canonicalization`), and
mixed-row promotion (`mem:solver/experiments/38-mixed-row-integer-substitution-results`).
Results: verified reprofile (`mem:solver/experiments/39-canonicalization-reprofile-results`).

## Question

Which canonicalization purpose and subphase now dominate after the marked-child DFS
bypass, state-coordinate reuse, deferred state keys, propagation cleanup, and sparse
substitution promotions?

The 2026-08-29 purpose profile predates those changes. Reusing its conclusion would
target a distribution that the solver no longer executes.

## Measurement

No solver source or scheduling policy changes. The retained hotspot recorder already
separates:

- state, open-port, marked-link, other graph, and SCC canonicalization purposes;
- incidence construction, dense graph construction, labeling, relabeling, equality,
  inequality, snapshot, and semantic encoding subphases;
- the top-level state-key, legal-decision, propagation, SCC, reachability, and complete
  evaluation buckets.

These aggregate worker timers overlap by design. Purpose and subphase totals explain
the top-level buckets; they must not be added as independent CPU time. Hotspot atomics
also add diagnostic overhead, so this run does not replace hotspot-off completion
timings.

## Frozen screen

Manifest: `benchmarks/custom/canonical-purpose-screening.json`. Reusing the original
four workloads preserves historical scope:

- 115 all, p1/32, N<=12, 180-second cap, expected complete;
- 238 all, p1/32, N<=12, 90-second cap, expected complete;
- hard 36 all, p1/32, N<=9, 60-second cap;
- hard 10 optimal, p1/32, N<=11, 60-second cap.

Every job records hotspots and process samples. The 390-second aggregate cap keeps the
screen manageable. The hard cases may remain incomplete and can establish cost shares
only. Corpus cyclic/acyclic labels do not enter solver policy.

Production reference commit: `1fd478e`. Current `profile_case.exe` SHA-256:
`6618B0B55768D9E275A82BA0446EA7DE7F2799F21CD26CE390899C07AC022CA1`.
Run directory: `target/parallelism-ladder/canonicalization-reprofile-20260901`.

## Validation and interpretation

Experiment 38's exhaustive rational oracle, solver-core, Reference, parallelism,
workspace, Clippy, analyzer, formatting, release build, and exact tiny smoke checks
all pass for this source and binary. A fresh hotspot-on tiny smoke is also optimal and
validated. Its ten marked-link, four open-port, and one other graph calls sum to all
15 graph calls, and every retained subphase counter records successfully.

Completed outputs must validate and match available canonical layout sets. Capped
jobs must retain honest incomplete proofs and no unvalidated witness. Do not infer
completion speed from work reached inside a cap.

## Decision rule

Choose the next calculation experiment only from the new dominant purpose and
subphase. If labeling remains dominant, split the labeler's refinement and search
work. If graph or semantic construction dominates, test reuse within one immutable
state. If canonicalization is no longer the largest actionable bucket, stop and move
to the measured leader. Scheduling stays paused.

## Result

All four records verify. State keys now account for 99.96-100.00% of graph-purpose
time. Equality and inequality encoding consume 58.9-67.2% of combined state and SCC
canonicalization, while labeling consumes only 13.8-16.4% of recorded subphase time.
See the full results (`mem:solver/experiments/39-canonicalization-reprofile-results`). Split equality and
inequality internals before attempting another calculation change.
