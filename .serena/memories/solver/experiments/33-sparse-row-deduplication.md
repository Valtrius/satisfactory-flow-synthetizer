# 33. Deduplicate normalized sparse rows

Date: 2026-08-31. State: rejected; source restored.

## Hypothesis

Experiment 32 made sparse analysis 6.79% more expensive per hard fixed-point pass
because weighted projection rebuilt rational rows. Retain only its smallest exact
operation: remove identical primitive rows after the substitution and sort already
performed by `SparseSystem`.

Rust slice deduplication is linear after sorting. If repeated constraints occur, this
avoids redundant Bareiss work without constructing projections or changing variables.
If duplicates are rare, the expected overhead is one adjacent equality scan per row.

## Candidate

- Substitute known exact values exactly as production already does.
- Normalize rows and remove tautologies exactly as production already does.
- Sort rows exactly as production already does, then call `dedup()`.
- Retain one copy of an explicit contradiction; never remove its constraint class.
- Leave weighted union-find, deductions, SCC construction, bounds, DFS, caches, proof
  accounting, validation, and p1 scheduling unchanged.
- Feature-gated telemetry counts input rows and duplicate rows removed in propagation.

Duplicate equations do not change a linear system's solution set, coefficient rank, or
augmented rank. Removing copies therefore preserves every valid deduction and
contradiction. Two focused tests compare repeated and unique systems and verify that
duplicate contradiction rows remain inconsistent.

## Validation

- 181 default solver-core library tests pass; two manual benchmarks are ignored.
- With `bench-internals`, 186 library tests pass and two are ignored. The exhaustive
  outer Reference differential and all four parallelism integrations pass.
- All workspace tests pass with five ignored.
- Strict solver-core all-target Clippy passes with and without `bench-internals`.
- All 33 analyzer tests pass.

These checks establish exactness, not performance or the prevalence of duplicates.

## Frozen screen

Manifest: `benchmarks/custom/sparse-dedup-screening.json`.

The randomized fresh-process screen contains 26 jobs:

- two samples per variant for 36 optimal;
- two samples per variant for 115 all L and minimum L;
- two samples per variant for 238 optimal, minimum L, and all L;
- one 60-second hard-10 diagnostic per variant with hotspots enabled.

The committed experiment-31 kernel is the reference. Frozen reference
`profile_case.exe` SHA-256:
`5254F6F526286FEDDE9F92AFC023B213BD122491EDCED7505682902F213D2FE4`.

The plan-only runner accepted all 26 jobs. Candidate `profile_case.exe` SHA-256:
`720ABA29C4E3739EEB95002C737DC1AF8F4FFDDDCE16D534C04BACEAAB983F2A`.
Run directory: `target/parallelism-ladder/sparse-dedup-screen-20260831`.

Completed pairs must preserve every exact result identity and structural counter. The
hard pair supplies fixed-time throughput, sparse timing, and duplicate-row prevalence;
it supplies no completion claim. The detached runner freezes all inputs and provides a
completion dialog.
Do not infer a result until its frozen analyzer reports completion.

## Decision gate

Promote if exact and structural verification passes and repeated completed workloads
are neutral or faster without a consistent regression. A zero or negligible duplicate
count rejects further work on this path even if noisy timings happen to favor it.
Scheduling remains paused.

## Results

The detached runner finished and the frozen analyzer verifies all 26 records with zero
failures. Twenty-four runs complete optimally; both hard-10 diagnostics cancel cleanly
at the common cap. No process failed or was killed. Every completed pair preserves all
exact result identities and every checked structural counter.

Each completed row has two fresh processes per variant:

| Workload      |    Before median (range) |     After median (range) |  Wall change |
| ------------- | -----------------------: | -----------------------: | -----------: |
| 36 optimal    | 13.485 (13.440-13.531) s | 13.452 (13.070-13.834) s | 0.25% faster |
| 115 all L     | 43.236 (43.226-43.245) s | 42.419 (42.005-42.834) s | 1.89% faster |
| 115 minimum L |    3.225 (3.218-3.232) s |    3.133 (3.107-3.159) s | 2.86% faster |
| 238 all L     | 17.431 (16.878-17.985) s | 17.749 (17.437-18.060) s | 1.82% slower |
| 238 minimum L |    8.975 (8.887-9.064) s |    8.666 (8.552-8.781) s | 3.44% faster |
| 238 optimal   |    8.637 (8.619-8.654) s |    8.653 (8.646-8.660) s | 0.19% slower |

CPU changes range from 1.82% lower to 1.14% higher. The two-sample wall differences
are small and mixed.

The hard candidate examines 164,042,086 input-row instances across 4,061,174
fixed-point passes and removes exactly zero duplicate rows. Its 89.021 microseconds of
sparse time per pass versus the reference's 90.522 microseconds is not attributable to
deduplication because no row was removed. Capped decisions differ with parallel timing
and supply no completion claim.

Summary SHA-256:
`5D8A9A40B2CE2820CC24B48DB7799651A081A7430A97EEFF983C14A7D7D11858`.

## Decision

Reject sparse-row deduplication. It finds no duplicate propagation rows in the hard
sample and the completed timings are neutral/mixed. The candidate source and its
telemetry were restored before this result record.

Stop pursuing duplicate equations. The next sparse diagnostic should bucket analysis
time and pass counts by matrix row/variable size and distinguish first analysis from
reanalyzes after a deduction. That evidence can determine whether a rollback-aware
incremental basis is worth its complexity. Scheduling remains paused.
