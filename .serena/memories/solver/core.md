# solver/core

Updated 2026-09-02.
Full status: `mem:solver/status`.

## Objective

Faster proven optimum + complete min-N enumeration on hard inputs.
Record first validated witness separately. CPU/memory explain cost; lower overhead
alone does not justify slower completion.
Scopes / benchmark names: `optimal` | `minimum_links` | `all`.

## Permanent (do not re-litigate without new evidence)

- Lazy MRV + exact RREF/inequality (`319191e`)
- Remove ordering-only full-witness refinement (`3e804e8`)
- Constructor eligibility / per-N reuse (`099cc12`, exp 10)
- Compact exact state/SCC keys (`2716fac`, exp 11)
- Unconditional p1 / adaptive partitions (`49ae34c`, exp 19); hard-10 memory deferred
- Skip constructor after winning N (`57e34c0`, exp 20)
- Analytic symmetric witness ports (`f5df873`, exp 22)
- Internal DFS marked-child bypass (`1b62e5e`, exp 26)
- State open-port coordinate reuse (exp 28)
- Deferred state canonicalization (exp 29); no-cache ablation rejected (exp 30)
- Propagation bound de-duplication (`9f63f01`, exp 31)
- Fully-known / no-known / mixed-row integer substitution (exps 36–38)
- Complete propagation variable discovery (`520b352`, exp 44)
- Allocation-free cache byte accounting (`331b87a`, exp 45)
- Canonical RREF reverse pivot ordering (`a79ecf7`, exp 46)

Rejected / absent from production source: weighted sparse quotient (32), sparse row
dedup (33), N≤9 p1 guard (18).

## Current posture

- Variables and allocation-free accounting remain permanent; all prior adverse
  observations retained. Promotion notes 6812173.
- RREF confirmation finished in about 37m48s: all 20 optimal. Independent frozen
  recheck, 976 hashes and ten exact paired proofs/15 structural counters pass.
  `mem:solver/experiments/46-rref-order-confirmation-results`.
- User approved permanent RREF retention in existing a79ecf7. Repeated completed
  wall gains on 115/238; CPU lower in all 18 RREF pairs across both runs.
  New wall medians -1.49%/-1.44% for 115/238. 258 +0.38% with one faster and one
  slower pair: no demonstrated 258 completion gain. No universal speedup claim.
- Previously capped variable/258 control completed twice, wall -1.09% median.
  Both below the old 320s deadline too. Earlier timeout is not reproduced and
  remains unexplained; preserve its failure record.
- RREF is permanent; promotion documentation precedes new diagnostic work.
  Promotion documentation committed as c106d4c. No push.
- Approved next work: profile canonical RREF pivot normalization, elimination and
  suffix preparation, plus coefficient/factor patterns. Existing +1/zero
  shortcuts already apply. Diagnostic twin committed as 1006e6f; validation/release smoke passed:
  `mem:solver/experiments/47-rref-arithmetic-profile`. No arithmetic optimization.
- No more broad ordering repeats recommended. Scheduler extras remain paused;
  no production affinity or cyclicity gate. Future benchmarks <=1h including
  cleanup. Both binaries frozen, 14-job plan validated; not launched yet.
  `mem:solver/active`.

## When to read more

- Exact timings / evidence → `mem:solver/experiments/index` then one experiment memory
- Launch recipe → `mem:solver/benchmarking`
- Flags / source map → `mem:solver/controls`
- Proof / identity → `mem:solver/contracts`
- Update process → `mem:solver/workflow`
- Full permanent table / open questions → `mem:solver/status`
