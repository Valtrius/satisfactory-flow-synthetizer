# 34. Sparse phase and matrix-shape profile

Date: 2026-08-31. State: completed; see results (`mem:solver/experiments/34-sparse-phase-profile-results`).

## Question

Experiments 32 and 33 rejected broad weighted substitution and row deduplication.
Before designing a rollback-aware incremental basis, measure where sparse analysis
spends time and which matrix shapes incur it.

The diagnostic asks:

- how time divides between substitution/sorting, Bareiss forward elimination, back
  reduction, and deduction extraction;
- how many analyses are first passes versus reanalyses after new values or ratios;
- how total analysis time varies with active row count, unknown-variable count, and
  nonzero-term count.

This uses only facts the solver already has during propagation. It does not use the
benchmark's cyclic or acyclic label.

## Instrumentation

Ordinary solves still call `SparseSystem::analyze_over` and do not read the clock.
When the existing hotspot recorder is enabled, propagation calls a profiled equivalent
that returns the same `SparseAnalysis` and adds diagnostic counters.

The profile records:

- input and post-substitution active rows, variables, and nonzero terms;
- nanoseconds for preparation, forward elimination, back reduction, and deductions;
- initial, value-triggered, ratio-triggered, and combined reanalysis counts;
- calls and total analysis nanoseconds in four row buckets, four variable buckets,
  and four nonzero-term buckets.

The size buckets use 0-15, 16-31, 32-47, and 48+ for rows and variables. Term
buckets use 0-63, 64-127, 128-191, and 192+. JSON output remains flat so the existing
analyzer can aggregate every field as a number.

## Validation

- 180 default solver-core library tests pass; two manual benchmarks are ignored.
- With `bench-internals`, 185 library tests pass and two are ignored. The exhaustive
  outer Reference differential and all four parallelism integrations pass.
- All workspace tests pass with five ignored.
- Strict solver-core all-target Clippy passes with and without `bench-internals`.
- All 33 analyzer tests pass. Repository formatting and the release build pass.
- A completed hotspot smoke run preserved the exact result. It recorded 25 sparse
  calls: 21 initial and four value-triggered. Row, variable, and term bucket call
  totals each equal the 25 profiled calls.

The first benchmark-feature build exceeded `serde_json::json!`'s macro recursion
limit after adding the new fields. The serializer now builds the established object
and inserts the diagnostic fields iteratively. This keeps the required flat JSON and
the subsequent full validation passes.

## Frozen screen

Manifest: `benchmarks/custom/sparse-phase-profile.json`.

The six fresh-process jobs pair the uninstrumented reference and diagnostic build:

- hard 10 optimal, p1/32, N<=11, 60-second cap;
- completed 115 all-L enumeration, p1/32, N<=12, 120-second cap;
- completed 238 optimal, p1/32, N<=12, 60-second cap.

All jobs enable existing hotspot recording. The reference is commit `2f5f043`.
Frozen reference `profile_case.exe` SHA-256:
`7D72873978AB420D879B4648EE5188C656F214AD470789AF7F83A2940E720E30`.
Candidate `profile_case.exe` SHA-256:
`F30EF85776418B24CFD999230A614B7E3F14D9DFAE475432F37F5E28496BF8D3`.
The plan-only runner accepted all six jobs.
Run directory: `target/parallelism-ladder/sparse-phase-profile-20260831`.

The detached runner freezes the current binary, source, manifest, cases, and scripts.
It provides a completion dialog and authoritative status files. Do not infer findings
until the frozen analyzer finishes.

This is a diagnostic screen, not a speed comparison. The instrumented variant reads
the clock several times per sparse pass and performs atomic counter updates. Completed
pairs must still preserve exact result identities. The hard pair supplies capped work
distribution only.

## Decision rule

Consider an incremental basis only if repeated analyses account for substantial time
and concentrate in matrix sizes where retained elimination work could plausibly repay
rollback and invalidation costs. If preparation dominates, optimize substitution or
allocation instead. If forward elimination dominates but most calls are initial, focus
on the elimination kernel rather than incremental state.

Scheduling stays paused.

## Result

The frozen analyzer verifies all six records. Four runs complete optimally and the
hard-10 pair cancels cleanly at the shared cap. Preparation consumes 55.2-84.0% of
sparse time; Bareiss forward elimination consumes 10.0-31.5%. Active matrices are
small, and 79.7-91.9% of stored input rows disappear after known substitution.

Do not build a rollback-aware Bareiss basis next. Split preparation further, then
remove its measured scan, substitution, normalization, or sorting waste. Keep the
profile instrumentation behind the hotspot recorder. Scheduling remains paused.
