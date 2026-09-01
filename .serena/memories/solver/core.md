# solver/core

Updated 2026-09-01.
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

Rejected / absent from production source: weighted sparse quotient (32), sparse row
dedup (33), N≤9 p1 guard (18).

## Current posture

- Scheduler extras (sharing, donation, parallel remaining groups) remain paused.
- Sparse substitution closed after exp 38; canonicalization profiling resumed.
- Exp 41 integer rows `cd46fae` and exp 42 complete variables `520b352` remain
  active committed candidates, not permanent. All 36 factorial runs completed and
  verified exactly. Variables improve repeated 115/238 wall 1.15%/4.10%; integer
  effects are mixed. A single 258 variable-only run regresses wall 22.46% and CPU
  9.51% despite identical structural work. Do not discard it or declare a distribution.
- User approved the focused 258 follow-up. Its ten-job plan, reused binary/source
  hashes, tooling tests and diagnostic smoke checks pass; ready to launch.
  Follow-up: `mem:solver/experiments/42-258-confirmation`. Scheduler remains paused.
- Results: `mem:solver/experiments/42-complete-propagation-variable-set-results`.
  Pending work: `mem:solver/active`. Analysis memory updates are uncommitted.

## When to read more

- Exact timings / evidence → `mem:solver/experiments/index` then one experiment memory
- Launch recipe → `mem:solver/benchmarking`
- Flags / source map → `mem:solver/controls`
- Proof / identity → `mem:solver/contracts`
- Update process → `mem:solver/workflow`
- Full permanent table / open questions → `mem:solver/status`
