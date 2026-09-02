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

- Scheduler extras remain paused. No production affinity policy.
- Integer inequality rows restored to rational rows in 5d8d924.
  Record: `mem:solver/experiments/41-integer-rows-restoration`.
- Exp44/45 completed 198/198 optimally; independent frozen recheck and 905
  artifact hashes pass. All 99 pairs preserve exact results, proofs and 15
  structural counters. Summary recheck is byte-identical.
- User approved permanent retention of both small improvements. Complete
  variables wins 53/75 wall pairs, CPU 67/75; accounting wins 21/24 wall pairs,
  CPU 24/24. These are descriptive counts, not pooled speedups.
- Variables are not uniformly faster: 258/CCD32 paired wall median +1.07%,
  interval -2.21 to +2.78%. Preserve this uncertainty. Accounting is clearer,
  but its long 258 sample is only two pairs/CCD and w32 was not tested.
- Results: `mem:solver/experiments/44-topology-variable-confirmation-results`
  and `mem:solver/experiments/45-cache-accounting-results`.
- Benchmark topology policy worked; residual timing drift remains. 258 favors
  CCD96 while other cases favor unrestricted execution. No universal pinning.
- Proposed next isolated hypothesis: replace redundant final RREF row sort/dedup
  with reversal of its unique ordered pivot rows. Exact oracle first; no code
  change or speed claim yet. Keep larger RREF arithmetic work separate.
- Variables are permanent in 520b352, accounting in 331b87a. Promotion notes
  will be committed before the next isolated RREF candidate. Tooling 7309fb7.
  No production source rewrite needed for these two promotions. No push.
  Next benchmark has a user-imposed one-hour limit including cleanup.
  Nothing running yet. Current handoff: `mem:solver/active`.

## When to read more

- Exact timings / evidence → `mem:solver/experiments/index` then one experiment memory
- Launch recipe → `mem:solver/benchmarking`
- Flags / source map → `mem:solver/controls`
- Proof / identity → `mem:solver/contracts`
- Update process → `mem:solver/workflow`
- Full permanent table / open questions → `mem:solver/status`
