# 14 - Promote the measured calculation changes

Date: 2026-08-28. User approved promotion after experiment 13.
[Isolated evidence](13-calculation-results.md), [whole regressions](13-whole-results.md).
Benchmark tooling and result history were committed separately in `473aba7`.
No push is requested or performed.

## Separate source changes

| Change                    | Promotion state | Evidence                                   |
| ------------------------- | --------------- | ------------------------------------------ |
| Earlier exact-L rejection | `5ae5631`       | 4.44-4.45x faster completed target profile |
| Cached witness leaves     | `1fbe95d`       | 14.87x replay, identical key and coverage  |
| Direct RREF bounds        | `af1682c`       | 7.3-8.3% shorter completed proof workloads |

The exact-L commit contains only `search.rs` and related documentation. It retains
the focused mismatch/cancellation test and a local Clippy line-count allowance
that keeps the failure sequence together. The allowance was present in the measured
combined variant and does not change runtime behavior.

The witness commit includes its cached encoder and differential byte tests in
`canonical.rs`. The basis commit adds only direct RREF bounds and their differential tests.
The frozen sequential patches preserve their independent review/build boundaries.
No benchmark label, scheduling default, public witness identity or proof rule changes.

## Verification

Experiment 13 tested each cumulative source before measurement: 209 tests for
exact-L and witness, 210 for the combined basis source, two ignored each. Strict
Clippy passed for witness/basis and the final default-feature source. The 80-job
screen passed independent verification, including exact saved witness comparisons.
See [the implementation record](13-calculation-changes.md) for the retained logs.

Promotion does not introduce another algorithm change. Source hunks are those
measured in experiment 13. `npm run format` runs before each commit. Benchmark
patch files retain required blank context lines; whitespace checks exclude those
patch payloads, not Rust or documentation files.

## Follow-up

Keep scheduling opt-in. Next compare p1/p14 after these improvements and add exact
prefix workloads for hard 10. A completed subtree is not a whole-profile proof.
All three source changes are now permanent. [Prefix workloads](15-prefix-workloads.md)
and the [scheduling screen](16-post-calculation-screen.md) continue the work.
