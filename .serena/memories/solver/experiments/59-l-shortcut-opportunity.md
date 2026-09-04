# 59. Exact-L shortcut opportunity counts

Date: 2026-09-04. State: analyzed.
Related: late L pruning analysis; experiment 13 complete-state exact-L rejection after algebra.

## Question and hypothesis

Do the three proposed exact-L shortcuts fire often enough on production search to justify implementing them?

Hypothesis: remaining-port under-L and over-L-before-prepare fire on applied children that currently still pay propagation/SCC; complete-graph L mismatch fires on finished wirings that currently still pay `solve_topology`.

## Change and comparison

Count-only instrumentation; no prune. Production p1, hotspots off.
Manifest `benchmarks/custom/l-shortcut-opportunity.json`. Four jobs, one repeat. Output `target/parallelism-ladder/l-shortcut-opportunity-screen-20260904`. Failed first plan path (`diagnostic` cohort invalid) left in place.

Analyzer reported `FINISHED WITH FAILURES` because renamed cases lack a frozen completed baseline (`lcut_r24_all` etc.). All four jobs completed, validated, `deadline_fired=false`. Layout identities match known production results: 24 all N=7 L=8 with 6 layouts; 115 all N=7 L=10 with 49 layouts; 238 optimal N=8 L=13 with 1 layout; 36 optimal N=9 L=11 with 1 layout. Walls are not a timing claim (1.64s / 36.23s / 5.60s / 11.10s).

## Results

Final merged diagnostics (hit rate = count / `raw_structural_decisions`):

| Job | Decisions | under-L remaining ports | over-L before prepare | complete L mismatch | lower_bound_prunes |
| 24 all | 360796 | 103298 (28.6%) | 0 | 6 | 4 |
| 115 all | 3989576 | 1949062 (48.9%) | 0 | 79 | 1956 |
| 238 optimal | 627595 | 0 | 0 | 0 | 14 |
| 36 optimal | 1495424 | 407348 (27.2%) | 0 | 0 | 2 |

238 last progress was the SAT group L=13; first-valid wall equals completion (5.599s). Under-L is expected to be rare on low-L unsat groups and on a short SAT group. 115 last progress was L=13 after min L=10, which is where a high target L makes remaining-port under-count fire.

Opportunity counts include children that current propagation would also kill. That is the prepare/algebra work a before-prepare under-L cut would skip.

## Correctness and limitations

Search identity unchanged. Unit tests: remaining-port bound on empty inventory; complete-state mismatch counter on the existing exact-L guard fixture. One sample per cell. Zero over-L is a measured miss, not a proof the cut is impossible on other cases. Complete-mismatch is on the complete path only; 79 vs 4.0M decisions on 115.

## Decision and next step

Under-L remaining-port prune is the only cut with material hit rate. Over-L before prepare: do not implement from this screen (zero hits). Complete-L before solve: rare; not promoted. Experiment60 isolated and promoted the remaining-port prune. Count-only counters are not in production. `mem:solver/experiments/60-remaining-port-under-l`.
