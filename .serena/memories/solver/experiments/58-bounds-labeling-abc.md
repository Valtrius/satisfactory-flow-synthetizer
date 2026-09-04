# 58. Isolated dirty bounds and color/labeling A/B/C

Date: 2026-09-04. State: analyzed; dirty bounds permanent this commit, unpushed. Related: `mem:solver/experiments/57-post54-cost-profile`, `mem:solver/experiments/54-bareiss-forward`.

## Question and hypothesis

After experiment57, remaining accounted wall is mostly propagation bounds and state+SCC canonicalization. Do either isolated candidate improve completion vs production 54 without combining them?

Hypothesis A (`dirty_bounds`): scanning only dirty weighted-UF components after a successful fixed-point is cheaper than visiting every registered port. Same three predicates. Do not restore `ExactInequality`.

Hypothesis B (`color_labeling`): strip numeric knowns/`Link.flow` from `BaseColor`, cache exact canonaut labeling. Values stay in encoding. 49-style bijection. Public witness protocol unchanged. Do not retry RREF kernels.

## Change and comparison

Parent: experiment 54, `d6902a421b26f309788fc6ed6626555cb24b256a`, unpushed. Candidates isolated; production working tree stayed on 54.

- `bounds`: `weighted.rs` + `propagation.rs`. Patch `benchmarks/custom/variants/dirty-bounds.patch`.
- `labeling`: `canonical.rs` + `canonical/color_labeling_tests.rs`. Patch `benchmarks/custom/variants/color-labeling.patch`.

Screen `target/parallelism-ladder/bounds-labeling-abc-screen-20260904`. Runner PID 24636. Finished 2026-09-04T23:05:52+02:00. Manifest `benchmarks/custom/bounds-labeling-abc-screen.json`. 72 hotspot-off jobs, 15s grace, `MaxScheduledSeconds` 7200. Two comparisons: `dirty_bounds` (before vs bounds), `color_labeling` (before vs labeling). `failures=[]`. Verified 72/72. 36 pairs, 32 completed, 4 intended stress caps.

Frozen `profile_case` SHA256 match `results/binaries.json`: before `09613FEEDAF451D0D4FE34151E01DC1FC0B426AE24F5360B8D21A665EC189474`, bounds `3D56C1E133EAF4DAFB1884FE93BB5E3A40D9666553CCF05B7AEE4AD18CF5B442`, labeling `75B955E38F71542818DAB48CB3BBE465B0B894F6CB227D2A75FC3446FB30CAD9`. Summary SHA256 `B933900BD3094B43929A862DC1E7A5F29F7C52DA9FBE50112985978E848316AE`. Affinity before resume true on 72/72; pinned cells requested=observed; 36 unrestricted observed `ffffffff`. Hotspots off (`accounted_timer_s` 0). Donated tasks 0.

## Results

Paired median wall (candidate/reference), exploratory bootstrap 95%. Verification-log group means are not paired medians.

### dirty_bounds

| Cell | Mode      | Place     | Pairs | Wall median | Wall 95%         | CPU median | Notes                                    |
| ---- | --------- | --------- | ----: | ----------: | ---------------- | ---------: | ---------------------------------------- |
| 115  | all       | CCD96 16w |     3 |      −7.29% | −7.96 to −7.18   |     −7.33% | 49 layouts, N=7 L=10; both CIs exclude 0 |
| 115  | all       | CCD32 16w |     2 |      −7.84% | −8.09 to −7.59   |     −7.65% | both CIs exclude 0                       |
| 238  | optimal   | CCD32 16w |     3 |     −10.98% | −12.39 to −7.11  |    −11.83% | 1 layout, N=8 L=13; both CIs exclude 0   |
| 238  | optimal   | CCD96 16w |     2 |     −11.04% | −11.51 to −10.58 |    −11.58% | both CIs exclude 0                       |
| 258  | min-links | CCD96 16w |     2 |      −3.39% | −3.77 to −3.00   |     −4.88% | 2 layouts, N=9 L=14; both CIs exclude 0  |
| 36   | optimal   | 32w none  |     3 |      +0.36% | −3.82 to +2.91   |     −5.82% | wall CI includes 0; CPU CI excludes 0    |
| tiny | all       | CCD96 16w |     1 |      +5.99% | n=1 ~10 ms       |        n/a | sampling floor                           |

