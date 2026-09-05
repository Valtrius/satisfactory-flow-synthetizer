# solver/core

Updated 2026-09-05. Full history: `mem:solver/status`. Live state: idle (`mem:solver/active`).

## Objective and contracts

Faster proven optimum and complete min-N enumeration on hard inputs. Keep first validated witness, proof completion and enumeration completion distinct. Scopes: optimal, minimum_links, all. CPU/memory explain cost; lower overhead alone does not justify slower completion. Compare exact full results/proofs, not counts. `mem:solver/contracts`.

## Permanent

- Lazy MRV and exact RREF/inequality feasibility,319191e.
- Remove ordering-only full-witness refinement,3e804e8.
- Constructor eligibility and per-N reuse,099cc12,exp10.
- Compact exact state/SCC keys,2716fac,exp11.
- Unconditional p1/adaptive partitions,49ae34c,exp19. Hard10 memory remains deferred.
- Skip constructor after winning N,57e34c0,exp20.
- Analytic symmetric witness ports,f5df873,exp22.
- Internal DFS marked-child bypass,1b62e5e,exp26.
- State open-port coordinate reuse,exp28.
- Deferred state canonicalization,exp29. Whole cache removal rejected,exp30.
- Propagation bound de-duplication,9f63f01,exp31.
- Fully-known/no-known/mixed-row integer substitution,exps36–38.
- Complete propagation variable discovery,520b352,exp44.
- Allocation-free cache byte accounting,331b87a,exp45.
- Canonical RREF reverse pivot ordering,a79ecf7,exp46.
- Canonical RREF negative-unit elimination,6c10b35,exp48.
- Omit derived inequality payload from internal state/SCC keys,4d711b1,exp49. Actual feasibility bounds remain. Version2 key equality is equivalent to version1; public witness protocol unchanged. All17 substantive timing pairs improved, medians10.2% to21.3%, exact full results/proofs match. `mem:solver/experiments/49-derived-key-results`.
- Skip remaining-profile reachability no-ops and borrow TopologyState for the search dead verdict,exp51. Public PartialTopology analyzer unchanged. Early pre-SCC move rejected. Permanent in `7799c11`, unpushed. `mem:solver/experiments/51-reachability-results`.
- Bareiss-forward bookkeeping without pivot-row clones, set-union scans, or denom-1 `div_rem`,exp54. Same fraction-free quotients. Permanent in `d6902a4`, unpushed. `mem:solver/experiments/54-bareiss-forward`.
- Dirty-component bound scan: after a successful fixed-point, positivity/capacity/negative-ratio checks walk only dirty weighted-UF components,exp58. Same predicates; no `ExactInequality` restore. Permanent in `90747a4`, unpushed. `mem:solver/experiments/58-bounds-labeling-abc`.
- Remaining-port under-L prune: applied children whose current operator-link count plus remaining node-port capacity cannot reach the exact L group are exhausted before `prepare_applied_child`,exp60. No over-L-before-prepare and no complete-L-before-solve. Permanent in `2d6501c`, unpushed. `mem:solver/experiments/60-remaining-port-under-l`.

## Current decision

