# 55. Completed-state sharing that keeps deferred owners

Date: 2026-09-04. State: analyzed; rejected. Solver source fully restored to experiment 54 `d6902a4`. Related: `mem:solver/experiments/54-bareiss-forward`, `mem:solver/experiments/52-tail-scheduling-results`, `mem:solver/experiments/29-deferred-state-canonicalization`.

## Question and hypothesis

Can concurrent roots publish completed canonical states for reuse while the owner still defers the first visit to a cheap invariant bucket?

Hypothesis: overlapping long roots on 238/258 (experiment 53) would reuse completed keys without giving back experiment 29 or repeating experiment 52’s 115 all-mode identity loss. Do not enable `work_stealing`.

## Change and comparison

Parent: experiment 54, `d6902a421b26f309788fc6ed6626555cb24b256a`, unpushed.

Screen candidate (now restored from production p1):

- Defer predicate is `state_cache == Deferred && donations.is_none()`. Sharing no longer disables deferred first visits. Donation helpers still use exact keys.
- Only completed keys are published; lookup never waits.
- `join()` merges helper `witnesses` maps, then `best`. Public `work_stealing` stayed off.
- Screen `p1` after enabled partitions plus sharing. Frozen before p1 was partitions only.

After analysis, and after the user rejected the wall/memory tradeoff, all experiment 55 solver source (`search.rs`, `search/donation.rs`, `solver.rs`, `lib.rs`, `profile_support`) was restored to HEAD `d6902a4`. Production p1 is partitions only. Sharing still disables deferred first visits (`shared.is_none()`). Donation `join` still keeps only `best`, not helper `witnesses` maps. Frozen 55 binaries/source remain under `target/parallelism-ladder/completed-sharing-screen-20260904` and `completed-sharing-variants-20260904`.

## Screen

Official run `target/parallelism-ladder/completed-sharing-screen-20260904`. Finished 2026-09-04T18:42:50+02:00. Runner PID 56660. Manifest `benchmarks/custom/completed-sharing-screen.json`. 66 hotspot-off jobs, 15s grace, `MaxScheduledSeconds` 7200. `failures=[]`. Verified 66/66. 33 pairs, 31 completed, 2 intended stress caps.

Before SHA256 `c38890fbc9ad6ee08ae60b71a1c1b1228ab0cc3456e0c0bee7acf7b2419247f2`. After SHA256 `9f854476c52069e3186540dba491e43cb594470ec43e2bba10f273232bc2be9b`. Frozen bins match `results/binaries.json`. Summary SHA256 `cb06494a99b7426231db40f1506044d4179101d4fe870817e920290d46bc7af7`. Affinity applied before resume on 66/66; pinned cells requested=observed; 36 unrestricted observed `ffffffff` as planned. Hotspots off. Donated tasks 0 on every group.

## Results

Paired median wall (candidate/reference), exploratory bootstrap 95%:

| Cell | Mode      | Place     | Pairs | Wall median | Wall 95%       | CPU median | Notes                                                     |
| ---- | --------- | --------- | ----: | ----------: | -------------- | ---------: | --------------------------------------------------------- |
| 115  | all       | CCD96 16w |     5 |      −1.49% | −3.20 to +0.38 |     −1.44% | 49 layouts, N=7 L=10; wall CI includes 0; CPU does not    |
| 115  | all       | CCD32 16w |     4 |      −0.99% | −2.57 to +0.24 |     −1.16% | same identity; wall CI includes 0; CPU does not           |
| 258  | min-links | CCD96 16w |     5 |      −4.27% | −6.06 to −3.66 |     −4.84% | 2 layouts; 5/5 faster; only cell whose wall CI excludes 0 |
| 238  | optimal   | CCD96 16w |     4 |      −0.88% | −3.37 to +3.18 |     +0.55% | 1 layout, N=8 L=13; 0 shared hits                         |
| 238  | optimal   | CCD32 16w |     5 |      +0.57% | −0.50 to +1.93 |     +0.92% | 0 shared hits; 2/5 wall pairs faster                      |
| 36   | optimal   | 32w none  |     5 |      +0.21% | −1.44 to +6.56 |     +0.99% | 0 shared hits; one pair +6.56%; interval includes 0       |
| tiny | all       | CCD96 16w |     3 |      +4.09% | +2.01 to +4.17 |        n/a | ~9 ms; sampling floor                                     |

115 first-valid medians +0.22% CCD96 and +0.03% CCD32 (CIs include 0). 258 first-valid tracks wall (−4.27%).

Shared completed hits (median after / before): 115 CCD96 4413 / 0; 115 CCD32 4414.5 / 0; 258 24360 / 0; 238/36/tiny 0 / 0. After still defers: 115 CCD96 deferred visits 1,622,870 vs 1,629,186. 115/258 do less structural work; 238/36 structural counters match except extra shared-cache bytes (36 MB / 25 MB) with zero hits.

Sampled peak WS medians, completed cells: 115 +33% / +36%; 238 +34% / +28%; 258 1,609,785,344 → 2,067,386,368 (+28%); 36 +45%. Shared cache bytes median 74 MB on 115, 686 MB on 258.

Stress (no completion-speedup): 36-all CCD96 and 10-opt CCD32 both `Incomplete(Cancelled)`, deadline fired, layouts 8 and 0 both sides. Shared hits 0. 10-opt after processed slightly more decisions (2,952,708→2,983,546); 36-all slightly fewer.

## Correctness and limitations

Analyzer: completed results match available references; capped stress remain explicitly incomplete. All 31 completed pairs have identical layout keys, preferred witness, status/N/L, and proof objects. 115 all-mode is 49 layouts both sides (experiment 52 was 49→43/44). Tiny structural counters match. Nested timers were off.

Zero shared hits on 238/36 means the overlapping-root hypothesis did not produce completed-key reuse there. Memory is paid even without hits. Wall uncertainty on 115 includes 0 despite a real CPU/work reduction.

## Decision and next step

Do not enable sharing as production p1. The identity gate passed on the measured candidate, but completion speedup is concentrated on 258 while every completed cell pays 28–45% peak WS. Solver source is fully restored to `d6902a4`; do not re-enable sharing, defer-with-sharing, or join witness merge.

Do not enable `work_stealing`. Do not restore experiment 52 tail-help. Do not combine with more Bareiss. Candidate preserved as `benchmarks/custom/variants/completed-sharing.patch` (`git apply --check` against `d6902a4`), manifest `benchmarks/custom/completed-sharing-screen.json`, and generator `scripts/generate-completed-sharing-screen.py`. Experiment 56 fraction-free RREF is also rejected. Reachability early and live DFS donation remain rejected. Recorded in `506bce8`. No push.
