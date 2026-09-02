# 48. Negative-unit confirmation

Date: 2026-09-02. User approved the confirmation recommendation.
Prepared and validated, not launched yet.
Initial measured results: `mem:solver/experiments/48-rref-elimination-shortcuts-results`.

## Question and scope

Do the small negative-unit completion gains repeat without a sustained regression?
Initial screen improved 7/8 completion pairs and all 8 CPU pairs, but had only
two pairs per workload. 258 completion was effectively unchanged; 115 first
witness was mixed. CPU savings alone do not establish the desired wall benefit.

No source implementation or rebuild. Use the identical before/minus executables
and source snapshots from the initial screen. Zero-destination remains held;
do not combine it or change production scheduling.

## Protocol and budget

Manifest: `benchmarks/custom/rref-negative-unit-confirmation.json`.
Variant map:
`target/parallelism-ladder/rref-elimination-variants-20260902/confirmation-variants.json`.
New run:
`target/parallelism-ladder/rref-negative-unit-confirmation-20260902`.

32 jobs, 16 adjacent matched pairs. Four new pairs per workload, repeats 3 through 6,
with two reference-first and two candidate-first pairs in each case.
Randomized recorded pair order; same caps/settings as initial screen.
All jobs are reference cohort and must complete. All p1, capacity 1200, hotspots OFF.

| Case / mode       | Workers / placement                        | Max N | Cap per job |
| ----------------- | ------------------------------------------ | ----: | ----------: |
| 115 all           | 16, CCD96 mask ffff, L3 100663296 bytes    |    10 |         65s |
| 238 optimal       | 16, CCD32 mask ffff0000, L3 33554432 bytes |    10 |         15s |
| 258 minimum_links | 16, CCD96 mask ffff, L3 100663296 bytes    |    12 |        245s |
| 36 optimal        | 32, unrestricted                           |     9 |         25s |

2800s search + 32*15s cancellation cleanup = 3280s, 54m40.
Reserve 320s for startup and verification inside one hour. No hard10 caps in
this completion screen. No production affinity, cyclicity or memory gate.

Analyze new pairs separately first. A secondary combined view may use the two
initial pairs plus four confirmation pairs only with identical case/settings/
binary identity, keeping screen identity and placement explicit.
Do not compare cross-CCD medians or call a capped job a completion improvement.
Retain first witness, full solve, CPU and sampled memory as separate metrics.

## Frozen identities and validation

Before SHA256:
`353d5863592991bcd86531248b996dd8c0f31383c99df97e2fc066fba35483ed`.
Minus SHA256:
`6465d5dca89b185c37a2b471428fa3732ede69a03fe6afb77ae0aa48b2d9037e`.
Both binaries byte-match the first run. Rechecked all 72 before and 73 minus
source/metadata/binary/patch hashes from their original manifests.

Prior source-level validation remains applicable to these unchanged executables:
candidate release suites, strict Clippy, exact matrix oracles and 15 CLI smokes
are recorded in `mem:solver/experiments/48-rref-elimination-shortcuts`.
No fresh Rust build or test suite is needed for a manifest-only confirmation.
The current runner/analyzer's 41 tests pass.

PlanOnly passed and queried live topology for requested cache domains.
Independent plan audit verifies 32 jobs, 16 adjacent pairs, four repetitions,
two AB/two BA per workload, placements, exact budget and all 192 frozen hashes.
Plan: `target/parallelism-ladder/rref-negative-unit-confirmation-plan-20260902`.
Helpers/logs: `target/exp48-confirmation-prepare.py`,
`target/exp48-confirmation-preparation.json`,
`target/exp48-confirmation-audit-plan.py`,
`target/exp48-confirmation-plan.log`,
`target/exp48-confirmation-tool-tests.log`.

## Commit and launch handoff

Neither arithmetic candidate is promoted. Existing preparation fadb316 and
earlier results da2c89f remain; unrelated history commit 6683546 is preserved.
This manifest and related Serena analysis/handoff will be committed after formatting.
No push. After launch record PID/time and live status, then end turn.
Windows completion/failure dialog plus BENCHMARK-STATUS/FINISHED/FAILED files
provide notification. No builds or tests during timed jobs.