User requested permanent commit49, then analysis of50. Commit4d711b1899ebca8cefe1b87cfb060596257d1049 contains the exact tested49 source. No push.
Experiment50 checked64 canonical RREF is NOT promoted. All56 jobs verified,52 complete and4 intended caps. Completed wall medians:115 -2.48%,238 -0.22%,258 -0.21%,36 +2.80%. All5 36 CPU pairs worse, median+8.49%. Conversion consumes57.8–63.6% of small-attempt diagnostic time on completed115/238. These nested timers are not CPU or ordinary wall shares. `mem:solver/experiments/50-checked-rref-results`.
Production source restored to49 and verified against frozen50-before. Candidate preserved only in benchmarks/custom/variants/checked-rref.patch, manifest, fixture, detached checkout and frozen evidence. No unconditional small-integer RREF path in production.
Experiment51 skip+borrowed dead-verdict is permanent in `7799c110e8266b96cb59dd0480b62e59c576cda7` on top of 482213a. Early not applied. Unpushed. Official screen `target/parallelism-ladder/reachability-screen-20260903` verified 312/312. Completed 115/238/258 OFF wall improved on 32/32 pairs, medians −3.08%/−3.62%/−1.52% on the primary cells; 36 wall −0.41% with bootstrap including 0, CPU −2.85% on 8/8. `mem:solver/experiments/51-reachability-results`.
Experiment54 isolated Bareiss-forward bookkeeping verified 66/66 at `target/parallelism-ladder/bareiss-forward-screen-20260904`. Permanent in `d6902a4`, unpushed. 115 all wall −10.85% CCD96 / −10.58% CCD32; 258 min-links −8.49% wall / −9.54% CPU; 238 −2.89% CCD96 / −2.39% CCD32 (CCD32 wall bootstrap includes 0); 36 −0.83% with bootstrap including 0. Exact keys/proofs/structural counters match on 31/31 completed pairs. `mem:solver/experiments/54-bareiss-forward`.
Experiment55 completed-state sharing that keeps deferred owners verified 66/66 at `target/parallelism-ladder/completed-sharing-screen-20260904`. Rejected. 115 all-mode identity held (49 layouts, N=7 L=10). 258 min-links wall −4.27% (CI excludes 0); 115 wall −1.49%/−0.99% with CI including 0; 238/36 no wall win and zero shared hits. Peak WS +28–45% on completed cells. Solver source fully restored to `d6902a4`. Sharing still disables deferred first visits. Donation `join` still keeps only `best`. Candidate preserved as `benchmarks/custom/variants/completed-sharing.patch`. `mem:solver/experiments/55-completed-state-sharing`.
Experiment56 fraction-free canonical dense RREF verified 66/66 at `target/parallelism-ladder/fraction-free-rref-screen-20260904`. Rejected. No wall win with CI excluding 0; 36 wall +2.98% / CPU +5.17% (both CIs exclude 0). Exact keys/proofs match; structural counters match except `algebra_time_ns`. `canonical.rs` restored to `d6902a4`. Candidate preserved as `benchmarks/custom/variants/fraction-free-rref.patch`. Results recorded in `506bce8`. `mem:solver/experiments/56-fraction-free-rref`.
Experiment61 isolated SCC deferral verified 30/30 at `target/parallelism-ladder/scc-deferral-ab-20260905`. Rejected. Identity held (115 49 layouts N=7 L=10; 238 N=8 L=13; 36 N=9 L=11). 115/238 wall −5.16 to −8.85% (CIs exclude 0); 36 wall +4.93% CI [+2.27, +14.0] excludes 0; 36 CPU flat. Peak WS up on 115/238. Solver-core restored to `6bb0763`. Candidate preserved as `benchmarks/custom/variants/scc-deferral.patch`. Results recorded in `4adddc2`. `mem:solver/experiments/61-scc-deferral`.
Experiment62 remaining-port L-bound A/B/C verified 60/60 at `target/parallelism-ladder/l-bound-abc-20260905`. Rejected both. Identity held. places 238 CCD32 wall +1.26% CI excludes 0; forced prune fires 1:1 with propagation contradictions, 115 CCD96 wall +0.91% / CPU +0.34% CIs exclude 0. Production stays `6bb0763`. Patches kept. Results recorded in `4adddc2`. `mem:solver/experiments/62-l-bound-abc`.

Rejected/held: weighted sparse quotient32, sparse row dedup33, N<=9 p1 guard18, integer inequality rows41, zero-destination48 held, checked64 RREF50, fraction-free canonical RREF56, sharing-as-p155, tail-help52, labeling58, SCC deferral61, earlier under-L placement62, forced remaining-L62, overflow-next-L63. Preserve adverse results. No production affinity, cyclicity, memory or case-based arithmetic policy.

## Remaining authorized work

Experiment63 overflow-next-L rejected after 20/20 at `target/parallelism-ladder/overflow-next-l-ab-20260905`. Public keys/solutions held. 36 optimal wall +20.03% [+19.86, +20.11] / CPU +31.89% with extra imported later-L search. 115 all wall −6.55% but first-valid 4.7× later. 258 min-links flat. Solver-core restored. Candidate `benchmarks/custom/variants/overflow-next-link-group.patch`. Results recorded in `2b7a1c5`. Do not speculate across N. `mem:solver/experiments/63-overflow-next-link-group`.

