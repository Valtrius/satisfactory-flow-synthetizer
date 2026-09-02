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

- Variables and allocation-free accounting are permanent, with the measured
  limitations retained in experiments 44/45. Promotion notes: 6812173.
- New isolated candidate: reverse final canonical RREF pivot rows instead of
  sorting/deduplicating. Source a79ecf7, active but not promoted.
  No elimination, identity, pruning, cache or proof change.
- 221 solver-core and 330 workspace all-target tests pass, two/five ignored.
  Both strict Clippy configurations, 41 tooling tests, release build, 12 exact
  executable smokes and the frozen 40-job plan pass.
- User limit: one hour. Experiment 46 has 40 jobs, 20 adjacent pairs, search caps
  plus cleanup 52 minutes. All order counts exactly balanced; no timing yet.
- Read `mem:solver/experiments/46-rref-order`. Nothing launched yet.
  Live state `mem:solver/active`.
- Scheduler extras remain paused; no production affinity policy. No push.
  Integer inequality rows remain restored in 5d8d924.
- Do not confuse a committed candidate with a permanent optimization. New
  RREF/258 and accounting/unrestricted258 are deferred to respect the time limit.

## When to read more

- Exact timings / evidence → `mem:solver/experiments/index` then one experiment memory
- Launch recipe → `mem:solver/benchmarking`
- Flags / source map → `mem:solver/controls`
- Proof / identity → `mem:solver/contracts`
- Update process → `mem:solver/workflow`
- Full permanent table / open questions → `mem:solver/status`
