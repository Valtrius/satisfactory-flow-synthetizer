# 35. Sparse preparation profile

Date: 2026-08-31. State: completed; see [results](35-sparse-preparation-profile-results.md).
Related: [sparse phase results](34-sparse-phase-profile-results.md).

## Question and hypothesis

Experiment 34 attributes 55.2-84.0% of sparse-analysis time to preparation and
shows that known substitution removes 79.7-91.9% of stored rows. Which preparation
operation is the useful next optimization target?

The diagnostic splits preparation into variable collection, substitution and
normalization, tautology filtering, sorting, and conversion to elimination rows.
It uses only equations and facts already available to the solver. It does not use
the benchmark's cyclic or acyclic classification.

## Change and comparison

Ordinary solves retain the existing timer-free `prepare_analysis` path. Hotspot
runs use an equivalent profiled path and publish five additional flat nanosecond
counters. To time filtering separately, that path temporarily collects substituted
tautologies before retaining active rows; this extra allocation makes wall time a
diagnostic artifact rather than performance evidence.

Manifest: `benchmarks/custom/sparse-preparation-profile.json`. It pairs the frozen
experiment-34 diagnostic binary with the new diagnostic binary on:

- hard 10 optimal, p1/32, N<=11, 60-second cap;
- 115 all-L enumeration, p1/32, N<=12, 120-second cap;
- 238 optimal, p1/32, N<=12, 60-second cap.

All six jobs enable hotspots. Each variant has one fresh process per workload.
Reference commit: `ed5cbdc`. Frozen reference binary SHA-256:
`F30EF85776418B24CFD999230A614B7E3F14D9DFAE475432F37F5E28496BF8D3`.
Candidate `profile_case.exe` SHA-256:
`1E7B837244875D9DB2087886CDD8A525848838BDAE6FA5355CD5F4DEA9834B06`.
Run directory:
`target/parallelism-ladder/sparse-preparation-profile-20260831`.
The plan-only runner accepted all six jobs. The detached runner freezes the
candidate and reference binaries, solver source, manifest, cases, and analysis
scripts. It provides a completion dialog and authoritative status files.

## Results

The frozen analyzer verifies all six records. Completed 115 and 238 pairs preserve
their full exact results; both hard-10 records remain capped and incomplete.

[The result analysis](35-sparse-preparation-profile-results.md) attributes 89.99-91.42%
of preparation to substitution and primitive row normalization. Variable collection
uses 7.49-8.59%; filtering, sorting, conversion, and residual work are individually
below 1% of preparation or total sparse time as applicable.

## Correctness and limitations

The diagnostic calls the same row substitution, normalization, tautology test,
sort order, and `WorkingRow` conversion as production. Existing profiled/plain
equivalence tests cover consistent, inconsistent, unique, and ratio-producing
systems.

Validation completed before launch:

- 185 benchmark-feature library tests pass, with two manual tests ignored;
- the exhaustive outer Reference differential and four parallelism integrations pass;
- strict all-target solver-core Clippy and all 33 analyzer tests pass;
- repository formatting and the release build pass;
- a completed tiny smoke solve is validated and optimal. Its 39 profiled sparse
  calls populate all five new counters, and its outer preparation total exceeds
  their preparation-only component sum as expected.

The frozen analyzer verifies the schedule and exact completed outputs.

Nanosecond counters include clock-read overhead. Phase sums need not exactly equal
the outer preparation timer. A large component identifies an opportunity; it does
not prove that a proposed replacement will improve whole-solve completion.

## Decision and next step

Keep the diagnostic. Test exact integer evaluation of fully known rows in
`SparseRow::substitute`, then run an exact-output A/B without hotspot timing. If it
wins, isolate the no-known-variable clone path and only then generalize integer
substitution to mixed rows. Keep variable-set, sparse-basis, canonicalization, and
scheduler changes out of those measurements.