Experiment62 remaining-port L-bound A/B/C rejected both isolated candidates after 60/60 at `target/parallelism-ladder/l-bound-abc-20260905`. Identity held. places: 238 CCD32 wall +1.26% CI excludes 0; no 115 completion win. forced: prune fires 1:1 with propagation contradictions; decisions/states unchanged; 115 CCD96 wall +0.91% / CPU +0.34% CIs exclude 0. Production stays `6bb0763`. Patches kept. Results recorded in `4adddc2`. Do not add earlier under-L placement or forced remaining-L. `mem:solver/experiments/62-l-bound-abc`.

Original apply-all request in `mem:solver/experiments/49-wall-time-priorities` is exhausted. Experiment58 dirty-component bound scan is permanent; BaseColor/labeling cache is rejected. Experiment60 remaining-port under-L prune is permanent in `2d6501c`. Experiment61 isolated SCC deferral is rejected: 30/30 at `target/parallelism-ladder/scc-deferral-ab-20260905`, identity held, 115/238 wall −5 to −9%, 36 wall +4.93% with CI excluding 0. Experiment62 remaining-port L-bound A/B/C rejected both: 60/60 at `target/parallelism-ladder/l-bound-abc-20260905`, identity held, places 238 CCD32 wall +1.26% CI excludes 0, forced 115 CCD96 wall +0.91% CI excludes 0. Solver-core remains `6bb0763`. Candidates remain `benchmarks/custom/variants/under-l-places.patch` and `benchmarks/custom/variants/forced-remaining-l.patch`. Do not add over-L-before-prepare, complete-L-before-solve, earlier under-L placement, or forced remaining-L. Do not retry RREF kernels, labeling reuse, or SCC deferral. Do not enable `work_stealing`. Park templates, sharing, donation, overflow-next-L, checked64, fraction-free RREF, weighted quotients, duplicate-row removal, rollback-aware Bareiss, early reachability, labeling, SCC deferral, earlier under-L placement, and forced remaining-L. Experiment63 overflow-next-L is rejected: 20/20 at `target/parallelism-ladder/overflow-next-l-ab-20260905`, public identity held, 36 wall +20.0% CI excludes 0, 115 first-valid 4.7× later. Solver-core restored. Candidate remains `benchmarks/custom/variants/overflow-next-link-group.patch`. `mem:solver/experiments/63-overflow-next-link-group`. Check `mem:solver/active`.

## Routing

- Evidence lookup: `mem:solver/experiments/index`, then one relevant experiment.
- Launch/interpretation: `mem:solver/benchmarking` and `mem:solver/workflow`.
- Flags/source map: `mem:solver/controls`.
- Update Serena after every meaningful decision. Benchmarks <=1h including cleanup; launch with completion signal then end turn; no builds/tests during timing.

2026-09-05 update: experiment63 overflow-next-L rejected after 20/20 at `target/parallelism-ladder/overflow-next-l-ab-20260905`. 36 wall +20.0% / CPU +31.9%; 115 first-valid 4.7× later; 258 flat. Solver-core restored. Candidate `benchmarks/custom/variants/overflow-next-link-group.patch`. Experiment62 remaining-port L-bound A/B/C rejected both after 60/60. Experiment61 isolated SCC deferral rejected after 30/30 official pairs. Experiment60 remaining-port under-L prune remains permanent in `2d6501c`, unpushed. Experiment59 count-only exact-L diagnostic: under-L 28.6–48.9% except 238 (0%); over-L 0; complete mismatch rare. Forced remaining-L fires but replaces propagation contradictions 1:1 without shrinking search. Do not implement over-L-before-prepare, complete-L-before-solve, earlier under-L placement, or forced remaining-L. Experiment58 dirty bounds remain permanent in `90747a4`, unpushed; labeling rejected. `mem:solver/experiments/63-overflow-next-link-group`.
