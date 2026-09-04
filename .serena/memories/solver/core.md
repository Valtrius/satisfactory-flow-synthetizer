# solver/core

Updated 2026-09-04. Full history: `mem:solver/status`. Live state: `mem:solver/active`.

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
- Skip remaining-profile reachability no-ops and borrow TopologyState for the search dead verdict,exp51. Public PartialTopology analyzer unchanged. Early pre-SCC move rejected. Permanent in production source, unpushed. `mem:solver/experiments/51-reachability-results`.

## Current decision

User requested permanent commit49, then analysis of50. Commit4d711b1899ebca8cefe1b87cfb060596257d1049 contains the exact tested49 source. No push.
Experiment50 checked64 canonical RREF is NOT promoted. All56 jobs verified,52 complete and4 intended caps. Completed wall medians:115 -2.48%,238 -0.22%,258 -0.21%,36 +2.80%. All5 36 CPU pairs worse, median+8.49%. Conversion consumes57.8–63.6% of small-attempt diagnostic time on completed115/238. These nested timers are not CPU or ordinary wall shares. `mem:solver/experiments/50-checked-rref-results`.
Production source restored to49 and verified against frozen50-before. Candidate preserved only in benchmarks/custom/variants/checked-rref.patch, manifest, fixture, detached checkout and frozen evidence. No unconditional small-integer RREF path in production.
Experiment51 skip+borrowed dead-verdict is permanent in production source on top of 482213a. Early not applied. Unpushed. Official screen `target/parallelism-ladder/reachability-screen-20260903` verified 312/312. Completed 115/238/258 OFF wall improved on 32/32 pairs, medians −3.08%/−3.62%/−1.52% on the primary cells; 36 wall −0.41% with bootstrap including 0, CPU −2.85% on 8/8. `mem:solver/experiments/51-reachability-results`.

Rejected/held: weighted sparse quotient32, sparse row dedup33, N<=9 p1 guard18, integer inequality rows41, zero-destination48 held, checked64 RREF50 not promoted. Preserve adverse results. No production affinity, cyclicity, memory or case-based arithmetic policy.

## Remaining authorized work

Original apply-all request remains in `mem:solver/experiments/49-wall-time-priorities`. Preparation reuse was deferred after profiling. Any new arithmetic trial must differ materially, e.g. avoid conversions or separately test fraction-free elimination. Reachability skip+verdict is permanent; early rejected. Experiment52 tail-only private-cache sibling help is rejected and restored. Next: hotspot-on cost-mix and root-tail profile on this tree, one-hour cap. `mem:solver/experiments/52-tail-scheduling-results`. Check `mem:solver/active` for live state.

## Routing

- Evidence lookup: `mem:solver/experiments/index`, then one relevant experiment.
- Launch/interpretation: `mem:solver/benchmarking` and `mem:solver/workflow`.
- Flags/source map: `mem:solver/controls`.
- Update Serena after every meaningful decision. Benchmarks <=1h including cleanup; launch with completion signal then end turn; no builds/tests during timing.

2026-09-04 update: experiment51 permanent, unpushed. Experiment52 rejected and restored. Next is a current hotspot profile. `mem:solver/experiments/51-reachability-results`.
