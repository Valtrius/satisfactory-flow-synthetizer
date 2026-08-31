# 32. Quotient sparse propagation by exact weighted representatives

Date: 2026-08-31. State: rejected; source restored.

## Hypothesis

After experiment 31, the capped hard-10 candidate still spends 372.129 aggregate
worker seconds in sparse analysis. `SparseSystem` currently substitutes fixed values
but otherwise eliminates the original port variables, even when the weighted
union-find has already proved that several variables are exact rational multiples of
one representative.

Rewrite every retained equation through those proved representatives before Bareiss
elimination. Combining equal columns, substituting fixed representatives, removing
tautologies, and deduplicating identical normalized rows should reduce exact matrix
work without changing search.

## Candidate

- Build `variable = factor * representative` projections from the current rollback
  weighted union-find at each propagation fixed-point pass.
- Replace each sparse term with its representative term and combine rational
  coefficients exactly.
- Substitute representatives with known exact values.
- Clear all denominators back to a primitive integer row.
- Remove tautologies and duplicate normalized rows, but retain explicit
  contradictions.
- Run the existing fraction-free elimination on the remaining representative
  variables. Feed its exact values and ratios back into the same weighted union-find.
- Leave SCC algebra, bounds, DFS, cache identity, proof accounting, validation, and
  production p1 scheduling unchanged.

## Soundness argument

Every projection used by production propagation is already an established exact fact:
terminal constants, link equalities, splitter equalities and arity, or a deduction
from an earlier equivalent sparse basis. Substituting `x = factor * representative`
therefore restricts neither the feasible set nor the contradiction set.

Summing coefficients for the same representative is ordinary exact substitution.
Multiplying the rational equation by the positive least common denominator and then
primitive normalization multiplies it only by nonzero scalars. Removing `0 = 0` and
duplicate rows removes no constraint; `0 = c` remains and still proves inconsistency.
Deductions for representatives cover every class member through its stored exact
factor.

The generic quotient API rejects a row variable without a supplied projection instead
of silently treating it as independent.

## Validation

- Three focused tests cover rational weighted projection, known representative
  substitution, contradiction preservation, and missing-projection rejection.
- 182 default solver-core library tests pass; two manual benchmarks are ignored.
- With `bench-internals`, 187 library tests pass and two are ignored. The exhaustive
  outer Reference differential and all four parallelism integrations pass.
- All workspace tests pass with five ignored.
- Strict solver-core all-target Clippy passes with and without `bench-internals`.
- All 33 analyzer tests pass.

The first strict Clippy pass rejected one owned rational parameter and the projection
helper's complex tuple return type. It passed after borrowing the rational and using a
named internal facts struct. This was a prebenchmark lint failure, not a solver result
failure.

## Frozen screen

Manifest: `benchmarks/custom/sparse-quotient-screening.json`.

The randomized fresh-process screen repeats experiment 31's 38-job protocol:

- three samples per variant for 36 optimal;
- three samples per variant for 115 all L and minimum L;
- three samples per variant for 238 optimal, minimum L, and all L;
- one 60-second hard-10 diagnostic per variant with hotspots enabled.

The committed experiment-31 reference is `9f63f01`. Frozen reference
`profile_case.exe` SHA-256:
`3E0D1674A89CFFDFE6313CFDC4D89384ACB171CA1F16F47E85AF254D5EB5F047`.

The plan-only runner accepted all 38 jobs. Candidate `profile_case.exe` SHA-256:
`D18ED1B5E7BE22E0D3965BCA1881B6F2E9BB16128F9B47D14EF636F47F3C3531`.
Run directory: `target/parallelism-ladder/sparse-quotient-screen-20260831`.

Completed pairs must preserve every exact result identity and checked structural
counter. The capped hard pair supplies only throughput and propagation-subphase
evidence. The runner freezes all inputs and provides a completion dialog.
Do not infer a result until its frozen analyzer reports completion.

## Decision gate

Promote only if exact verification passes and completed medians show a consistent net
gain. A reduction in sparse time alone is insufficient if projection construction or
rational normalization makes whole solves slower. Keep scheduling paused.

## Results

The detached runner finished and the frozen analyzer verifies all 38 records with zero
failures. Thirty-six runs complete optimally; both hard-10 diagnostics cancel cleanly
at the common cap. No process failed or was killed. Total process time was 11.531
minutes.

Every completed pair preserves status, proof, preferred witness, every solution
object, canonical layout-key set, validation, and layout count. Each row has three
fresh processes per variant:

| Workload      |    Before median (range) |     After median (range) |  Wall change |
| ------------- | -----------------------: | -----------------------: | -----------: |
| 36 optimal    | 13.466 (13.383-13.993) s | 14.078 (13.622-14.558) s | 4.54% slower |
| 115 all L     | 42.674 (42.048-42.793) s | 40.452 (40.066-40.684) s | 5.21% faster |
| 115 minimum L |    3.243 (3.135-3.272) s |    2.931 (2.848-2.963) s | 9.63% faster |
| 238 all L     | 17.647 (17.470-17.761) s | 17.737 (17.607-18.353) s | 0.51% slower |
| 238 minimum L |    8.613 (8.603-8.853) s |    9.256 (8.879-9.397) s | 7.47% slower |
| 238 optimal   |    8.568 (8.510-8.776) s |    9.324 (9.196-9.591) s | 8.83% slower |

First-witness medians change by the same broad pattern: 115 improves 9.64-9.96%, 36
regresses 4.54%, 238 minimum L and optimal regress 7.46-8.83%, and 238 all is neutral.
Process CPU corroborates the regressions: 36 rises 8.83%, 238 minimum L rises 5.00%,
and 238 optimal rises 8.41%.

Completed structural counters are identical for 36 and differ by less than 0.02% on
the other workloads. The quotient changes the reduced basis and deduction cadence, so
the predeclared structural-counter equality gate is not strictly met even though all
exact result identities agree.

The hard pair is fixed-time evidence only:

| Metric                         |     Before |      After |      Change |
| ------------------------------ | ---------: | ---------: | ----------: |
| Structural decisions           |  3,395,770 |  3,365,307 | 0.90% fewer |
| Fixed-point passes             |  3,840,894 |  3,861,186 |  0.53% more |
| Aggregate sparse-analysis time |  337.558 s |  362.394 s |  7.36% more |
| Sparse time per pass           |  87.885 us |  93.855 us |  6.79% more |
| Propagation time per sync      | 170.959 us | 165.759 us |  3.04% less |

The intended sparse calculation itself becomes more expensive per pass. Lower total
non-sparse fixed-point work and slightly different search do not translate into a
consistent whole-solve gain.

Summary SHA-256:
`05069E26064B640366299A99C7CC2ED74330466B193230A1A4061B4F1C880BAE`.

## Decision

Reject the unconditional weighted quotient. It improves 115 but regresses 36 and the
238 single-solution paths, including first-witness latency. The original experiment-31
source was restored before this result record.

Do not select the optimization from benchmark cyclicity or case identity. A future
runtime gate would first need diagnostic counts for original/projected variables,
rows and terms, plus separate projection and elimination timers. Test normalized-row
deduplication without representative projection as the smaller next calculation.
Scheduling remains paused.
