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

Rejected / absent from production source: weighted sparse quotient (32), sparse row
dedup (33), N≤9 p1 guard (18).

## Current posture

- Scheduler extras (sharing, donation, parallel remaining groups) remain paused.
- Sparse substitution closed after exp 38; canonicalization profiling resumed.
- Integer rows `cd46fae` and complete variables `520b352` remain active committed
  candidates, not permanent. All 46 factorial/follow-up records verify exactly.
- Pooled 258 n=3 medians regress for all candidates: wall +22.46 to +27.38%,
  CPU +7.84 to +10.03%. The reference itself ranges 195.5-268.2s; do not dismiss
  adverse samples or infer causality. Two roots own the final ~108s search tail;
  cleanup on either longest root is under 0.6s. Variable collection falls 82.30%
  in one diagnostic, without a whole-solve improvement.
  Results: `mem:solver/experiments/42-258-confirmation-results`.
- User authorized expanded AFK testing. Experiment 43's 28-job isolated-root
  factorial fixes CPU affinity to logical CPU 0 or 31 while preserving the original
  p1/32, target-128, 76-key root plan. Compare variants within each CPU.
  Validated; preparing launch. No production solver or scheduler edits.
  Record: `mem:solver/experiments/43-root258-affinity`.
- Live handoff: `mem:solver/active`. Tooling/manifest/results updates pending commit.
  No promotion or push.

## When to read more

- Exact timings / evidence → `mem:solver/experiments/index` then one experiment memory
- Launch recipe → `mem:solver/benchmarking`
- Flags / source map → `mem:solver/controls`
- Proof / identity → `mem:solver/contracts`
- Update process → `mem:solver/workflow`
- Full permanent table / open questions → `mem:solver/status`
