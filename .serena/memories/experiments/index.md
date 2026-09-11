# Solver experiment decisions

Current production keeps descending roots, direct equalities, first-optimum stopping, sparse/Boolean portfolio and hybrid All min N/L partitions. solver-reference remains the independent oracle. Scheduling work is paused after explicit acceptance of the hybrid tradeoff.

| Record                                       | Decision                                                   |
| -------------------------------------------- | ---------------------------------------------------------- |
| `mem:experiments/01-initial-screen`          | Preserve initial tie-verifier failure                      |
| `mem:experiments/02-output-partitions`       | Keep complete first-output roots                           |
| `mem:experiments/03-direct-flow`             | Keep single-input equalities                               |
| `mem:experiments/04-first-optimum`           | Keep first proved optimum; 16713a0                         |
| `mem:experiments/05-sparse-counts`           | Keep as portfolio branch                                   |
| `mem:experiments/06-boolean-counts`          | Keep as portfolio branch                                   |
| `mem:experiments/07-portfolio`               | Keep independent proof races; 02ae07b                      |
| `mem:experiments/08-eager-second-output`     | Reject broad eager policy; a9c114d                         |
| `mem:experiments/09-optimization-campaign`   | Discovery provenance and frozen evidence                   |
| `mem:experiments/10-ordering-and-partitions` | Keep descending order; cd703e9                             |
| `mem:experiments/11-partition-promotion`     | Keep static Boolean path; 6852be9                          |
| `mem:experiments/12-adaptive-comparison`     | Reject global adaptive/grace replacements                  |
| `mem:experiments/13-hybrid-final`            | Hybrid promoted with accepted case-97 uncertainty; 49d57e0 |

Application follow-up: benchmarks/optimization/results-application-20260911.md records three promotions, two exclusions, and the combined frontend/database tradeoff. The measured production candidate is ab8b913; no solver scheduling change was promoted.

Evidence IDs resolve through benchmarks/evidence.json. Completed recipes/manifests are retained in Git 2dccf50 and matching frozen artifacts. Current contracts are in `mem:solver/contracts`, and benchmark authorization/limits in `mem:solver/benchmarking`. Read a record as measured history, not as permission to rerun its queue.
