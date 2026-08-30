# 28. Reuse state coordinates for DFS open-port selection

Date: 2026-08-30. State: permanent.

## Hypothesis

Recursive DFS canonicalizes the complete partial state before it chooses an MRV open
port. The old final tie-break then individualizes every surviving open-port candidate
and runs graph canonicalization again. Return canonical coordinates from the existing
state labeling and choose the minimum coordinate instead.

This uses only information already computed at the current state. It does not use a
case label or assumed cyclicity. Root partition planning and adaptive-frontier keys
keep the existing individualized-port path.

## Candidate

- State canonicalization returns its unchanged exact key plus canonical coordinates
  for every open producer and consumer.
- Recursive DFS keeps the existing cheap MRV prefix: partner count, known flow,
  external ownership, and physical-port symmetry class.
- Only the final invariant tie-break changes. Raw references break ties inside one
  canonical coordinate, where the choices are automorphic.
- Recursive DFS still enumerates raw local decisions and relies on the propagated
  canonical state cache for completion identity. Scheduler policy stays p1.

## Correctness gate

- 183 solver-core tests passed with `bench-internals`; two manual benchmarks were
  ignored.
- The exhaustive outer Reference test and all four parallelism integrations passed.
- A relabeling test compares the coordinate-selected child state-key set across every
  three-node permutation, every symmetric-port swap, and equal-input swaps.
- Strict solver-core all-target Clippy passed with `bench-internals`.

These checks establish the prelaunch candidate only. They do not establish a speedup.

## Frozen screen

Manifest: `benchmarks/custom/open-port-coordinate-screening.json`.

The 14 randomized jobs compare one committed reference and one candidate sample for:

- 36 optimal;
- 115 all L and all at minimum L;
- 238 optimal, all at minimum L, and all L;
- a capped 60-second hard-10 optimal diagnostic with hotspots enabled.

Completed pairs must preserve status, proof, preferred witness, full solution objects,
and the canonical layout-key set. The hard diagnostic may remain incomplete; its
common partial layouts and proof state must agree. Timing conclusions apply only to
completed pairs.

Frozen `profile_case.exe` SHA-256 values:

- reference frozen before the feature commit amendment at `3edfd54`:
  `6dec8c1431823d299c74ef7508b9f345785a4801574fafbcaf8e0efb2cde4fef`;
- candidate working tree: `77951194552315748e710719482c27453adb964b1b1208ccdcf4028e2884c4ed`.

The amended feature commit is `c38a334`. Its `crates/solver-core` tree is byte-identical
to the frozen reference tree, so the amendment does not change this A/B comparison.

The plan-only runner accepted all 14 jobs. The detached runner writes live status,
an authoritative finish or failure file, and shows a Windows completion dialog.

## Decision gate

Do not make the candidate permanent from hotspot subtraction alone. Prefer it if the
exact gates pass and completed whole-solve timings improve consistently. Repeat any
mixed or small result before promotion. Keep scheduling work paused.

## Results

The detached run finished and the analyzer verified all 14 records with no failures.
All six completed pairs preserved their exact status, proof, preferred witness, full
solution objects, and layout-key set.

| Workload      |    Before |    After | Speedup | Wall reduction |
| ------------- | --------: | -------: | ------: | -------------: |
| 36 optimal    |  18.733 s | 15.326 s |   1.22x |          18.2% |
| 115 all L     | 101.749 s | 57.597 s |   1.77x |          43.4% |
| 115 minimum L |   7.510 s |  5.153 s |   1.46x |          31.4% |
| 238 all L     |  40.180 s | 20.453 s |   1.96x |          49.1% |
| 238 minimum L |  20.067 s | 10.193 s |   1.97x |          49.2% |
| 238 optimal   |  19.948 s | 10.102 s |   1.97x |          49.4% |

Each row is one randomized sample per frozen binary. The consistent direction and
large effect across optimal, minimum-link, full enumeration, cyclic, and acyclic
completed workloads support promotion. The 115 all-L first witness improved from
8.021 to 5.143 seconds. The 238 all-L first witness improved from 20.189 to 10.337
seconds.

The hard-10 pair hit the common 60-second cap with no witness, so it supplies no
completion-speed claim. The candidate processed 1,281,513 states versus 883,264 and
3,141,365 decisions versus 2,206,691. Sampled peak working set rose from 2.17 to
2.45 GB. Its measured open-port graph-canonicalization time fell from 295.6 seconds
of aggregate worker time to 0.0008 seconds. More work then reached state
canonicalization within the fixed wall interval.

## Decision

Make the candidate permanent. It improves every completed workload by 18.2-49.4%
and preserves all exact comparison fields. The hard diagnostic shows higher memory
while advancing substantially more search work; memory is not a promotion gate for
this project. Root and frontier identities remain unchanged. Scheduling work stays
paused.
