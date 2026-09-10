# Solver experiments

Current production includes descending first-output order, static Boolean All min N/L partitions, direct flow equalities, first-optimum stopping and the sparse/Boolean portfolio. solver-reference remains the independent exhaustive oracle.

| Record                                       | Decision                                                  |
| -------------------------------------------- | --------------------------------------------------------- |
| `mem:experiments/01-initial-screen`          | Initial measurement; preserve tie-verifier failure        |
| `mem:experiments/02-output-partitions`       | Complete first-output producer roots                      |
| `mem:experiments/03-direct-flow`             | Single-input direct equalities                            |
| `mem:experiments/04-first-optimum`           | First proved optimum; 16713a0                             |
| `mem:experiments/05-sparse-counts`           | Portfolio branch                                          |
| `mem:experiments/06-boolean-counts`          | Portfolio branch                                          |
| `mem:experiments/07-portfolio`               | General allocation; 02ae07b                               |
| `mem:experiments/08-eager-second-output`     | Rejected/restored; a9c114d                                |
| `mem:experiments/09-optimization-campaign`   | Discovery candidates and frozen methodology               |
| `mem:experiments/10-ordering-and-partitions` | Descending order promoted; earlier adaptive evidence      |
| `mem:experiments/11-partition-promotion`     | Combined static partition policy promoted; 6852be9        |
| `mem:experiments/12-adaptive-comparison`     | Next bounded adaptive/grace comparison against production |

Read the matching record and actual campaign status. Preparation never implies promotion. Prioritize hard exact completion, preserve adverse pairs, use at least eight workers for performance and enforce the current three-hour session limit. SMT obligations, complete proof-preserving decomposition and incremental reuse are the remaining hard-case targets.
