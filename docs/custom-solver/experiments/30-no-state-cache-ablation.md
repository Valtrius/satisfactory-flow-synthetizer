# 30. Remove the recursive DFS state cache

Date: 2026-08-31. State: rejected by the fail-fast screen.

## Question

Does recursive DFS state memoization reduce whole-solve completion time after
experiment 29 made most first visits cheap?

The measured duplicate fraction is 11.3-20.4% of cache-eligible visits. A hit can skip
an entire remaining subtree, so this fraction may understate the benefit. The cache
also costs fingerprint construction, saved snapshots, hashing, exact promotion,
memory, and teardown. Counts alone cannot establish which side wins.

## Isolated candidate

- Disable the worker-local recursive DFS state cache entirely.
- Do not compute state fingerprints, save deferred snapshots, or build state canonical
  keys for memoization.
- Keep root partition and adaptive-frontier canonical identities unchanged.
- Keep SCC caching, witness canonicalization/deduplication, exact propagation,
  cancellation, validation, and proof accounting unchanged.
- Use the same raw MRV representative as experiment 29 inside recursive DFS.

The bounded search remains finite because each decision occupies a port or adds a
bounded-profile node/link. This needs an exhaustive Reference differential before any
timing run.

## Fail-fast benchmark plan

First compare the frozen experiment-29 candidate with no cache on one fresh process
per variant:

- 115 minimum L, capped at 30 seconds;
- 238 optimal, capped at 30 seconds;
- 36 optimal, capped at 45 seconds.

Stop the branch if completed work regresses badly or reaches a cap. If it remains
competitive, repeat the completed workloads and add 115/238 all plus a fixed-time
hard-10 throughput diagnostic. Exact output equality remains mandatory; structural
counter equality is not expected because memoization changes explored work.

## Validation

The candidate added an explicit disabled state-cache mode and an uncached-visit
counter. Shared-cache and donation paths remained exact, though production p1 does not
enable them.

- 179 solver-core library tests passed; two manual benchmarks were ignored.
- With `bench-internals`, 184 library tests passed and two were ignored. The exhaustive
  outer Reference test and all four parallelism integrations passed.
- The state-cache differential compared exact, deferred, and disabled modes on fixed
  profiles. The existing exhaustive finite Reference matrix exercised disabled mode.
- All workspace tests passed with five ignored.
- Strict all-target Clippy passed with and without `bench-internals`.
- All 33 analyzer tests passed.

One cache-specific relabeling test initially failed because it still instantiated the
production policy and expected one cache hit. The test was corrected to select deferred
mode explicitly. This was a test-wiring failure, not a changed solver result.

## Fail-fast results

Fresh sequential controls compared the frozen experiment-29 binary with the no-cache
candidate on the same machine state:

| Workload      | Deferred cache | No cache | No-cache change |
| ------------- | -------------: | -------: | --------------: |
| 115 minimum L |        3.447 s |  3.870 s |    12.3% slower |
| 238 optimal   |        9.322 s | 27.949 s |    3.00x slower |

Both pairs preserve status, proof outcome, preferred key, every solution object,
canonical layout-key set, validation, and layout count.

The 115 candidate attempts 634,849 structural decisions versus 397,054 with deferred
caching, 59.9% more. On 238 it attempts 2,714,160 versus 627,595, 4.33 times as many.
Removing state-key work therefore exposes much more propagation and SCC work than the
cache avoids in bookkeeping.

No-cache deterministic cache bytes fall sharply because only other caches remain, but
memory is not the optimization gate. Completion time regresses on both controls.

## Decision

Reject complete removal of recursive DFS state memoization. The predeclared fail-fast
gate triggered on the first two workloads, so the 36 control and larger detached suite
were intentionally not run. Restore experiment 29 unchanged. Scheduler work remains
paused.
