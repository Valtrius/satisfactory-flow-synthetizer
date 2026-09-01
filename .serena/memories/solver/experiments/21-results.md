# 21 - Adaptive root-tail results

Date: 2026-08-29. State: completed and independently reverified.
Protocol: adaptive root-tail profiling (`mem:solver/experiments/21-adaptive-root-profiling`).

## Integrity

The six serial jobs completed without process failure, watchdog kill or open
activity span. `hard-profile.py verify` accepts 6/6 fixed workloads and confirms
that exhaustion is local to the selected root. Reverification left `summary.json`
byte-identical at SHA-256
`4d145762e64ad3ff96668439b74f68535c56a8239697345c23577a27ff5ccb04`.

Every request reconstructs the same 38-key production p1 plan at target 128 and
selects ordinal 28. All jobs return the same single exact witness. Best and all
also report identical structural work: 73,795 raw decisions, 21,779 retained
canonical states, 3,207 canonical duplicates and 8,320 SCC solves.

## Timings

Solver wall time excludes roughly 0.013-0.014 seconds of preparation.

| Mode | Ordinary samples | Median   | Range           |
| ---- | ---------------- | -------- | --------------- |
| best | 2                | 50.034 s | 48.871-51.197 s |
| all  | 2                | 51.665 s | 51.561-51.769 s |

The diagnostic samples take 55.033 seconds for best and 53.844 seconds for all.
These are instrumented single samples and do not enter the ordinary medians.
The old 82.731-second whole-solve root span included concurrent contention or
aggregate trace effects; it is not an isolated-root baseline.

## Hotspots

The all diagnostic records 53.838 seconds in root search. Major measured phases:

| Calculation                               | Time or count                 |
| ----------------------------------------- | ----------------------------- |
| Witness search                            | 33.654 s                      |
| Graph canonicalization, overlapping total | 45.013 s across 160,886 calls |
| Algebra                                   | 5.645 s                       |
| Legal decisions                           | 6.773 s                       |
| Propagation synchronization               | 5.443 s                       |
| State labeling/canonicalization           | 9.988 s combined              |
| Witness branches                          | 338,946,509                   |
| Witness leaves                            | 80,621,568                    |

Best has the same branch and leaf counts and a 34.403-second witness search.
The authoritative minimum witness key therefore requires the same exhaustive
labeling even when only the preferred solution is retained. `collect_all` is not
the cause of this root tail; the ordinary best/all difference is run variance.

## Decision

Optimize full-witness labeling next while preserving the public reference byte
protocol. The leaf count factors as `4! * 3! * 2! * (3!)^7`: node permutations
contribute only 144 leaves, while independent symmetric physical-port permutations
multiply them by 559,872. Experiment 22 (`mem:solver/experiments/22-analytic-witness-ports`) tests exact
analytic port minimization. Scheduler work remains paused.
