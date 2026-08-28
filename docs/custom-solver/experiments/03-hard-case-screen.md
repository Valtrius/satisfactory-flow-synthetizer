# 03. Bounded hard-case screen

Date: 2026-08-28. State: analyzed; cyclic cutoff design corrected in experiment 04.
Hypothesis: earlier scheduler choices extend to difficult completion workloads.
New inputs: 36 = 11+9+7+5+3+1 and 10 = 6.04+3.96, with exact max rate 1200.

## Comparison

Manifest `benchmarks/custom/screening.json`, 24 fresh-process single samples,
hotspots off, no single-worker baseline. Nominal search caps totaled 91.5 minutes;
measured process time was 68.82 minutes. Each job allowed 60 s cancellation grace.

| Cases  | Mode           | N cap | Time cap | Stages/workers                       |
| ------ | -------------- | ----: | -------: | ------------------------------------ |
| 24, 65 | optimal        |  7, 6 |     15 s | baseline/p1/p12 at 32                |
| 24, 65 | all            |  7, 6 |     60 s | baseline/groups/p14 at 32            |
| 36     | optimal        |     9 |    480 s | baseline/p1/p12 at 32, p1 at 16      |
| 36     | all            |     9 |    600 s | baseline/groups/p14 at 32, p14 at 16 |
| 10     | optimal prefix |    10 |    180 s | baseline/p1/p12 at 32, p1 at 16      |

## Results

All 24 records verified: 16 optimal, four deadline-cancelled enumerations, four
lower-bound rejections with no search. All 36 optima had the same validated N=9/L=11 witness.

| 36 optimal stage | Workers | Completion s | Process CPU s | Peak MiB |
| ---------------- | ------: | -----------: | ------------: | -------: |
| baseline         |      32 |       418.51 |        626.53 |      654 |
| p1               |      32 |       360.62 |        754.19 |      456 |
| p1               |      16 |       353.96 |        703.56 |      358 |
| p12              |      32 |       346.86 |        739.33 |    1,741 |

Partitions improved the observed sample about 14-15%. Sharing added zero cache
hits, unchanged state/decision counts, about 1.25 GB retained cache payload and
1,285 MiB more peak process memory. Its extra 13.76 s apparent gain was not proof
of cache benefit. Nor did one 16-worker sample establish a worker-count policy.

All four 36 enumerations returned the same two partial layouts and remained incomplete:

| Stage    | Workers | First witness s | Return at 600 s cap | Peak MiB |
| -------- | ------: | --------------: | ------------------: | -------: |
| baseline |      32 |          417.22 |              600.01 |      808 |
| groups   |      32 |          428.08 |              639.22 |    3,261 |
| p14      |      32 |          346.69 |              636.46 |    3,235 |
| p14      |      16 |          341.98 |              639.82 |    3,282 |

Parallel variants had 36-40 s cancellation tails; baseline's was about 0.014 s.
More states/roots were not completion wins. Coordinator N=9/L=12 did not describe
every concurrently searched group. Optimal averaged 1.50-2.13 CPU-core equivalents,
all averaged 1.52-3.43, without identifying the cause of low aggregate utilization.

The smaller full sets still matched. At 32 workers, 24 all baseline/groups/p14
took 35.84/13.62/9.84 s; 65 took 31.52/16.10/15.91 s.

## Failed hypothesis and corrected design

The 10 starting proven lower bound was 11. N<=10 returned in 0.06-0.11 ms with
zero profiles, roots or decisions. This was a valid bounded proof but a benchmark
design error, not an optimization. Raise the cap to 11 before comparing scheduling.

Every hard optimal sample spent 43.41-46.27 s after its last `ValidatingWitness`
event. That interval included canonicalization, validation and rate restoration;
it was not yet a measurement of canonicalization alone. Full witnesses use
exhaustive labeling; partial Canonaut calls were a separate cancellation hypothesis.

## Decision and evidence

Measure saved-witness phases and root teardown before implementing a shared coarse
pool. Keep p1 as the difficult-optimal comparison, sharing/donation optional, and
all defaults unchanged. No harder cases or costly single-worker runs were needed.

Local evidence: `target/parallelism-ladder/hard-screen-20260828/results/`, including
`summary.json`, per-run full solutions, progress, keys and hashes; source snapshots
and runner files in its parent. Follow-up: [04, isolated diagnostics](04-witness-constructor-diagnostics.md).
