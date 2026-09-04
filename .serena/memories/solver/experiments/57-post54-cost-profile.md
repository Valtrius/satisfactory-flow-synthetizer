# 57. Post-experiment54 hotspot cost-mix profile

Date: 2026-09-04. State: analyzed. Related: `mem:solver/experiments/53-post51-cost-profile`, `mem:solver/experiments/54-bareiss-forward`, `mem:solver/experiments/56-fraction-free-rref`, `mem:solver/experiments/49-wall-time-priorities` item 2.

## Question and hypothesis

After promoting Bareiss-forward (experiment 54), which remaining nested buckets own wall on production p1, and does sparse preparation still dominate enough to justify immutable per-problem/profile input templates and scratch reuse?

Hypothesis: experiment 53 is pre-54. Forward was then ~15–21% of accounted on 115/258 leftover 16–31-row systems; unbucketed prep was ~1–2% of propagation. Nested timers overlap and are not CPU or hotspot-off wall.

## Screen

Run `target/parallelism-ladder/post54-cost-profile-20260904`. Runner PID 48508. Started 21:31:30, finished 21:42:23+02:00, about 11m. Manifest `benchmarks/custom/post54-cost-profile.json`. 12/12 jobs, `failures=[]`. Binary SHA256 `D8A433AD6E216194D0B6CE70BC3E23B22B3F86B6FADAEA3FA9A34EB163376D09` matches launch. Frozen revision `2af30a63260e8238a00039541b6fbcb1932d8c11` (`crates/solver-core` matches `d6902a4`). Affinity observed equals requested on every pinned job; `affinity_before_resume` true on 12/12; unrestricted 36 observed `ffffffff`. Official summary SHA256 `f83b3e632744b49082174e7e8d1ea775a37f2c6f39f85fde2adaab3710e26240`. All jobs hotspot-on. Do not compare these walls with experiment 54 OFF timings.

10 completed reference jobs, 2 intended stress caps (`Incomplete(Cancelled)`). Completed identities match the known public results: 115 all 49 layouts `Optimal(N=7, L=10)`; 238 optimal 1 layout `Optimal(N=8, L=13)`; 258 minimum_links 2 layouts `Optimal(N=9, L=14)`; 36 optimal 1 layout `Optimal(N=9, L=11)`. Stress 36 all kept 8 layouts at the cap; stress 10 optimal kept none. Activity traces complete (`dropped=0`) on every job.

## Top-level accounted mix

Sibling search buckets only. Median within each cell. Propagation still leads every completed cell except capped 10. Reachability remains 0.6–1.1%. Canonicalization is state keys plus SCC; inequality encoding is 0 after experiment 49.

| Cell                | Wall s |  CPU s | Accounted s |  Prop | State+SCC canon | Reach | RREF / accounted | Elim / RREF |
| ------------------- | -----: | -----: | ----------: | ----: | --------------: | ----: | ---------------: | ----------: |
| 115 all CCD96       |  38.69 |  411.7 |       345.4 | 62.5% |           30.1% |  0.8% |            10.5% |       62.7% |
| 115 all CCD32       |  36.56 |  387.0 |       321.3 | 62.4% |           30.3% |  0.8% |            10.5% |       62.7% |
| 238 opt CCD32       |   6.66 |   56.5 |        48.2 | 54.7% |           38.1% |  0.7% |            12.3% |       62.1% |
| 238 opt CCD96       |   7.17 |   61.0 |        52.3 | 55.0% |           37.8% |  0.7% |            12.3% |       62.1% |
| 258 min-links CCD96 | 150.72 | 1645.8 |       981.1 | 59.2% |           34.6% |  0.6% |            12.0% |       64.0% |
| 36 opt 32w          |  12.19 |  100.4 |        80.9 | 52.8% |           38.9% |  1.1% |            10.6% |       61.4% |
| 36 all CCD96 cap    |  90.12 |  945.8 |       753.0 | 56.8% |           35.0% |  0.8% |            11.5% |       61.5% |
| 10 opt CCD32 cap    |  90.81 | 1366.9 |       661.3 | 38.8% |           54.6% |  0.7% |            11.9% |       63.6% |

Open-port plus marked-link graph time remains <0.05s except 36 optimal (0.16s) and capped 36 (0.23s). Labeling is 23–28% of combined state+SCC canon, not the leader. Equality is 80–81% RREF except 36 optimal (63.1%).

Versus experiment 53: Bareiss-forward’s share collapsed. Forward / accounted is 4.6% on 115 CCD96 (was ~15.5%) and 6.7% on 258 (was ~21.4%). State+SCC canon rose to 30–39% of accounted (was 26–38%). Do not treat these hotspot-on walls as a 54 speedup claim.

## Sparse mix after experiment 54

Preparation leftover besides substitution remains tiny. Substitution is almost all of the preparation timer. Forward is no longer the 115/258 leader inside propagation.

