# 09 results. Compact encoding helps; enumeration remains unfinished

Date: 2026-08-28. State: analyzed. Protocol and provenance (`mem:solver/experiments/09-serializer-and-find-all`).
42 records verified: 34 optimal, eight cooperative capped incomplete, zero kills.
Measured process time totaled 40.10 minutes. No solver code changed for this analysis.

## Isolated encoding comparison

Both binaries use boxed keys and identical constructor code without a helper deadline.
Before encodes dense decimal rows; after encodes sparse exact binary rows. All rate
1200 and 32 workers. Timing samples have hotspots off. Median seconds and sample count:

| Case/mode/stage     | Samples per variant | Legacy | Compact | Time change |
| ------------------- | ------------------: | -----: | ------: | ----------: |
| 24 all baseline     |                   5 | 24.763 |  21.627 |      -12.7% |
| 24 all groups       |                   2 | 10.816 |  10.154 |       -6.1% |
| 65 all baseline     |                   2 | 24.034 |  21.393 |      -11.0% |
| 24 optimal baseline |                   1 |  1.773 |   1.555 |      -12.3% |
| 24 optimal p1       |                   2 |  0.902 |   0.791 |      -12.4% |
| 65 optimal baseline |                   1 |  1.315 |   1.166 |      -11.4% |
| 65 optimal p1       |                   2 |  0.450 |   0.410 |       -8.9% |

All seven medians favor compact encoding, with identical reported states/decisions.
For 24 all/baseline, legacy ranges 19.257-26.812 s and compact 20.123-24.624 s.
The ranges overlap; five samples on one machine are not a universal speed guarantee.
Median CPU falls 79.61 to 67.56 s, -15.1%. Peak sampled working set falls 234.25
to 41.63 MiB, -82.2%. Both explore 72,706 retained states and 328,052 decisions.

For 24 all/groups, ranges overlap at 10.088-11.543 versus 9.921-10.387 s; peak
memory falls 434.86 to 73.19 MiB. For 65 all/baseline, ranges are 23.353-24.714
versus 21.104-21.682 s; CPU falls 11.2% and peak memory 237.96 to 61.66 MiB.
Two samples each. The single baseline optimal samples are supporting observations.

The prior 6.2% regression compared different bundles. This experiment weakens the
hypothesis that compact row encoding caused it, but does not establish its cause.
Boxed storage is fixed here, so its independent effect is not measured. Do not
reinterpret the old comparison as an isolated serializer result or erase it.

## Diagnostic pair

One instrumented 24 all/baseline run per variant. Summed phase seconds, not CPU:

| Phase                       |         Legacy |        Compact |
| --------------------------- | -------------: | -------------: |
| Semantic encoding           |          7.631 |          0.405 |
| State canonicalization      |         27.953 |         24.763 |
| Partial graph labeling      |         12.851 |         15.067 |
| Equality / inequality basis | 5.006 / 10.778 | 6.032 / 12.867 |
| Legal decisions             |         21.553 |         25.489 |

Encoding itself is 18.8x cheaper in this pair. There are 582,760 graph canonicalization
calls and 479,232 witness leaves on both sides. Both traces have 912 records, none
dropped or left active. Four constructor attempts take 0.077/0.078 s, with four reuses.
State-cache destruction sums fall 0.0483 to 0.0106 s, a small part of this short run.

Whole-run diagnostic times are nearly identical, 22.342/22.327 s; compact uses 3.3%
more CPU in that pair and several unrelated phase timers rise. Do not use one
instrumented pair to override the repeated timing comparison or add nested timers.
The remaining cost suggests investigating canonicalization and exact basis work,
but hard-input diagnostics are needed before assuming they have the same bottleneck.

## Hard 36 scheduling

Two samples each, current compact binary, no helper deadline. All find-all runs
reach the 240 s cap with the same two full solution objects as experiment 08.

| Find-all stage/workers | First witness median [range] s | Median CPU s | Peak MiB |
| ---------------------- | -----------------------------: | -----------: | -------: |
| groups / 16            |        99.018 [95.703-102.333] |     1,887.85 |   547.57 |
| groups / 32            |         95.078 [93.339-96.817] |     2,989.16 |   806.41 |
| p14 / 16               |         38.999 [37.958-40.040] |     2,952.98 |   479.08 |
| p14 / 32               |         31.636 [31.088-32.183] |     5,086.59 |   661.48 |

P14/32 finds the first witness 3.01x sooner than groups/32, with 70.2% more CPU
and 18.0% lower peak memory. P14/16 gives 2.54x earlier first witness with 56.4%
more CPU. These are latency/cost observations, not enumeration speedups.

All eight prove N through 8, 20 link groups and 45 profiles exhausted, ending at
coordinator N=9/L=12. Exhausted roots differ, 173 for groups, 1,650 for p14/16,
2,029 for p14/32, because partition boundaries differ. They do not establish more
completed link/profile obligations. No hard find-all configuration finishes.

Hard optimal p1/32 takes 31.902 s, range 31.716-32.087, with the same N=9/L=11
witness and proof as before. Previous median was 31.378 s with a helper deadline;
this is a cross-run comparison, not an isolated deadline A/B. No large regression
appeared after removing it. Keep p1/32 as the next hard-optimal comparison.

## Decisions

1. Recommend permanent constructor eligibility/reuse/integer sums in one focused
   commit and compact exact keys in another. Earlier constructor evidence and its
   isolated tests support promotion; this screen is an encoding A/B, not an isolated
   constructor speedup. Keep the unproven five-second deadline out. No commit made here.
2. Retain p14/32 as the next hard find-all candidate for earlier witnesses. Keep
   scheduling defaults unchanged; full enumeration and 10-case completion remain unproved.
3. Next, profile fixed hard N/L/profile obligations around 36 N=9/L=12 and 10
   N=11/L=18. Use exact-accounting benchmark entry points, frozen inputs and explicit
   exhausted/incomplete statuses. Select smaller complete obligations from diagnostics
   where possible; a profile proof is not a global optimum or complete enumeration.
4. Use that evidence to target repeated canonical graph construction, exact basis
   generation or witness processing. Do not add a coarse queue just for occupancy,
   or spend the next cycle reversing an encoding that now has direct favorable evidence.

Evidence: `target/parallelism-ladder/serializer-scheduling-20260828/`, with
`results/summary-rechecked.json`, `verification-rechecked.log`, `analysis-metrics.json`
and `analyze-results.py`. Rechecked/original summaries match. Binary hashes match
preparation provenance; current Rust sources match the frozen snapshot. All 34
completed results and all eight partial solution sets match experiment 08 exactly.
