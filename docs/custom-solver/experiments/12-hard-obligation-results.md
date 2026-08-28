# 12 results - Hard work and the remaining tail

Date: 2026-08-28. [Protocol](12-hard-obligation-profiling.md).
These are single instrumented diagnostic samples, not speedup measurements.

## Verification and provenance

Run: `target/parallelism-ladder/hard-obligations-20260828/`.
All 24 scheduled results reverified: 14 locally exhausted, ten capped incomplete,
zero watchdog kills, zero nonempty stderr logs, zero dropped trace records.
Total process time was 14.028 minutes. All four tiny controls exhausted.
No hard whole solve completed; profile exhaustion has only the selected scope.

Launch revision: `1b558a6a1a5281e63467d9daac1e4c24b011dd64`.
Frozen executable SHA-256:
`8534abaa30b271451693cb405790486983a23faf6caa8d56c890e7f207abbe51`.
The 34 frozen source/build files and both scripts match the launch commit,
allowing line-ending differences. The frozen binary matches its recorded hash.
The launcher did not create `working-tree.txt` or `working-tree.diff` for empty
Git output; source comparisons supply the relevant check, not those absent files.

`summary-rechecked.json` equals original `summary.json`; the original is unchanged.
`analysis-rechecked.json` records derived totals, bucket shares and root samples.
Exact saved solution maps, preferred witnesses and local exhaustion were checked.
The four partial 36 all-mode witnesses match between baseline and p1; both best
runs retain the minimum of that observed set. This does not prove it is complete.

## Exact-group observations

All hard groups used 32 workers and a 90 s search cap. Wall time includes cleanup.
Profiles completed and witnesses refer only to the selected exact N/L.

| Case, N/L | Mode | Stage    | Wall s | CPU s  | Peak MiB | Profiles done | Witnesses |
| --------- | ---- | -------- | ------ | ------ | -------- | ------------- | --------- |
| 36, 9/12  | best | baseline | 90.065 | 668.4  | 255.9    | 3/5           | 1         |
| 36, 9/12  | all  | baseline | 90.056 | 673.3  | 245.2    | 3/5           | 4         |
| 36, 9/12  | best | p1       | 90.012 | 1348.1 | 258.9    | 4/5           | 1         |
| 36, 9/12  | all  | p1       | 90.018 | 1352.6 | 261.4    | 4/5           | 4         |
| 10, 11/18 | best | baseline | 90.056 | 268.6  | 333.9    | 6/7           | 0         |
| 10, 11/18 | all  | baseline | 90.052 | 268.7  | 337.2    | 6/7           | 0         |
| 10, 11/18 | best | p1       | 91.408 | 2657.8 | 2214.1   | 6/7           | 0         |
| 10, 11/18 | all  | p1       | 91.262 | 2639.3 | 2127.4   | 6/7           | 0         |

P1 exhausts more 36 profiles, but no exact group finishes. More work or CPU does
not establish shorter completion. These modes are not repeated samples of each other.

## 36 has a witness-canonicalization tail

P1 completes 391/392 registered roots, leaving one root in profile
`S2=3,S3=3,M2=2,M3=1` incomplete. Root counts are not percentages of remaining work.
In all mode, active search roots fall from 32 at 30 s to 20 at 40 s, nine at
50 s, three at 60 s, two at 70/80 s and one at 89 s. The last root is inside
`witness_canonicalize` from 77.853 s until cancellation around 90.010 s.
Best mode also ends inside witness canonicalization in that same profile/root.
Existing group scheduling cannot split this serial operation.

Seven witness-canonicalization spans total 142.871 s in all mode and 148.638 s
in best mode across workers. Leaf relabeling/encoding accounts for 93.96% and
94.35% of those totals, with about 16.2-16.4 million examined leaves per run.
The old ordering-only refinement is already removed; this is the remaining leaf cost.

In the all/p1 top-level timer sum, legal-decision work accounts for 45.0%, state
canonicalization 30.0%, propagation 13.1%, and complete-candidate work 9.9%.
Aggregate cost and the final critical path therefore point to different priorities.

## 10 has distributed exact-search work

Six profiles are immediately rejected by production lower-bound checks, before
structural decisions. Only `S2=5,S3=2,M2=0,M3=4` remains expensive. This follows
normalized problem/profile facts, not the benchmark's cyclic label.

Baseline has three active roots throughout the sampled search. P1 has 32 at every
sample from 1 to 89 s, with about 29 CPU seconds per process second. One of its
105 roots exhausts; 32 run until cancellation and 72 are never started.
This shows a broad workload, not starvation of the worker pool after partitioning.

Best/p1 top-level shares: state canonicalization 32.7%, legal decisions 27.6%,
propagation 23.8%, SCC canonicalization 13.2%. There are no witness leaves.
Nested exact-basis timers record 292.2 s for equalities and 662.8 s for inequalities;
labeling records 561.4 s. Compact row writing is only 23.5 s.
These are accumulated elapsed timers across workers, not CPU or additive wall costs.

## Useful completed workloads and measurement limits

36's separate all/p1 profiles provide complete, empty witness-set references:

| S2,S3,M2,M3 | Wall s | Roots |
| ----------- | ------ | ----- |
| 1,4,4,0     | 2.831  | 49    |
| 4,2,3,0     | 18.234 | 73    |
| 5,2,0,2     | 26.749 | 65    |
| 6,1,1,1     | 9.034  | 76    |

The remaining 36 profile hits 30 s; 10's expensive profile hits 30.327 s.
The other six 10 profiles take under 1 ms and are correctness controls, not useful
performance workloads. No harder user input is needed yet.

Planning takes at most 0.133 s. Maximum search-cap overshoot is 1.409 s, in 10 p1;
there is no long cancellation tail in this run. Trace timers overlap and include
wall scheduling effects. Tiny zero CPU/memory samples reflect measurement resolution.
The profile diagnostic fields for root partitions/planning are zero here; use
explicit root results and `root_planning` spans, not those fields, for these claims.

See [the next experiments](12-next-experiments.md) for the proposed implementation
order, correctness gates and bounded verification plan. No new optimization or
scheduler default was applied during this analysis.