| Cell      | Prop s | Analyze / prop | Prep / prop | Subst / prop | Forward / prop | Back / prop | Bounds / prop |
| --------- | -----: | -------------: | ----------: | -----------: | -------------: | ----------: | ------------: |
| 115 CCD96 |  215.9 |          38.1% |       18.5% |        16.8% |           7.4% |        8.6% |         22.4% |
| 238 CCD32 |   26.4 |          27.4% |       20.7% |        19.2% |           1.8% |        2.5% |         26.9% |
| 258 CCD96 |  581.1 |          42.8% |       16.6% |        14.9% |          11.3% |       11.1% |         19.7% |
| 36 opt    |   42.7 |          22.6% |       19.0% |        17.7% |           0.4% |        0.9% |         27.3% |

Prep / accounted is 9.8–11.6% on completed cells. Unbucketed prep (prep minus subst) is 0.7–1.1% of accounted and 1.4–1.7% of prop, matching experiment 53’s “no longer material.” Full prep’s higher share of prop is a smaller forward denominator, not a new prep hotspot.

Sparse calls and shape (active rows after substitution) are unchanged from experiment 53:

- 115: 4,057,892 calls, avg in 26.67 → act 5.39. Rows 0–15: 90.9% calls / 74.9% analyze time. Rows 16–31: 9.1% / 24.2% (was 33.6% of analyze).
- 258: 8,187,802 calls, avg in 32.40 → act 8.03. Rows 0–15: 77.7% / 51.1%. Rows 16–31: 22.3% / 48.3% (was 61.0%).
- 238: 484,676 calls, avg act 2.75. Rows 0–15: 99.8% / 98%.
- 36 opt: 660,458 calls, avg act 1.47. Rows 0–15: 100%.

Reanalysis remains ~32% value-deduction, <1% ratio. RREF operand pattern is unchanged: ~78–80% zero destinations, max bits 11–16 / 6–7. Nested, not a wall share.

## Activity / tails

Traces are wall spans. Peak overlap is concurrent roots, not CPU. Same shape as experiment 53.

- **258:** 64 roots, peak 16, mean_active 10.8–11.4. Overlapping long roots, not one unique leftover.
- **238:** 38 roots, peak 16, mean_active 8.5–8.6.
- **115:** 812 roots, peak 16, mean_active 10.7–10.8. Busy concurrent search, not a serial tail.
- **36 optimal:** 1833 short roots. `acyclic_construct` union 6.25–7.92s of 12.19s wall; root union 4.60–4.62s. CPU util 0.24–0.27. Constructor, not a fat DFS tail. Skip-after-winning-N is already permanent.
- **Stress 10:** 17 roots, peak 16, mean_active 16.0, util 0.94, peak WS 2.60 GiB. Saturated overlapping, incomplete.
- **Stress 36 all:** 2172 roots, plus 8.22s constructor. Cap as planned.

Cache-drop tails after cancel: 0.45s (10) and 0.63s (36-all). Completed 258 drop union ~1.11s vs 151s wall. Donated_tasks and shared_cache_hits remain 0. Deferred visits still dwarf promotions (115: 1.63M visits / 0.26M promotions).

## Decision and next step

Diagnostic complete. No source change. **Park prep-template reuse.** Unbucketed preparation is ~1% of accounted. Full preparation including substitution is ~10–12% of accounted and does not own wall. Original apply-all list in `mem:solver/experiments/49-wall-time-priorities` is now exhausted (items 1/3/4/5 done or rejected; item 2 parked on this profile).

If continuing, the remaining large nested shares are state+SCC canonicalization (30–39% of accounted) and, inside propagation, bounds (20–27% of prop) then substitution (15–19% of prop). Forward Bareiss is no longer the 115/258 leader. Keep parked: sharing, donation, checked64, fraction-free RREF, weighted quotients, duplicate-row removal, rollback-aware Bareiss, early reachability, and templates. Do not enable `work_stealing`. No new experiment started.

Suggested isolated follow-ups (not authorized): 2026-09-04. Experiment57 nested split: state keys 23–27% of accounted (graph/canonaut ≈ the whole state timer); SCC keys 7–11% except 36 (1.5%). Equality 13–17% acc, of which RREF is ~80%; labeling 7–11% acc; semantic encoding ~1% after 49. Known Rationals are in `BaseColor` (`ProducerPort.known`, `ConsumerPort.known`, `Link.flow`), so canonaut reruns when values change. Bounds remaining work is `check_exact_bounds_inner`: full `registered_ports` scan after every successful fixed-point (exp31 already removed the duplicate inequality path). Ranked candidates: (1) dirty-set or representative-only bound scan, same predicates; (2) strip numeric knowns from `BaseColor` and reuse labeling when topology/open/known-support are unchanged, with a 49-style key bijection; (3) only then SCC deferral. Do not retry RREF kernels, inequality payload, or ExactInequality storage.
