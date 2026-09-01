# 08 results. Repeats and worker-count comparison

Date: 2026-08-28. State: analyzed. Protocol and provenance (`mem:solver/experiments/08-repeat-scheduling`).
Follow-up: 09 results (`mem:solver/experiments/09-serializer-and-find-all-results`) favor isolated compact
encoding; the earlier bundle regression below remains an unexplained historical result.
62 records verified: 56 optimal, six cooperative timeouts, zero watchdog kills.
Measured process time totaled 31.60 minutes. All timings below have hotspots off,
max rate 1200 and fresh randomized processes. No solver changes were made for analysis.

## Short-case bundle comparison

Three samples per configuration, 32 workers. Median seconds with min-max range:

| Case/mode/stage     |  Before median [range] | Current median [range] | Median time change |
| ------------------- | ---------------------: | ---------------------: | -----------------: |
| 24 optimal baseline |    1.910 [1.894-1.951] |    1.564 [1.531-1.598] |             -18.1% |
| 24 optimal p1       |    1.067 [1.066-1.089] |    0.805 [0.788-0.813] |             -24.5% |
| 24 all baseline     | 20.134 [19.991-26.272] | 21.388 [21.226-21.514] |              +6.2% |
| 24 all groups       |  11.282 [9.242-11.375] |   9.996 [9.933-10.172] |             -11.4% |
| 65 optimal baseline |    1.292 [1.282-1.317] |    1.176 [1.132-1.194] |              -9.0% |
| 65 optimal p1       |    0.446 [0.442-0.473] |    0.398 [0.393-0.412] |             -10.9% |
| 65 all baseline     | 23.303 [21.555-23.889] | 20.560 [20.539-20.624] |             -11.8% |
| 65 all groups       | 12.451 [11.820-12.495] |   8.561 [8.203-10.563] |             -31.2% |

Seven of eight median comparisons favor the bundle; not every timing range is
separated. All eight have identical reported before/after state and decision counts,
as well as exact statuses, keys, preferred witnesses and full saved solutions.

The 24 all/baseline regression signal persists: median process CPU rose 59.75 to
67.86 s, +13.6%, for 72,706 retained states and 328,052 decisions in both versions.
All current samples are slower than the two approximately 20 s reference samples;
the reference also had a 26.272 s outlier. This supports investigating extra
computation, but does not isolate serialization from the other bundle changes.

Memory still improved. Maximum sampled working set for 24 all/baseline fell
220.75 to 57.84 MiB, -73.8%; groups fell 441.95 to 68.27 MiB, -84.6%.
65 all/baseline fell 242.04 to 45.70 MiB, -81.1%; groups 472.28 to 79.06 MiB, -83.3%.
Do not discard those storage gains or describe the whole bundle as universally faster.

## Hard 36 find-optimal

Two samples per configuration, all the same validated N=9/L=11 solution:

| Stage/workers |       Median s [range] | Median CPU s | Peak MiB |
| ------------- | ---------------------: | -----------: | -------: |
| baseline / 16 | 95.160 [93.820-96.500] |       280.06 |    79.71 |
| baseline / 32 | 92.868 [91.132-94.604] |       273.45 |    78.42 |
| p1 / 16       | 37.530 [36.963-38.097] |       339.84 |    67.67 |
| p1 / 32       | 31.378 [30.698-32.059] |       377.15 |    87.30 |

P1 at 32 workers is 2.96x faster than baseline at the same budget, using 37.9%
more process CPU. Compared with p1/16, it takes 16.4% less time, 11.0% more CPU
and about 20 MiB more peak memory. The large separated timing ranges support this
case's latency tradeoff; two repeats on one machine do not establish universal defaults.
First validated-witness time is effectively terminal optimal time in these runs.

All eight proofs exhaust N through 8, 19 link groups and 41 profiles. Root-proof
counts differ with partitioning, as expected. The final validating-phase remainder
is about 6.3-7.3 s across configurations; it is a broad phase interval, not an isolated timer.

## Capped hard searches

Single samples, explicitly incomplete. Memory is sampled process peak:

| Case/mode/stage/workers  | Cap / return s | First witness s | Layouts | Peak MiB |
| ------------------------ | -------------: | --------------: | ------: | -------: |
| 36 all groups / 16       |  240 / 240.152 |          78.631 |       2 |   735.57 |
| 36 all groups / 32       |  240 / 240.278 |          89.984 |       2 |   806.82 |
| 10 optimal baseline / 16 |  120 / 120.059 |            None |       0 |   442.15 |
| 10 optimal baseline / 32 |  120 / 120.071 |            None |       0 |   557.56 |
| 10 optimal p1 / 16       |  120 / 120.527 |            None |       0 | 1,791.92 |
| 10 optimal p1 / 32       |  120 / 121.581 |            None |       0 | 2,619.19 |

Both 36 runs have the same two earlier partial solution objects and proof summary:
N exhausted through 8, 20 groups, 45 profiles and 173 roots. Coordinator progress
is N=9/L=12. The 16-worker sample used less memory/CPU and found its first witness
earlier, but one capped sample cannot establish faster full enumeration.

All four 10 runs remain at N=11/L=18 with three groups, 13 profiles and 13 roots
proved exhausted. P1/32 reported 3.03 million decisions versus baseline/32's
0.614 million, but neither advanced the completed proof summary or found a witness.
Higher activity is not a measured completion gain. At these caps all cleanup
returned normally; nominal cap overshoot is not exact cancellation latency.

## Decisions and next tests

1. Keep p1/32 as the next hard find-optimal comparison. Test p14, partitions plus
   remaining groups, on hard 36 find-all before implementing a new coarse scheduler.
   Groups alone cannot speed the initial SAT-group search. Keep automatic defaults off.
2. Retain constructor eligibility/reuse/integer arithmetic as promotion candidates.
   Isolate them from compact serialization and the unproven five-second budget before
   committing. No new isolated kernel speedup is established by this bundle comparison.
3. Profile and A/B the 24 all/baseline cost with fixed scheduling and constructor
   policy, preserving compact storage's memory benefit. Do not commit all remaining
   optimizations together or dismiss the regression as noise.
4. For 10, compare equal bounded proof work and diagnose expensive kernels; do not
   add workers or a coarse queue solely to raise occupancy. Existing hard inputs suffice.

Evidence: `target/parallelism-ladder/repeat-scheduling-20260828/results/summary.json`,
`summary-rechecked.json` in the same results directory, `verification-rechecked.log`
and `analysis-metrics.json` in the run root. The analyzer summaries match exactly;
current Rust sources match the snapshot and hard-36 solution objects match experiment 03.
