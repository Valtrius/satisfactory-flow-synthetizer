# 30. Remove the recursive DFS state cache

Date: 2026-08-31. State: planned.

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
