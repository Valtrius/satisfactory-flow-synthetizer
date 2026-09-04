# 60. Isolated remaining-port under-L prune

Date: 2026-09-04/05. State: permanent in `2d6501c`, unpushed.
Related: experiment59 count-only opportunity screen.

## Question and hypothesis

Does pruning applied children whose remaining node-port capacity cannot reach the exact L group, before propagation/SCC, improve completed wall time without changing exact layouts/proofs?

## Change and comparison

Isolated prune only. `evaluate_applied_child` returns `Exhausted` when `current L + remaining_operator_link_upper_bound < expected L`, before `prepare_applied_child`. Increments existing `lower_bound_prunes`. No over-L-before-prepare, no complete-L-before-solve.

Before: production `53794bd`. After: `benchmarks/custom/variants/remaining-port-under-l.patch`. Binaries hashed in `C:/Users/jakez/.codex/tmp/sf60/{before,after}/metadata.json`. Screen `target/parallelism-ladder/remaining-port-under-l-ab-20260904`. 30/30 verified, failures []. Three AB/BA pairs, hotspot off, p1. 258 omitted.

## Results

All 30 completed. Analyzer matched status, layout keys, preferred witnesses and full solutions.

Paired candidate/reference wall (median change, bootstrap 95%):

| Cell | Wall | CPU | First valid | Decisions before→after |
| 115 all CCD32 | −39.43% [−39.84, −39.34] | −49.85% | −12.60% | 3,989,576 → 2,087,005 |
| 115 all CCD96 | −39.73% [−39.93, −38.54] | −49.40% | −13.06% | same 47.7% fewer decisions |
| 238 opt CCD32 | −1.94% [−2.46, −1.35] | −0.97% | −1.94% | 627,595 = 627,595 |
| 238 opt CCD96 | +2.22% [−1.54, +7.78] | +0.17% | +2.22% | identical counters |
| 36 opt all-CPU | −7.58% [−7.87, −5.90] | −24.31% | −7.57% | 1,495,424 → 1,147,205 |

115 peak WS 245 MiB → 185–188 MiB. 36 74 → 69 MiB. 238 WS unchanged. 115 lower_bound_prunes 1956 → 47475; algebra_time_ns about half. 238 CCD96 r1 structural counters identical (14/627595/341428/70833), so the CCD96 wall CI including 0 is placement noise on a no-hit cell.

Layouts: 115 all 49 at N=7 L=10; 238 N=8 L=13 one layout; 36 N=9 L=11 one layout.

## Correctness and limitations

Exact public identity held. Structural counters differ where the prune fires, as required. Three pairs not five; 115 intervals are tight. 258 untested. Count-only experiment59 counters are absent from both timing binaries.

## Decision and next step

Accepted. Permanent in `2d6501c`, unpushed. Do not add over-L-before-prepare or complete-L-before-solve from experiment59. Isolated screened diff remains `benchmarks/custom/variants/remaining-port-under-l.patch` (do not re-apply). 258 can be a later control, not a promotion gate given the 238 no-hit identity match. Next: post-58/60 hotspot-on cost-mix, then SCC deferral only if it still owns a material nested share.
