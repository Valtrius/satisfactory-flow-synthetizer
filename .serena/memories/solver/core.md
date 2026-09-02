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

- Variables, allocation-free accounting and RREF ordering remain permanent.
  RREF source a79ecf7, promotion docs c106d4c. Prior adverse cells and the
  unexplained earlier variable/258 timeout remain in experiment46 records.
- Experiment47 identified RREF elimination and zero/negative-unit arithmetic as
  candidates. Its instrumentation is diagnostic, not a speedup result.
  `mem:solver/experiments/47-rref-arithmetic-profile-results`.
- Experiment48 completed in 33m45. All 32 jobs optimal; 263 frozen hashes, exact
  solutions/outcomes/proofs and 15 paired structural counters verify.
  `mem:solver/experiments/48-rref-elimination-shortcuts-results`.
- Minus wall paired medians: 115 all -1.16%, 238 optimal -0.65%,
  258 minimum_links effectively 0%, hard36 optimal -1.14%. Seven of eight wall
  pairs and all CPU pairs improve, but only two pairs per workload.
- Zero is mixed: same workloads +0.52%, -2.64%, +4.71%, -2.28%.
  Preserve the adverse +10.62% 258 pair; cause unknown. Do not combine candidates.
- Recommend a negative-unit-only confirmation, four new pairs per workload,
  same frozen binaries and placement, 54m40 search+cleanup allowance under one
  hour. New screen analyzed separately first. User approved; preparation/plan pass.
  `mem:solver/experiments/48-rref-negative-unit-confirmation`.
- Neither arithmetic shortcut is in production. Shared tests/patches/manifest
  fadb316, prior results da2c89f. Current unrelated history HEAD 6683546 preserved;
  current canonical/solver production prefixes match the frozen baseline.
- Confirmation manifest and result/handoff notes await commit. No push.
- Scheduler extras stay paused; no production affinity, cyclicity or memory gate.
  New benchmark limit <=1h including cleanup. Confirmation prepared; not launched yet.
  `mem:solver/active`.

## When to read more

- Exact timings / evidence → `mem:solver/experiments/index` then one experiment memory
- Launch recipe → `mem:solver/benchmarking`
- Flags / source map → `mem:solver/controls`
- Proof / identity → `mem:solver/contracts`
- Update process → `mem:solver/workflow`
- Full permanent table / open questions → `mem:solver/status`
