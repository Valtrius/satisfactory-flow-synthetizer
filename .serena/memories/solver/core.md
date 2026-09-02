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
  unexplained earlier variable/258 timeout remain in the experiment46 records.
- Experiment47 diagnostics finished in 18m54: 14 verified, 10 optimal controls,
  four intended hard caps. 192 frozen hashes; exact completed outputs/proofs and
  15 structural counters equal. No new speedup claim.
  `mem:solver/experiments/47-rref-arithmetic-profile-results`.
- Elimination is 66–69% of instrumented RREF time; normalization 17–18%, suffix 4%.
  Zero destinations occur in 78–80% of updates; factor -1 in 26–30%.
  The installed rational operators lack explicit shortcuts for these patterns.
- Recommend zero-destination shortcut first, then factor -1 separately. Preserve
  exact arbitrary-size arithmetic and canonical order. Test hotspot-OFF completed
  workloads with fixed affinity and balanced order before any promotion.
- These are proposals, not implemented changes. No new benchmark launched.
  Diagnostic source 1006e6f and manifest 14d3f73 remain committed; current result
  memories uncommitted. No commit/push in analysis.
- Scheduler extras stay paused; no production affinity, cyclicity or memory gate.
  New benchmark limit <=1h including cleanup. Nothing running.
  `mem:solver/active`.

## When to read more

- Exact timings / evidence → `mem:solver/experiments/index` then one experiment memory
- Launch recipe → `mem:solver/benchmarking`
- Flags / source map → `mem:solver/controls`
- Proof / identity → `mem:solver/contracts`
- Update process → `mem:solver/workflow`
- Full permanent table / open questions → `mem:solver/status`
