# 13 results - Isolated calculation changes

Date: 2026-08-28. Both phases finished and independently reverified, 80/80 jobs.
The three candidates remain active and uncommitted. Recommend separate promotion
commits, each with its tests and related docs. No solver or scheduling change was
made during analysis. [Implementation](13-calculation-changes.md),
[protocol](13-calculation-screen.md), [whole solves and next steps](13-whole-results.md).

## Evidence and verification

Run root: `target/parallelism-ladder/calculation-screen-20260828/`.
The 52 fixed/replay jobs contain 38 local exhaustions, six capped incomplete jobs,
six completed witness replays and two requested replay cancellations.
The 28 whole jobs contain 26 optimal completions and two capped enumerations.
Total measured process wall time is 28.669 minutes, excluding gaps between jobs.
No verification failures or watchdog kills occurred.

Reran the frozen verifiers and checked schedule identities, inputs, binary/source
hashes, local proof accounting and available exact witness references. Original
summaries are unchanged and equal the rechecked summaries. Whole binary mappings
match the intended reference/basis snapshots. Current Rust sources match the
measured basis snapshot after newline normalization. All recorded traces dropped
zero events. The only nonempty stderr files contain expected whole-run timer messages.

Evidence: `summary.json`, `summary-rechecked.json`, `process-metrics.csv`,
`analysis-rechecked.json`, `verification-rechecked.log`, `results/`, `variants/`,
and the corresponding `whole-results/` files. Full witness objects are compared,
not just counts. A completed reference is not available for every hard SAT scope.

## Earlier exact-L rejection

Isolated reference versus exact_l, 36 N=9/L=12, profile 4,2,3,0, p1/32.
Tuple order is S2,S3,M2,M3. Three uninstrumented fresh processes per mode/variant.
All runs exhaust the same 73 roots and return the same empty witness set.

| Mode | Reference median [range], s | Exact-L median [range], s | Time reduction |
| ---- | --------------------------- | ------------------------- | -------------- |
| best | 20.230 [19.710, 20.698]     | 4.541 [4.534, 4.554]      | 77.6%, 4.45x   |
| all  | 19.959 [19.481, 20.359]     | 4.496 [4.491, 4.589]      | 77.5%, 4.44x   |

Median process CPU falls from 84.484 to 67.828 s in best and 83.359 to 67.594 s
in all, about 19-20%. Sampled peak memory remains around 68-72 MiB.

Separate diagnostics confirm the proposed cause. The reference makes two full
witness calls, visiting 2,654,208 leaves and spending 16.808 s in witness
canonicalization. Exact-L makes zero such calls. Both exhaust the same profile
with no retained witness. Together with the focused mismatch tests, this confirms
that expensive labeling was performed for candidates rejected by the existing L check.

Decision: strong evidence to make this change permanent. The large wall gain
comes from removing serial work; it does not require more occupied workers.

## Cached full-witness leaves

Isolated exact_l versus witness, one saved validated 36 N=9/L=12 graph,
three completed instrumented replays per variant.

| Variant | Canonicalization median [range], s | Median leaf time, s |
| ------- | ---------------------------------- | ------------------- |
| exact_l | 15.845 [15.734, 15.897]            | 15.072              |
| witness | 1.066 [1.062, 1.225]               | 0.515               |

Replay improves 14.87x. Every completed replay retains the same exact public key,
2,985,984 leaves and 13,214,106 branches. The examples validate the graph before
and after canonicalization. This complements the independent/differential tests;
it is one witness, not a distribution of whole-solver speedups.

Both requested 20 ms cancellations return incomplete with no key. Actual timer
requests occur at 26.6/32.7 ms; return follows promptly. These are cancellation
checks, not evidence that a 20 ms wall deadline is exact.

Decision: strong evidence to make cached leaves permanent. Public identity and
permutation coverage stay unchanged. Further witness search changes are lower
priority until a current profile shows that this phase still limits completion.

## Direct RREF bounds

Isolated witness versus basis, 36 N=9/L=12, profile 5,2,0,2, p1/32.
Three uninstrumented processes per mode/variant. Every run exhausts all 65 roots
with the same empty witness set; there is no full-witness work in this profile.

| Mode | Witness median [range], s | Basis median [range], s | Time reduction |
| ---- | ------------------------- | ----------------------- | -------------- |
| best | 34.461 [34.180, 34.618]   | 31.950 [31.609, 32.715] | 7.3%           |
| all  | 34.739 [33.885, 35.234]   | 31.864 [31.540, 32.031] | 8.3%           |

Median CPU falls 6.6% in best and 6.3% in all. Sampled peaks span 112-118 MiB.
Separate diagnostics make exactly 1,444,472 graph canonicalization calls in each
variant. Summed inequality time falls from 48.820 to 30.856 s, 36.8%.
Nested timers overlap other buckets; this is not a CPU percentage.

Decision: promote separately. The gain is smaller than witness replay but repeats
on completed, identical proof work in both collection modes. Exact-row differential
tests and whole SAT regressions support correctness. Empty profiles alone do not.

The eight tiny jobs all exhaust with agreeing exact results in 2.2-2.5 ms. They
are correctness controls, too short and insufficiently repeated for a speed claim.
Zero sampled CPU/memory in some tiny processes reflects measurement resolution.
