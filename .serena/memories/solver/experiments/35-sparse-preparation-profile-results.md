# 35. Sparse preparation profile results

Date: 2026-08-31. State: analyzed. Related: experiment design (`mem:solver/experiments/35-sparse-preparation-profile`).

## Verification

The frozen runner completed and the analyzer verifies all six records with no
failures or watchdog kills. The candidate and reference executable hashes match
the prelaunch record.

- 115 all-L enumeration: both variants return `Optimal(N=7,L=10)` with the same
  49 exact layouts.
- 238 optimal: both return `Optimal(N=8,L=13)` with the same single layout.
- Hard 10: both cancel cleanly at the common 60-second cap without a witness.
  These two records measure capped work only.

Run: `target/parallelism-ladder/sparse-preparation-profile-20260831`.
Verified summary SHA-256:
`604A1B30F97F2A4A86EC56641EC23B8F4636137636160E1A80C47F7B02554D7A`.

## Preparation split

The table uses the candidate's worker-summed nanosecond counters. Percentages are
shares of the outer preparation timer. Working-row conversion happens after that
timer and is reported separately.

| Workload    | Preparation | Variables | Substitute + normalize | Filter |  Sort | Residual |
| ----------- | ----------: | --------: | ---------------------: | -----: | ----: | -------: |
| Hard 10 cap |    223.29 s |     8.59% |                 90.21% |  0.35% | 0.56% |    0.28% |
| 115 all     |    120.97 s |     8.16% |                 89.99% |  0.47% | 0.83% |    0.54% |
| 238 optimal |     17.53 s |     7.49% |                 91.42% |  0.40% | 0.28% |    0.40% |

Substitution and normalization consume 201.44, 108.86, and 16.03 worker-seconds.
That is respectively 58.05%, 49.12%, and 76.30% of the complete sparse-analysis
timer. Working-row conversion is only 0.07-0.11% of sparse-analysis time.

The other preparation components are too small to justify first attention:
tautology filtering is 0.35-0.47%, sorting is 0.28-0.83%, and residual timer and
profile construction work is 0.28-0.54%. Variable collection is measurable but
roughly eleven times smaller than substitution and normalization.

## Timing limitation

This is not a whole-solve speed comparison. The candidate adds clock reads and
temporarily collects tautologies so filtering can be timed separately. The single
completed wall samples differ by +0.38% on 115 and -3.70% on 238; neither establishes
a performance change. The hard variants perform different amounts of work within
the cap and cannot be compared as completion timings.

## Decision

Keep the diagnostic split. It is feature-gated behind hotspot recording and leaves
ordinary solves on the timer-free path. The instrumentation and documentation are
validated but not yet committed.

The next isolated production candidate should fast-path fully known rows in
`SparseRow::substitute`. Experiment 34 shows that 79.7-91.9% of stored row instances
become tautologies after substitution; every such row has all coefficients known.
Evaluate those equations with one exact integer common denominator and return either
the canonical tautology or contradiction directly. This avoids repeated
`BigRational` normalization and constructing a row that the caller immediately drops.

Benchmark that fast path without hotspots and with repeated completed 115/238 runs,
plus a fixed-cap hard-10 work comparison. If it is positive, separately test the
no-known-variable case, where cloning the already primitive row can skip redundant
normalization. Generalize integer denominator-LCM substitution to mixed rows only
after both smaller paths are measured. Do not combine these steps: their prevalence,
costs, and correctness surfaces differ.

Canonicalization remains the larger hard-run bucket, and scheduler work stays paused.