115 first-valid −5.92%/−3.99% (CIs exclude 0). Optimal first-valid tracks wall.

### color_labeling

| Cell | Mode      | Place     | Pairs | Wall median | Wall 95%       | CPU median | Notes                                 |
| ---- | --------- | --------- | ----: | ----------: | -------------- | ---------: | ------------------------------------- |
| 115  | all       | CCD96 16w |     3 |      +0.71% | −1.15 to +0.86 |     −0.13% | wall CI includes 0                    |
| 115  | all       | CCD32 16w |     2 |      +0.68% | −0.30 to +1.67 |     +0.10% | wall CI includes 0                    |
| 238  | optimal   | CCD32 16w |     3 |      −0.30% | −2.34 to +0.08 |     −1.37% | wall CI includes 0; CPU CI excludes 0 |
| 238  | optimal   | CCD96 16w |     2 |      +1.67% | +0.18 to +3.17 |     +0.56% | wall regression; both CIs exclude 0   |
| 258  | min-links | CCD96 16w |     2 |      −0.36% | −0.79 to +0.07 |     −0.50% | wall CI includes 0; CPU CI excludes 0 |
| 36   | optimal   | 32w none  |     3 |      −1.10% | −4.11 to −1.00 |     −1.19% | small wall win; both CIs exclude 0    |
| tiny | all       | CCD96 16w |     1 |      +6.33% | n=1 ~10 ms     |        n/a | sampling floor                        |

Peak WS on completed cells is flat to a few percent either way (tiny samples 0). One 238 CCD96 dirty pair sampled +12.8% WS; not a consistent memory movement.

Stress (no completion-speedup): 36-all CCD96 and 10-opt CCD32 both `Incomplete(Cancelled)`, deadline fired, layouts 8 and 0 both sides of both comparisons. Structural counters differ only on these capped runs.

## Correctness and limitations

Analyzer: completed results match available references; capped stress remain explicitly incomplete. All 32 completed pairs have identical status/N/L, layout-key sets, preferred witness, and full solution objects. 115 is 49 layouts both sides. Completed structural counters match except timers (`canonicalization_time_ns`, `partition_planning_ns`) and sampled `peak_memory_bytes`. Public identities unchanged for labeling despite internal color/cache change.

n=2 cells have wide sampling uncertainty in principle; dirty_bounds ratios on those cells are uniformly below 1. Tiny CPU ratios are timer resolution, not evidence.

## Decision and next step

**Promote dirty_bounds.** Permanent in production this commit, unpushed. Wall wins with CIs excluding 0 on every completed 115/238/258 cell; 36 wall flat. Exact keys/proofs/structural counters match. Isolated screened diff remains `benchmarks/custom/variants/dirty-bounds.patch` (do not re-apply).

**Reject color_labeling.** No consistent wall win. 238 CCD96 regresses with CI excluding 0; 115 wall slightly worse with CI including 0; 36’s −1.10% does not outweigh that. Keep `benchmarks/custom/variants/color-labeling.patch`. Do not combine with bounds.

Do not retry RREF kernels. Park templates, sharing, donation, checked64, fraction-free RREF, weighted quotients, duplicate-row removal, rollback-aware Bareiss, early reachability, and labeling reuse. SCC deferral remains the leftover ranked 57 candidate (7–11% of accounted) but is smaller than remaining state-key canonaut (23–27%). Next evidence step is a post-58 hotspot-on cost-mix profile, then SCC deferral only if it still owns a material nested share. No push.
