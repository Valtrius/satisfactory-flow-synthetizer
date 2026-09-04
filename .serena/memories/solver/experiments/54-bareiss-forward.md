# 54. Isolated Bareiss-forward bookkeeping

Date: 2026-09-04. State: analyzed; promoted. Unpushed. Related: `mem:solver/experiments/53-post51-cost-profile`. Sharing remains experiment55.

## Question and hypothesis

After experiment53, leftover 16–31-row Bareiss systems own the largest nested timer on 115/258. Dropping the per-pivot `WorkingRow` clone, merge-joining remaining terms instead of a `BTreeSet` union, scaling rows that miss the pivot column without walking the pivot row, and skipping `div_rem` when the previous pivot is 1 should cut that bookkeeping without changing fraction-free quotients or search. Exact public results must match. 238/36 must not regress.

## Change and comparison

`crates/solver-core/src/algebra/sparse.rs` only. Same Bareiss identities. No scheduler, cache, reachability, or RREF change. Frozen before is production `7799c11`. Candidate is this promotion.

Screen `target/parallelism-ladder/bareiss-forward-screen-20260904`. Manifest `benchmarks/custom/bareiss-forward-screen.json`. 66 hotspot-off jobs, 15s grace, `MaxScheduledSeconds` 7200. Runner PID 24648. Finished 2026-09-04T16:34:39+02:00. `failures=[]`. Verified 66/66. 33 pairs, 31 completed, 2 intended stress caps.

Before SHA256 `d3b1f0f2b4f2bab2ed03b58ebc5634f9c710cd57a591e4d9043a2daa1d465952`. After SHA256 `ad645b376c898607281701fe453c76b304ba05acfa55c50d599ecaee7c27fc5d`. Frozen bins match `results/binaries.json`. Summary SHA256 `5949585c770c23a7389a942c71d52ef87b6348a10c764cbd5dfb88035332cd9b`. Affinity applied before resume on 66/66; requested=observed; 36 unrestricted has empty mask as planned. Hotspots off on every job. Do not compare with experiment53 hotspot-on walls.

## Results

Paired median wall (candidate/reference), exploratory bootstrap 95%:

| Cell | Mode      | Place     | Pairs | Wall median | Wall 95%         | CPU median | Notes                                                         |
| ---- | --------- | --------- | ----: | ----------: | ---------------- | ---------: | ------------------------------------------------------------- |
| 115  | all       | CCD96 16w |     5 |     −10.85% | −11.28 to −10.50 |    −10.36% | 49 layouts, N=7 L=10                                          |
| 115  | all       | CCD32 16w |     4 |     −10.58% | −10.65 to −10.23 |    −10.30% | same identity                                                 |
| 258  | min-links | CCD96 16w |     5 |      −8.49% | −8.62 to −8.00   |     −9.54% | 2 layouts, N=9 L=14                                           |
| 238  | optimal   | CCD96 16w |     4 |      −2.89% | −3.02 to −2.27   |     −2.58% | 1 layout, N=8 L=13                                            |
| 238  | optimal   | CCD32 16w |     5 |      −2.39% | −3.58 to +0.10   |     −2.40% | wall interval includes 0; CPU does not; 4/5 wall pairs faster |
| 36   | optimal   | 32w none  |     5 |      −0.83% | −1.79 to +0.39   |     −0.16% | interval includes 0; not a regression                         |
| tiny | all       | CCD96 16w |     3 |      −1.57% | includes 0       |        n/a | ~8 ms; sampling floor                                         |

115 first-valid medians −15.20% CCD96 and −14.32% CCD32. 258 first-valid tracks wall (−8.49%). Tiny CPU is sampling resolution, not a cost.

Always-on `algebra_time_ns` medians (not wall shares, not hotspot nested timers): 115 −17.69% both CCDs; 258 −24.88%; 238 −4.94%/−4.75%; 36 −0.74%. Consistent with leftover Bareiss bookkeeping mattering on 115/258 and little on 36.

Sampled peak WS: 258 after median 1,606,012,928 vs before 1,528,193,024 (~+5%). Other completed cells are flat or slightly lower. Peak cache bytes match on all 31 completed pairs.

Stress (no completion-speedup): 36-all CCD96 cap90 and 10-opt CCD32 cap90 both `Incomplete(Cancelled)`, deadline fired, layouts 8 and 0. After processed slightly more search in the same wall (36-all decisions 15,308,101→15,488,906).

## Correctness and limitations

Analyzer: completed results match available references; capped stress remain explicitly incomplete. All 31 completed pairs have identical layout keys, preferred witness, N/L, problem, proof object, and structural counters (decisions, states, deferred visits/promotions, root partitions, duplicates, contradictions, SCC solves, peak cache size/bytes). Only `last_progress` timings differ. Pre-launch: solver-core release tests 196/2 ignored with bench-internals, 191/2 default; both Clippy configs `-D warnings`.

Nested timers were off. Stress caps have no completion-speedup claim. 238 CCD32 wall bootstrap includes 0; the promotion case is the 115/258 medians plus no 36 median regression.

## Decision and next step

Permanent in `d6902a421b26f309788fc6ed6626555cb24b256a`; unpushed. Do not restore. Do not revive weighted quotients (32), duplicate-row removal (33), rollback-aware Bareiss (34), or prep-template reuse.

Next isolated experiment is 55: completed-state sharing that keeps deferred on owners. Not in this binary. 115 all-mode identity is the hard gate. Park fraction-free RREF, reachability, live DFS donation, production affinity.
