# 29. Defer state canonicalization until an invariant repeats

Date: 2026-08-31. State: permanent.

## Hypothesis

Experiment 28 removed the second graph-labeling pass used to choose a recursive DFS
open port. State memoization still labels every propagated state before search. In the
capped hard-10 sample, the candidate retained 1,281,513 states and rejected 312,642
canonical duplicates. Most state visits therefore paid for an exact key that did not
produce a cache hit.

Use a cheaper isomorphism-invariant fingerprint first. Keep the first state in each
fingerprint bucket without labeling it. If the bucket repeats, label both the saved
state and the current state and use only those authoritative exact keys for cache
reuse.

## Candidate

- The fingerprint includes the remaining profile, discard count, link count, sorted
  link endpoint/type/flow descriptors, and sorted node incident descriptors. Raw node
  identifiers and port labels do not enter the semantic descriptors.
- The first state in a fingerprint bucket uses the existing MRV prefix and a raw
  representative as its final traversal-order tie-break.
- The second state promotes the bucket. The solver canonicalizes the saved and current
  snapshots, transfers the saved status to its exact key, and checks the current exact
  key normally. Later visits to that bucket use exact canonicalization directly.
- A fingerprint collision can trigger unnecessary promotion, but it cannot cause a
  cache hit. Fingerprint inequality can miss a reuse opportunity, but it cannot prune
  search.
- Shared-cache and donation paths retain exact state keys. Root partition and adaptive
  frontier identities are unchanged. Scheduler policy remains p1.
- Diagnostics count first deferred visits and bucket promotions. Deterministic cache
  byte accounting includes saved snapshots.

The raw open-port choice changes traversal order only. Search still enumerates every
legal decision for the selected mandatory obligation. Completion, dead-state reuse,
SAT reuse, and duplicate elimination still require an exact canonical key.

## Correctness gate

- 184 solver-core tests passed with `bench-internals`; two manual benchmarks were
  ignored. The exhaustive outer Reference test and all four parallelism integrations
  also passed.
- A new differential test compares every fixed profile's exact witness set with the
  old always-canonical cache.
- The existing relabeling test now checks fingerprint equality for equivalent partial
  states and exercises promotion to exact keys.
- All workspace tests passed: 179 solver-core library tests, 124 other tests, five
  ignored in total, plus the exhaustive and parallel integration gates.
- All 33 analyzer tests passed.
- Strict solver-core all-target Clippy passed with and without `bench-internals`.

These checks establish the candidate's prelaunch correctness. They do not establish a
speedup.

## Smoke observations

Fresh release runs used production p1 with 32 workers and a 30-second cap:

| Workload      | Experiment 28 | Candidate | Observation |
| ------------- | ------------: | --------: | ----------- |
| 115 minimum L |       5.153 s |   3.580 s | Promising   |
| 238 optimal   |      10.102 s |   9.726 s | Small/noisy |

The 115 candidate retained 167,899 states but made 45,554 state-labeling calls. This
confirms that the mechanism skips most state labels on that run. These are unmatched
smoke samples against historical timings, not promotion evidence.

## Frozen screen

Manifest: `benchmarks/custom/deferred-state-canonicalization-screening.json`.

The screen compares the committed experiment-28 binary with the candidate on 26
randomized fresh processes:

- two samples per variant for 36 optimal;
- two samples per variant for 115 all L and minimum L;
- two samples per variant for 238 optimal, minimum L, and all L;
- one 60-second hard-10 diagnostic per variant with hotspots enabled.

Completed pairs must preserve status, proof, preferred witness, full solution objects,
and canonical layout-key sets. The hard diagnostic may remain incomplete. Only its
common partial results and proof state are compared, and it supplies no completion
claim.

The plan-only runner accepted all 26 jobs. Frozen `profile_case.exe` SHA-256 values:

- committed experiment-28 reference: `77951194552315748e710719482c27453adb964b1b1208ccdcf4028e2884c4ed`;
- experiment-29 candidate: `3c68cdb7c017ad0d0fbdb6841a339bf287acae9b7d028d8f356ca23ac61fd458`.

The detached runner will freeze its manifest, cases, scripts, binaries, source tree,
and working-tree diff. Its status files and Windows dialog are authoritative for
completion.

## Decision gate

Prefer the candidate if exact verification passes and the repeated completed workloads
show a consistent whole-solve improvement. A small neutral result is acceptable if the
larger completed workloads improve and no case regresses consistently. Keep scheduler
experiments paused.

## Results

The detached runner finished and verified all 26 records. Twenty-four completed
optimally and the two hard-10 diagnostics reached their common 60-second cap. Every
completed pair preserved status, proof, preferred witness, full solution objects, and
canonical layout-key sets. The capped pair preserved its comparable partial proof
state. No process failed or was killed.

| Workload      | Before median | After median | Wall reduction |
| ------------- | ------------: | -----------: | -------------: |
| 36 optimal    |      16.559 s |     13.525 s |          18.3% |
| 115 all L     |      61.074 s |     43.557 s |          28.7% |
| 115 minimum L |       5.473 s |      3.388 s |          38.1% |
| 238 all L     |      23.762 s |     18.992 s |          20.1% |
| 238 minimum L |      11.166 s |      8.629 s |          22.7% |
| 238 optimal   |      12.032 s |      9.415 s |          21.8% |

Both samples favor the candidate in every row. The completed workloads retain the
same broad amount of structural work except 238, where raw first-visit ordering raises
decisions 14.6% but still reduces wall time 20.1-22.7%.

Deferred bucket promotions closely track exact duplicates. For example, 115 all has
264,634 promotions and 266,644 duplicate hits; hard 10 has 340,182 promotions and
346,590 hits. The invariant therefore avoids most exact labels without turning many
unrelated states into promoted buckets.

Exact duplicate hits account for 11.3-20.4% of cache-eligible visits across these
workloads. State canonicalization aggregate time falls from 622.8 to 213.9 worker
seconds on 115 all and from 131.8 to 52.1 seconds on 36 optimal. In the fixed hard-10
cap, state graph-labeling calls fall 48.5%, while retained states rise 6.6% and
decisions rise 8.6%. The hard pair does not establish completion speed.

Deterministic cache-byte peaks rise 9.6% on 36, 19.2% on 115 all, 27.4% on 238, and
34.4% in the hard diagnostic. Memory remains diagnostic rather than a promotion gate.

Summary SHA-256:
`db9c47452cbfaeb1ef16e6855e31237dbb6bba1a004d7a7ecdbf09b01f374069`.

## Decision

Make deferred state canonicalization permanent. The repeated screen satisfies the
predeclared gate on all six completed workloads, preserves exact results, and improves
both completion and first-witness latency. Keep scheduling paused.

This result does not prove that a state cache is faster than no cache. Duplicate counts
measure avoided entry visits, not the size of the subtrees behind them. Experiment 30
will test complete removal directly.
