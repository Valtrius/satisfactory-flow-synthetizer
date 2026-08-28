# 06. Recovery screen and cache destruction

Date: 2026-08-28. State: analyzed; screen failed verification due to three kills.
Hypothesis: preserve open-operation evidence through cancellation to distinguish
partial labeling, search unwind and cache destruction before choosing a fix.

## Changes and comparison

Recovery changed diagnostics and the runner, not mathematical or scheduler decisions:
open phase spans, per-thread sampled partial-labeling slots, and complete numbered
post-cancellation sidecars every 15 s. Sidecars are diagnostic, not final results.
Failed jobs now enter `failed-jobs.json`; the runner continues and the analyzer
rejects the failed screen even with `--allow-incomplete`.

Same 12 solver jobs as experiment 05, hotspots on, 32 workers, max rate 1200.
Skipped completed witness replays; cancellation grace returned to 60 s.
All jobs were attempted. Nine returned optimal results; three were killed.

## Completed results

All nine preserved earlier statuses, full layout keys, preferred keys and complete
saved solution objects. Rechecked analyzer summary and executable/source identity matched.
Single-sample seconds; earlier hard screen had hotspots off, recovery had them on:

| Case/mode/stage     | Earlier hard screen | Recovery |
| ------------------- | ------------------: | -------: |
| 36 optimal baseline |             418.515 |  244.974 |
| 24 all baseline     |              35.842 |   21.379 |
| 24 all groups       |              13.623 |   11.946 |
| 65 all baseline     |              31.516 |   24.406 |
| 65 all groups       |              16.105 |   11.158 |

These were bundle observations, not isolated attribution or a controlled profiling
comparison. The hard optimum remained N=9/L=11. Constructor time was 143.962 s,
about 58.8% of its optimal solve, motivating exact subset arithmetic optimization.

## Failures and what they establish

| Job                 | Search cap s | Watchdog return/kill s | Peak GiB |
| ------------------- | -----------: | ---------------------: | -------: |
| 10 optimal baseline |          180 |                240.299 |     6.68 |
| 10 optimal p1       |          180 |                240.690 |    11.55 |
| 36 all groups       |          600 |                660.470 |     8.81 |

10 baseline cancelled at 180.007 s. All three roots left search by 180.009 s and
entered state-cache destruction. At activity time 196.943 s all three drops were
still open with no active search/partial labeling. This establishes at least
16.93 s of cache destruction, not the full missing 60 s before the kill.

36 all showed 16 workers dropping state caches and five unwinding search at
cancellation. One completed state-cache drop during search took 7.304 s.
10 p1 saved no live sidecar, so its blocking phase remained unknown. Allocating
JSON writers were themselves unreliable under this pressure.

Allocator contention was plausible, but no allocator stack or paging trace proved
it. High working set is process memory, not cache payload. No allocator replacement
or unbounded detached cleanup was justified by this evidence.

Full-witness work also remained costly: the 36 live trace reached 142,079,406
leaves, with some witness calls interrupted after minutes. Cheaper leaves do not
remove factorial growth or authorize dropping public-key permutations.

## Decision and evidence

Reduce exact state/SCC key storage first, optimize helper subset sums, and retain
an explicit optional-attempt budget. Add a nonallocating heartbeat independent of
the trace mutex. Then compare 16/32-worker scheduling on the resulting kernel.
Keep defaults unchanged; all existing hard cases remain useful.

Local evidence: `target/parallelism-ladder/recovery-20260828/`, especially
`results/failed-jobs.json`, surviving result JSON, live sidecars, `summary.json`,
binary hashes and `solver-source/`. A killed job never becomes an incomplete
returned solver result. Follow-up: [07, compact keys and constructor](07-compact-keys-and-constructor.md).
