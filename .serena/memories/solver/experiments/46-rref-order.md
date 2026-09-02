# 46. Canonical RREF ordering without sorting

Date: 2026-09-02. State: implemented and correctness-validated; benchmark not launched.
User approved prior recommendations and limited the next benchmark to one hour.
Prior promotions: `mem:solver/experiments/44-topology-variable-confirmation-results`
and `mem:solver/experiments/45-cache-accounting-results`, recorded in 6812173.
Existing source commits 520b352 and 331b87a are now permanent; no push.

## Hypothesis and change

Canonical rational_rref scans pivot columns from left to right and makes each
pivot +1. Retained nonzero rows have distinct pivots in ascending column order.
At the first differing column, an earlier row has 1 and a later row 0. Reversing
these rows therefore gives the same lexicographic order as sorting. Duplicate
rows cannot survive distinct pivots. Empty systems and the single canonical
contradiction are unchanged.

Replace only final rows.sort()/dedup() with rows.reverse() in canonical.rs.
No elimination arithmetic, identity encoding, inequalities, pruning, cache,
scheduler or proof change. Candidate active in source, not promoted.
Existing dense reference remains untouched and still sorts/deduplicates.
Extend its generated test to zero-variable systems and reversed/scaled generators,
with dependent and zero rows. Existing contradiction, large coefficient and exact
sparse encoding checks remain. Focused generated oracle passes.

## Focused plan

Manifest: benchmarks/custom/rref-order-hour-screening.json.
40 jobs, 20 adjacent pairs. All capacity 1200, p1, hotspots off and optimal
completion required. All comparisons use an even pair count for exact AB/BA balance.
Search caps 2520s + 15s cleanup per job = 3120s, 52 minutes. Manifest guard 3120s,
leaving eight minutes within the user's hour for startup and verification.

| Comparison              | Workload          | Placement/workers | Pairs | Per-job cap |
| ----------------------- | ----------------- | ----------------- | ----: | ----------: |
| accounting -> rref      | 115 all           | CCD96 / 16        |     4 |         60s |
| accounting -> rref      | 238 optimal       | CCD32 / 16        |     4 |         15s |
| accounting -> rref      | 36 optimal        | unrestricted / 32 |     2 |         25s |
| before -> variables     | 258 minimum_links | CCD32 / 16        |     2 |        320s |
| before -> variables     | 115 all           | unrestricted / 32 |     2 |         45s |
| before -> variables     | 36 optimal        | CCD32 / 16        |     2 |         20s |
| variables -> accounting | 115 all           | unrestricted / 32 |     2 |         45s |
| variables -> accounting | 36 optimal        | unrestricted / 32 |     2 |         25s |

Max N10 for 115/238, N9 for 36, N12 for 258. The three retained variable controls
are the adverse cells from experiment 44, not a new solver gate.
Frozen before/variables/accounting executables will be reused unchanged.
The new rref binary differs only by the ordering shortcut and its tests.

The one-hour limit reduces this to an initial RREF screen and small regression
spot checks, not a final statistical confirmation. No hard10 or full hard36 all.
RREF/258 and accounting/unrestricted258 are deferred to keep this run bounded.
Do not pool CPU placements or add unrelated percentage gains.

## Validation and results

Focused generated dense oracle passes: 192 generated systems plus 192
permuted/scaled equivalents, including zero-variable, empty, dependent and
contradictory systems. Original oracle output order and sparse encoding checks
remain authoritative.

221 solver-core all-target tests with bench-internals pass, two ignored.
330 workspace all-target tests pass, five ignored. Both strict all-target
solver-core Clippy configurations pass. All 41 runner/analyzer tests pass.
Release profile_case builds with default features in 44.89s using the unchanged
VS2019/znver4 environment. Formatting and diff checks pass.
Logs: target/exp46-oracle.log, exp46-core-tests.log, exp46-workspace-tests.log,
exp46-clippy-feature.log, exp46-clippy-default.log, exp46-tool-tests.log,
exp46-build.log and exp46-format.log.
Frozen provenance verified: 70 source files, only canonical.rs differs from the
retained accounting source. All 12 frozen real-executable tiny smokes pass, with
exact optimal/minimum_links outputs on both CCDs and unrestricted execution.
Full 40-job frozen plan passes; every comparison has exactly balanced AB/BA order.
The 3120s search+cleanup sum is independently checked. No performance result.
An initial patch used an incorrect trailing function anchor and failed before
changing source; reapplied with the exact observed context. No runtime failure.
Complete/failure notification required; end turn immediately after launch.

## Frozen identity and prelaunch status

RREF candidate source commit a79ecf7, not promoted. Promotions recorded in 6812173.
New profile_case SHA256:
a431c5ce82fe435c75831a7d4403e84e9c5c6e53353ba04bcde05a40e00678d7.
Map: target/parallelism-ladder/rref-order-variants-20260902/variants.json.
Source/build metadata, source hashes and exact executable are alongside that map.
Frozen references remain identical to experiment 44:
before 8b2dd2768976469ba74617cf5e0d31e72324cf2b41e831f8473777dc837d2ea2;
variables b19b601f29e8f9bed6b3427a89152c376466ab68408e0b201d87ad384f594acf;
accounting 253b8245c9106211ee1a0a44f006bffc246138b0cb59ef2e895bfb4f1b6b1573.

Plan evidence: target/exp46-plan.log and rref-order-hour-plan-20260902/input.
Smoke evidence: target/exp46-smoke-analysis.log and rref-order-smoke-20260902/results.
Intended real output: target/parallelism-ladder/rref-order-hour-20260902.
Launch with start-benchmark-screen.ps1, the above manifest/map and
-CancellationGraceSeconds 15. Completion/failure dialog enabled by wrapper.
No run launched yet; no performance conclusion. Benchmark manifest and handoff
are being committed separately. No push.
