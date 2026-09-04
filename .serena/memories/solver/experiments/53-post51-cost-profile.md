# 53. Post-experiment51 hotspot cost-mix and root-tail profile

Date: 2026-09-04. State: analyzed. Related: `mem:solver/experiments/51-reachability-results`, `mem:solver/experiments/52-tail-scheduling-results`, `mem:solver/experiments/34-sparse-phase-profile-results`, `mem:solver/experiments/49-wall-time-priorities`.

## Question

After promoting experiment51, which remaining buckets own wall on production p1, and do activity traces show one fat remaining root or overlapping roots? Diagnostic only. Nested timers overlap and are not CPU or hotspot-off wall.

## Screen

Run `target/parallelism-ladder/post51-cost-profile-20260904`. Runner PID 51660. Started 14:40:58, finished 14:53:02+02:00, about 12m. Manifest `benchmarks/custom/post51-cost-profile.json`. 12/12 jobs, `failures=[]`. Binary SHA256 `5AD38F10AA914EA0C7E00A0CF291AC74C587F523F235D5FC1FA3CA42224612A6` matches launch. Frozen revision `7799c110e8266b96cb59dd0480b62e59c576cda7`. Affinity observed equals requested on every pinned job; `affinity_before_resume` true on 12/12; unrestricted 36 observed `ffffffff`. Official summary SHA256 `a9067c071d9150294a26c5e29f735f13efdfdbdd425a8ced900c727f287a9871`. All jobs hotspot-on. Do not compare these walls with experiment51 OFF timings.

10 completed reference jobs, 2 intended stress caps (`Incomplete(Cancelled)`). Completed identities match the known public results: 115 all 49 layouts `Optimal(N=7, L=10)`; 238 optimal 1 layout `Optimal(N=8, L=13)`; 258 minimum_links 2 layouts `Optimal(N=9, L=14)`; 36 optimal 1 layout `Optimal(N=9, L=11)`. Stress 36 all kept 8 layouts at the cap; stress 10 optimal kept none. Activity traces complete (`dropped=0`) on every job.

## Top-level accounted mix

Sibling search buckets only. Median within each cell. Propagation leads every completed cell. Reachability is 0.6–1.1% after experiment51. Canonicalization is state keys plus SCC; inequality encoding is 0 after experiment49.

| Cell                | Wall s |  CPU s | Accounted s |  Prop | State+SCC canon | Reach | RREF / accounted | Elim / RREF |
| ------------------- | -----: | -----: | ----------: | ----: | --------------: | ----: | ---------------: | ----------: |
| 115 all CCD96       |  44.92 |  464.4 |       394.3 | 66.8% |           26.5% |  0.7% |             9.5% |       62.9% |
| 115 all CCD32       |  40.88 |  432.3 |       365.5 | 66.8% |           26.5% |  0.7% |             9.4% |       62.9% |
| 238 opt CCD32       |   6.85 |   58.3 |        49.8 | 56.4% |           36.5% |  0.7% |            12.2% |       62.4% |
| 238 opt CCD96       |   7.44 |   63.8 |        54.7 | 56.2% |           36.7% |  0.7% |            12.1% |       62.3% |
| 258 min-links CCD96 | 180.15 | 1881.7 |      1210.4 | 65.8% |           28.7% |  0.6% |            10.2% |       64.5% |
| 36 opt 32w          |  11.28 |   98.7 |        79.1 | 53.7% |           38.1% |  1.1% |            10.8% |       61.8% |
| 36 all CCD96 cap    |  90.12 |  935.4 |       740.4 | 57.7% |           34.2% |  0.8% |            11.5% |       61.7% |
| 10 opt CCD32 cap    |  90.80 | 1430.1 |       610.5 | 47.3% |           45.4% |  0.7% |            13.6% |       64.3% |

Open-port plus marked-link graph time remains <0.2s except capped 36 (0.21s). Labeling is 22–24% of combined state+SCC canon on 115, not the leader. Equality is 75–81% RREF except 36 optimal (63.6%).

## Sparse mix after experiments 36–38

Preparation leftover besides substitution is tiny. Substitution is almost all of the preparation timer. Forward Bareiss grew on 115/258 because substitution already removed most rows.

