# 34. Sparse phase profile results

Date: 2026-08-31. State: verified; diagnostic retained.

## Verification

The detached runner and frozen analyzer verify all six scheduled records with zero
failures or kills. The 115 all-L and 238 optimal pairs complete and preserve every
exact result identity and checked structural counter. The hard-10 pair cancels cleanly
at 60 seconds and supplies no completion claim.

- 115 all L: 49 layouts and `Optimal(N=7, L=10)` in both variants.
- 238 optimal: one layout and `Optimal(N=8, L=13)` in both variants.
- hard 10: both variants remain `Incomplete(Cancelled)` without a witness.

Summary SHA-256:
`D42B785095A58937BA39101B8F175022E9418037D82E03963B86EDC7B8081143`.

The diagnostic adds clock reads and atomic counters. Its 115 and 238 wall times are
1.33% and 1.11% longer than the uninstrumented reference. These single samples measure
diagnostic overhead, not solver performance. The hard pair is capped and cannot be
compared by wall time.

## Phase distribution

Times are accumulated across workers. Phase percentages use the diagnostic's internal
sparse total. The sparse/fixed-point column uses the existing enclosing timers.

| Workload    |     Calls | Sparse / fixed point | Avg input -> active rows | Preparation | Forward |  Back | Deductions | Residual | Avg/call |
| ----------- | --------: | -------------------: | -----------------------: | ----------: | ------: | ----: | ---------: | -------: | -------: |
| hard 10     | 3,961,736 |               73.37% |            40.39 -> 6.06 |      64.49% |  25.68% | 6.74% |      1.88% |    1.21% | 87.55 us |
| 115 all L   | 4,212,718 |               74.46% |            26.64 -> 5.41 |      55.23% |  31.51% | 9.30% |      2.30% |    1.66% | 51.26 us |
| 238 optimal |   484,676 |               67.56% |            34.01 -> 2.75 |      84.01% |   9.99% | 3.18% |      1.34% |    1.47% | 42.44 us |

Known substitution removes 85.0% of stored rows on hard 10, 79.7% on 115, and
91.9% on 238 before elimination.

## Reanalysis and matrix size

| Workload    | Initial | After value | After ratio | After both |
| ----------- | ------: | ----------: | ----------: | ---------: |
| hard 10     |  68.61% |      30.58% |       0.74% |      0.07% |
| 115 all L   |  66.99% |      32.30% |       0.69% |      0.03% |
| 238 optimal |  64.59% |      35.33% |       0.06% |      0.02% |

Most active matrices are small:

- hard 10: rows <=15 account for 89.94% of calls and 74.95% of time; variables
  <=15 account for 87.57% and 71.52%. Terms <=63 account for 98.82% of calls.
- 115: rows <=15 account for 90.66% of calls and 76.06% of time; variables <=15
  account for 87.82% and 71.01%. Every matrix has at most 63 terms.
- 238: rows <=15 account for 99.80% of calls and 99.35% of time; variables <=15
  account for 98.50% and 96.69%. Every matrix has at most 63 terms.

## Decision and next work

Do not implement a rollback-aware Bareiss basis now. Forward elimination is a minority
of sparse cost, matrices are usually tiny, and only 31-35% of calls are reanalyses
after a same-topology deduction. The added invalidation and rollback machinery would
target too little measured work.

Retain the gated diagnostic. Ordinary propagation takes the existing timer-free and
branch-free algebra path.

Next, split preparation into variable collection, substitution/normalization,
tautology filtering, sorting, and `WorkingRow` conversion. Then isolate the largest
component. The first low-risk candidate is a propagation-only analysis entry point
that trusts the already sorted registered-port keys as the complete variable set,
backed by an invariant test that every inserted sparse-row variable is registered.
Do not combine it with a known-row cache until its independent value is measured.

Canonicalization remains a separate hard-run target: hard 10 records 575.2 seconds of
state canonicalization and 296.1 seconds of SCC canonicalization, versus 223.7 seconds
of sparse preparation. Continue that track after the smaller preparation experiment;
do not hide a canonical change inside an algebra candidate. Scheduling stays paused.
