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

- Scheduler extras remain paused. No production affinity policy.
- Integer inequality rows restored to rational rows after the controlled adverse
  root comparisons. Record: `mem:solver/experiments/41-integer-rows-restoration`.
- Complete variables remain a candidate. Allocation-free cache-byte accounting
  is an independent active candidate, not promoted.
- Exp44 implements topology-aware, before-resume benchmark affinity, adjacent
  balanced pairs and separate per-CCD/unrestricted cohorts. The 198-job plan and
  real-executable smokes pass. Search+cleanup allowance 7h26m30s.
  Preparing launch; no performance result yet.
- Primary confirmation: `mem:solver/experiments/44-topology-variable-confirmation`.
  Extra accounting test: `mem:solver/experiments/45-cache-accounting`.
  Live state: `mem:solver/active`.
- 41 tooling tests, 221 feature-enabled solver-core tests, 309 workspace tests,
  both strict Clippy configurations, release build and formatting pass.
  Restoration `5d8d924`; accounting candidate `331b87a`;
  benchmark tooling/manifest commit pending. No push.

## When to read more

- Exact timings / evidence → `mem:solver/experiments/index` then one experiment memory
- Launch recipe → `mem:solver/benchmarking`
- Flags / source map → `mem:solver/controls`
- Proof / identity → `mem:solver/contracts`
- Update process → `mem:solver/workflow`
- Full permanent table / open questions → `mem:solver/status`
