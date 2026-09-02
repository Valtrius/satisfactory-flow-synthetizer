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
- Canonical RREF negative-unit elimination (`6c10b35`, exp 48)

Rejected / absent from production source: weighted sparse quotient (32), sparse row
dedup (33), N≤9 p1 guard (18).

## Current posture

- Variables, allocation-free accounting and RREF ordering remain permanent.
  Earlier adverse observations stay in the experiment46 records.
- Experiment48 negative-unit confirmation completed in 36m02. All 32 new jobs
  optimal; 191 frozen hashes, exact outputs/proofs and 15 structural counters pass.
  `mem:solver/experiments/48-rref-negative-unit-confirmation-results`.
- New completion paired medians: 115 all -1.59%, 238 optimal -2.05%,
  258 minimum_links -1.96%, hard36 optimal +0.39%.
  The new 115 and long 258 comparisons improve in all four pairs.
- Secondary six-pair medians by the same workload/placement: -1.16%, -1.42%,
  -1.35%, -0.05%. Across both screens, 19/24 wall and 23/24 CPU pairs improve.
  Preserve hard36's +4.70% pair. Its completion gain is not established.
- User approved permanent negative-unit arithmetic. Integrated kernel and diagnostic
  twin exactly match the tested candidate; no duplicate test changes.
  223 release tests with bench-internals and 213 default pass, 2 ignored each;
  strict release Clippy passes both. `mem:solver/experiments/48-negative-unit-promotion`.
- Zero-destination stays held. Initial 115/258 results mixed, including a +10.62%
  258 pair of unknown cause. No implicit combination or case-specific guard.
- Candidate/shared test preparation fadb316; initial results and confirmation
  manifest 3b32c51. Unrelated edb4f0b base preserved. Production now contains the tested negative-unit
  arithmetic. Source and supporting results/docs committed as 6c10b35.
  No push performed.
- Scheduler extras remain paused. No production affinity, cyclicity or memory
  gate. Future benchmarks <=1h including cleanup. Nothing running.
  `mem:solver/active`.

## When to read more

- Exact timings / evidence → `mem:solver/experiments/index` then one experiment memory
- Launch recipe → `mem:solver/benchmarking`
- Flags / source map → `mem:solver/controls`
- Proof / identity → `mem:solver/contracts`
- Update process → `mem:solver/workflow`
- Full permanent table / open questions → `mem:solver/status`