| Cell      | Prop s | Analyze / prop | Prep / prop | Subst / prop | Forward / prop | Back / prop | Bounds / prop |
| --------- | -----: | -------------: | ----------: | -----------: | -------------: | ----------: | ------------: |
| 115 CCD96 |  263.4 |          48.1% |       15.1% |        13.7% |          23.3% |        6.8% |         18.7% |
| 238 CCD32 |   28.1 |          30.7% |       19.4% |        18.0% |           7.0% |        2.2% |         25.1% |
| 258 CCD96 |  796.7 |          56.1% |       12.4% |        11.1% |          32.6% |        8.3% |         15.0% |
| 36 opt    |   42.5 |          23.1% |       18.4% |        17.1% |           1.8% |        0.7% |         26.6% |

Forward / accounted is about 15.5% on 115 CCD96 and 21.4% on 258. RREF elimination / accounted is about 6.0% and 6.6%. Unbucketed sparse prep (prep minus subst) is ~1–2% of prop.

Sparse calls and shape (active rows after substitution):

- 115: 4,057,892 calls, avg in 26.67 → act 5.39. Rows 0–15: 90.9% calls / 65.8% analyze time. Rows 16–31: 9.1% / 33.6%.
- 258: 8,187,802 calls, avg in 32.40 → act 8.03. Rows 0–15: 77.7% / 38.6%. Rows 16–31: 22.3% / **61.0%**.
- 238: 484,676 calls, avg act 2.75. Rows 0–15: 99.8% / 98%.
- 36 opt: 660,458 calls, avg act 1.47. Rows 0–15: 100%.

Reanalysis remains ~32% value-deduction, <1% ratio, matching experiment34’s order of magnitude. Experiment34’s “forward is a minority, skip rollback-aware Bareiss” no longer describes 115/258 after substitution promotions. Matrices are still small (almost none ≥32 rows); the new cost is the leftover 16–31-row Bareiss systems on 115/258.

RREF operand pattern is unchanged from experiment47: 78–80% zero destinations, max bits 11–16 / 6–7. Nested, not a wall share.

## Activity / tails

Traces are wall spans. Peak overlap is concurrent roots, not CPU.

- **258:** 64 roots, peak 16, mean_active 10.5. Roots 7 and 22 cover 98–99% and 95% of wall with the same profile `{merger2:0, merger3:3, splitter2:5, splitter3:1}`. Three more sit at 49–62%. Overlapping long roots, not one unique leftover.
- **238:** 38 roots, peak 16, mean_active 8.5. Root 26 is 82% of wall; 27/9/7 also 53–60% concurrently. Same shape.
- **115:** 812 roots, peak 16, mean_active 10.2–10.6. Longest ~24% of wall; several distinct IDs (0, 43, 48) overlap. Busy concurrent search, not a serial tail.
- **36 optimal:** 1833 short roots, none above 11% of wall. `acyclic_construct` union 6.23s of 11.28s wall; root union only 4.57s. CPU util 0.27. Constructor, not a fat DFS tail, owns this cell’s wall. Skip-after-winning-N is already permanent.
- **Stress 10:** 17 roots all span the 90s cap, peak 16, mean_active 16.0, util 0.98, peak WS 2.65 GiB. Saturated overlapping, incomplete.
- **Stress 36 all:** 2170 roots, longest 43% of wall, plus 6.61s constructor. Cap as planned.

Cache-drop tails after cancel are <0.8s. Donated_tasks and shared_cache_hits remain 0. Deferred visits still dwarf promotions (115: 1.63M visits / 0.26M promotions).

## Decision

Diagnostic complete. No source change.

1. Next calculation experiment: isolate a `bareiss_forward` candidate aimed at leftover 16–31-row systems. That is the largest remaining nested timer on 115/258. Do not revive weighted quotients (experiment32), duplicate-row removal (33), rollback-aware Bareiss as the first try (34), or prep-template reuse (unbucketed prep is no longer material). Require 238/36 not to regress.
2. Next scheduler experiment, separate: completed-state sharing that keeps deferred canonicalization on owners. Traces show overlapping long similar roots on 238/258, so experiment52’s live private-cache DFS donation is the wrong shape. Any sharing candidate must merge helper `witnesses` maps (52’s all-mode identity bug) and must not force exact keys on first visits. Do not enable public `work_stealing` without that sharing.
3. Park fraction-free canonical RREF. Elimination is still ~63% of RREF, but RREF is only ~9–12% of accounted. Experiment50 already failed a conversion-based small-integer path. Zero-destination remains held.
4. Do not chase reachability, labeling, inequality encoding, production affinity, or 36 constructor as a general next step.

No commit of this analysis. Experiment51 remains permanent in `7799c11`, unpushed.
