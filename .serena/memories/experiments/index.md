# Solver experiments

The current solver-core code is the baseline, including first-output partitions, direct flow equalities, first-optimum stopping and the sparse/Boolean portfolio. The eager second-output candidate is excluded.

All experiments below ran on 2026-09-08 (initial screen started 2026-09-07). Times are release wall seconds, 32 total workers unless stated. Read one corresponding memory for details.

| Record                                   | Decision                                             |
| ---------------------------------------- | ---------------------------------------------------- |
| `mem:experiments/01-initial-screen`      | Initial measurement; preserve tie-verifier failure   |
| `mem:experiments/02-output-partitions`   | Keep complete first-output producer roots            |
| `mem:experiments/03-direct-flow`         | Keep direct equalities for single-input destinations |
| `mem:experiments/04-first-optimum`       | Keep first proved optimum; commit 16713a0            |
| `mem:experiments/05-sparse-counts`       | Keep as one portfolio branch                         |
| `mem:experiments/06-boolean-counts`      | Keep as the other branch; unsuitable alone for 10    |
| `mem:experiments/07-portfolio`           | Keep general default; commit 02ae07b                 |
| `mem:experiments/08-eager-second-output` | Rejected/restored; archive commit a9c114d            |

Prepared campaign: `mem:experiments/09-optimization-campaign` covers scope-limited second-output partitions, worker allocations, delayed starts, standalone controls and separate root audits. No candidate is promoted by preparation.

Next unimplemented hypotheses: delayed bounded splitting that preserves parent progress, proven profile/rate cuts and incremental reuse across L groups. Measure hardest completion and retain adverse samples. SMT checks dominate, so canonicalization/duplicate suppression are lower priorities.
