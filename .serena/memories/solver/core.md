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

Rejected / absent from production source: weighted sparse quotient (32), sparse row
dedup (33), N≤9 p1 guard (18).

## Current posture

- Variables and allocation-free accounting remain permanent, with all measured
  limitations retained. Promotion notes: 6812173.
- RREF reverse-order candidate a79ecf7 remains active but unpromoted.
  Exp46 finished in about 36m29s: 39 optimal, one clean cap; whole screen FAILED.
  All 977 frozen hashes, exact completed outputs and 19 complete paired proofs/
  15 structural counters pass. Failure is the variables/258/CCD32 320s timeout.
- RREF wall medians: 115 -0.44%, 238 -1.68%, 36 -0.83%. CPU lower in all ten pairs,
  but wall lower in only six; all small-sample wall intervals cross zero.
  Recommend one focused confirmation before making permanent.
- Variable controls: 115/36 favorable medians; 258 has one -0.64% completed pair
  and one capped candidate against a 302.350s completed reference. Retain both;
  no overall 258 completion ratio. Regression question remains unresolved.
- Accounting unrestricted controls: 115 wall +0.54%, 36 -1.06%, two pairs each.
  Keep prior promotion but do not claim a universal win.
- Prior results: `mem:solver/experiments/46-rref-order-results`.
  Focused confirmation is plan-validated, not launched yet:
  `mem:solver/experiments/46-rref-order-confirmation`. 20 jobs, 55 minutes
  search+cleanup, 400s cap for variables/258 and new RREF/258 coverage.
  Same frozen binaries; no new solver candidate or build.
- Scheduler extras remain paused; no production affinity policy. Integer
  inequality rows remain restored. No new source edit or push.
  Follow-up manifest and results/handoff notes being committed; `mem:solver/active`.

## When to read more

- Exact timings / evidence → `mem:solver/experiments/index` then one experiment memory
- Launch recipe → `mem:solver/benchmarking`
- Flags / source map → `mem:solver/controls`
- Proof / identity → `mem:solver/contracts`
- Update process → `mem:solver/workflow`
- Full permanent table / open questions → `mem:solver/status`
