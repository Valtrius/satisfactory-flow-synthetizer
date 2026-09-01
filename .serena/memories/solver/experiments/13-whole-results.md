# 13 results - Whole solves and remaining hard work

Subsequent promotion is recorded in 14 (`mem:solver/experiments/14-calculation-promotion`). The results
and uncommitted-state descriptions below retain the analysis-time context.

Date: 2026-08-28. Verified results, not a new launch.
Isolated evidence and provenance (`mem:solver/experiments/13-calculation-results`),
protocol (`mem:solver/experiments/13-calculation-screen`). Reference versus combined basis candidate,
capacity 1200 and 32 workers throughout. Scheduler settings are held fixed within
each comparison. These data do not compare scheduling policies with each other.

## Actual optimal/all APIs

All timing jobs are uninstrumented. Short cases have three fresh processes per
variant; hard 36 has one. Values are solver wall seconds, median and sample range.

| Case/mode  | Scheduling | Reference median [range], s | Combined median [range], s | Time reduction     |
| ---------- | ---------- | --------------------------- | -------------------------- | ------------------ |
| 24 optimal | baseline   | 1.549 [1.538, 1.553]        | 1.414 [1.406, 1.418]       | 8.7%               |
| 24 all     | baseline   | 22.566 [20.380, 22.727]     | 20.333 [19.381, 20.539]    | 9.9%               |
| 65 optimal | baseline   | 1.164 [1.151, 1.166]        | 1.099 [1.093, 1.103]       | 5.6%               |
| 65 all     | baseline   | 21.369 [20.302, 22.003]     | 18.738 [17.613, 19.215]    | 12.3%              |
| 36 optimal | p1         | 33.274, one sample          | 24.972, one sample         | 25.0%, provisional |
| 36 all     | p14        | capped at 120.207           | capped at 120.213          | Not established    |

All 26 completed results agree with available exact references, including full
saved solution maps. Complete 24/65 all runs return six/thirteen layouts. The
hard optimum remains N=9/L=11 with the same preferred witness. The 24 all ranges
overlap slightly; three samples are not a universal performance guarantee.

Hard 36 all finds its first validated witness at 33.784 versus 25.071 s, 25.8%
sooner in one sample. Both caps retain the same two full solution objects and
stop while searching N=9/L=12. Neither establishes complete enumeration.
At the cap, process CPU rises from 2,343.859 to 2,733.891 s and sampled peak memory
from 445.9 to 556.2 MiB. More search in the same budget does not establish a
completion-speed or memory-efficiency improvement.

Short-case CPU medians improve in all four comparisons. Sampled memory is mixed:
24 optimal rises from a maximum 19.6 to 35.0 MiB; 24 all falls 45.8 to 44.2 MiB;
65 all falls 62.5 to 48.3 MiB. Do not promote these as universal memory savings.

## Completed hard N/L group

The separate fixed workload covers all five profiles of 36 N=9/L=12 with p1/32,
not the earlier N/L obligations or later enumeration groups. One process per cell.

| Collection | Reference, 110 s cap  | Combined            | Retained witnesses               |
| ---------- | --------------------- | ------------------- | -------------------------------- |
| best       | Incomplete, 110.010 s | Exhausted, 63.539 s | One each                         |
| all        | Incomplete, 110.018 s | Exhausted, 62.409 s | Five partial versus six complete |

Combined exhausts all 392 roots and five profiles. Reference stops with one root
unfinished in the SAT profile, 128/129 roots there. Its five all-mode witnesses
are an exact subset of the combined six; the preferred witness agrees. There is
no fully exhausted reference SAT profile in this screen, so the sixth witness is
validated without an independent complete-set reference. This is useful local
completion evidence, not a measured full-solve ratio or a global optimality claim.

The six complete fixed-group witnesses and two partial whole-run witnesses have
different scopes. They are not contradictory layout counts or interchangeable results.

## Hard 10 diagnosis

One instrumented 30 s sample per mode/variant, profile 5,2,0,4 at N=11/L=18, p1/32.
All four remain incomplete with no witness and 1/105 roots exhausted. Each has 32
roots active at cancellation. Post-request root tails are 0.303-0.346 s; no kill.

| Mode | Variant | Graph calls, millions | Summed inequality, s | Summed labeling, s |
| ---- | ------- | --------------------- | -------------------- | ------------------ |
| best | witness | 2.195                 | 201.951              | 192.409            |
| best | basis   | 2.210                 | 147.269              | 203.902            |
| all  | witness | 2.044                 | 186.398              | 194.355            |
| all  | basis   | 2.246                 | 140.596              | 206.421            |

These nested timers cover different unfinished search prefixes. They suggest less
inequality work per canonicalization, but cannot establish faster completion or
an exact per-state speedup. Graph labeling remains a substantial cost. The current
fixed runner selects N/L/profile, not an arbitrary exact partition prefix.

## Recommendations

1. Commit the three calculation changes separately, each with its tests and docs.
   Isolated repeat gains and whole SAT regressions support all three. Keep benchmark
   tooling/manifests in a separate commit. None was committed or pushed by this analysis.
2. Keep scheduler defaults unchanged. Compare p1 alone against p14 on actual hard
   36 all after these changes. Two fresh repeats per policy at 32 workers and a
   240 s cap would cost at most 16 minutes plus cleanup. Use a separate instrumented
   run to locate any remaining tail. Repeat hard optimal before treating 25% as stable.
3. For hard 10, add a benchmark-only exact partition-prefix workload with frozen
   profile, decision sequence, frontier identity and local exhaustion accounting.
   Select a prefix that completes in tens of seconds, then repeat it before/after
   another basis or labeling change. A root ordinal alone is not a portable identity.
4. Revisit bounded donation only if the new trace shows idle workers alongside an
   expensive DFS tail. The current 10 samples already keep 32 roots active, and
   this run does not trace the remaining whole-36 tail. There is no evidence here
   for a universal larger frontier, another pool or a new default scheduler.

Existing cases remain difficult enough. No hard single-worker baseline or new user
case is needed yet. Solver decisions must use normalized inputs and observed
runtime state, never the corpus's cyclic/acyclic labels. Compare completion time;
keep CPU, memory, first witness and capped progress as separate measurements.
