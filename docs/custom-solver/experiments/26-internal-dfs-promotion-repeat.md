# 26 - Internal DFS promotion repeat

Date: 2026-08-30. State: verified and promoted permanently.
Related: [screening results](25-internal-dfs-marked-bypass.md).
Promotion commit: `1b62e5e`.

Run: ignored `target/parallelism-ladder/unkeyed-dfs-repeat-20260830/`.
Manifest: `benchmarks/custom/unkeyed-dfs-repeat.json`.

## Goal and gate

Repeat only the four experiment 25 workloads that completed. Two new samples per
variant combine with the screening sample to give three measurements for 36 optimal,
115 all, and 238 all/optimal. The 16 new jobs have 1,500 seconds of aggregate caps.
The runner randomizes their frozen schedule.

Promote the candidate only if all records verify, every completed pair preserves
the exact result fields, and the combined three-sample median favors the bypass on
each workload. Keep root planning and adaptive-frontier decisions keyed. Scheduler
experiments remain paused.

The run reuses experiment 25's frozen binaries. Their required SHA-256 values are:

- keyed reference: `743ecf14efeda83b708651c2b33042a89d1ce58b555fd3ee4328939e0515a7fd`;
- internal unkeyed candidate: `f15d49bba7a3263ae23711e553cb978eb0bdcb0902b17072596d1cd626f5dd73`.

## Results and decision

All 16 repeat records complete optimally and match their keyed references. No run
fails or is killed. The repeat `results/summary.json` has SHA-256
`6277ee8cb6c0de55fa895621d1511fdde39c5f8b695d1c583e033d21fb0a41b6`.

Combined with experiment 25, every variant has three samples:

| Workload    |      Keyed median (range) |     Bypass median (range) | Reduction |
| ----------- | ------------------------: | ------------------------: | --------: |
| 36 optimal  |    23.883 (23.314-24.291) |    18.404 (17.997-18.665) |    22.94% |
| 115 all     | 137.338 (135.907-137.975) | 113.342 (110.521-115.847) |    17.47% |
| 238 all     |    60.680 (60.345-61.754) |    47.746 (47.566-49.831) |    21.31% |
| 238 optimal |    30.110 (29.655-30.744) |    23.964 (20.110-24.767) |    20.41% |

All 12 completed A/B pairs preserve the problem, mode, optimal status, validation,
outcome, full solution objects, canonical layout-key set, layout count and preferred
key. Every predeclared gate passes, so the internal DFS bypass is permanent.

Production recursive DFS now enumerates local physical representatives directly and
lets the propagated canonical state cache remove the small number of equivalent child
states. Root partition planning and adaptive-frontier refinement remain keyed, so
stable proof-ledger partition identities do not change. The benchmark Cargo feature
has been removed. Scheduler experiments remain paused.

## Promotion validation

- Solver-core all-target tests with `bench-internals`: 183 passed, two manual
  benchmarks ignored.
- Exhaustive outer-reference integration: one passed.
- Parallelism integrations: four passed, including full enumeration across stages
  and worker counts.
- Shared application tests: 16 passed.
- Strict solver-core all-target Clippy passes with default features and with
  `bench-internals`.
- `npm run format` and `git diff --check` pass.

## Hard-run memory attribution

The high working set is primarily the sum of worker-local exact memoization caches.
Each dispatched root constructs a `SearchContext` with its own state-status map and
open-SCC summary map. Production p1 can keep 32 such roots active concurrently;
sharing remains disabled.

The experiment 25 hard-10 diagnostic supports this attribution:

| Variant | Largest local cache | State entries | Process peak |
| ------- | ------------------: | ------------: | -----------: |
| Keyed   |             54.0 MB |        24,039 |      1.61 GB |
| Bypass  |             65.6 MB |        30,988 |      2.04 GB |

Thirty-two times the candidate's largest local cache is about 2.10 GB, close to the
sampled 2.04 GB process peak. This is not exact accounting. The local counter combines
state and SCC payload lower bounds, reports the largest local cache rather than their
simultaneous sum, and excludes hash-table buckets, spare capacity, allocator retention,
thread stacks and temporary canonicalization buffers.

State keys are the likely largest retained item because every unique partial state
stores its full canonical byte identity plus a status. SCC summaries add exact keys,
endpoint deductions and arbitrary-precision rational payloads. Saved solutions are not
the cause on hard 10 because neither variant finds a witness.

If memory work resumes, first split live state-cache and SCC-cache bytes per worker and
report their aggregate concurrent totals. Then test smaller exact state storage or a
bounded completed-state policy. Never replace exact keys with unchecked hashes, and
retain in-progress entries needed for cycle and proof accounting.

## Next performance target

Do not resume scheduler experiments yet. In the promoted hard-10 diagnostic,
open-port canonicalization accounts for 303.6 of 316.5 legal-decision seconds. State
canonicalization consumes 623.4 accounted seconds and propagation consumes 569.9.
These are capped diagnostic totals, not completion-speed evidence.

The next isolated hypothesis is to return canonical open-port or orbit coordinates
from the state canonicalization already performed at each DFS node, then reuse that
labeling for MRV's invariant tie-break. This could remove a second family of graph
labelings without changing the state key or relying on known cyclicity. Preserve the
current keyed root/frontier identity path and verify against the exhaustive reference
matrix before timing whole solves.
