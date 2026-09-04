# 56. Fraction-free canonical dense RREF

Date: 2026-09-04. State: analyzed; rejected. Source restored to experiment 54 `d6902a4`. Related: `mem:solver/experiments/49-wall-time-priorities` item 3, `mem:solver/experiments/50-checked-rref-results`, `mem:solver/experiments/54-bareiss-forward`.

## Question and hypothesis

Can canonical dense RREF clear per-row denominators, run integer Gauss–Jordan, then normalize each pivot row in Q, matching the existing dense oracle without experiment 50’s checked64 conversion tax?

Hypothesis: fraction-free integer elimination avoids repeated Rational gcd work. Operand swell is the risk. Sparse Bareiss unchanged. Not combined with checked64.

## Change and comparison

Parent: experiment 54, `d6902a421b26f309788fc6ed6626555cb24b256a`, unpushed. Candidate changed only `crates/solver-core/src/canonical.rs`. Production p1 partitions only.

Screen `target/parallelism-ladder/fraction-free-rref-screen-20260904`. Finished 2026-09-04T20:15:44+02:00. Runner PID 53844. Manifest `benchmarks/custom/fraction-free-rref-screen.json`. 66 hotspot-off jobs, 15s grace, `MaxScheduledSeconds` 7200. `failures=[]`. Verified 66/66. 33 pairs, 31 completed, 2 intended stress caps.

Before SHA256 `1fe8d84be0520f23e2e160a958a7d673d7fc367de5c942ebfdadfba92bddb740`. After SHA256 `313b8f89d6fa40cf23c9de50f4b214d67799c2301cfb177d6580dd7441f49211`. Frozen bins match `results/binaries.json`. Summary SHA256 `fb058f37c944be5ca651bcd542e15973146a4b99765f91ae7f824badfbed22c6`. Affinity applied before resume on 66/66; pinned cells requested=observed; 36 unrestricted observed `ffffffff` as planned. Hotspots off (`accounted_timer_s` 0). Donated tasks 0.

## Results

Paired median wall (candidate/reference), exploratory bootstrap 95%:

| Cell | Mode      | Place     | Pairs | Wall median | Wall 95%        | CPU median | Notes                                      |
| ---- | --------- | --------- | ----: | ----------: | --------------- | ---------: | ------------------------------------------ |
| 115  | all       | CCD96 16w |     5 |      +0.44% | −0.29 to +0.78  |     +0.64% | 49 layouts, N=7 L=10; CPU CI excludes 0    |
| 115  | all       | CCD32 16w |     4 |      −0.55% | −1.43 to +0.62  |     +0.30% | wall CI includes 0                         |
| 258  | min-links | CCD96 16w |     5 |      +1.16% | −3.35 to +7.50  |     +1.06% | 2 layouts, N=9 L=14; wall CI includes 0    |
| 238  | optimal   | CCD96 16w |     4 |      +3.76% | −2.61 to +10.04 |     +2.30% | 1 layout; CPU CI excludes 0; one pair +10% |
| 238  | optimal   | CCD32 16w |     5 |      +0.58% | −0.95 to +1.34  |     +0.85% | wall CI includes 0                         |
| 36   | optimal   | 32w none  |     5 |      +2.98% | +0.24 to +4.46  |     +5.17% | 5/5 wall worse; both CIs exclude 0         |
| tiny | all       | CCD96 16w |     3 |      +5.11% | includes 0      |        n/a | ~9 ms; sampling floor                      |

115 first-valid tracks wall (CIs include 0). 258 first-valid tracks wall. 36 first-valid +2.99%.

Always-on `algebra_time_ns` (sparse, not canonical RREF; not a wall share): 115 +0.07%/+0.15%; 258 +2.26%; 238 −0.15%/+0.69%; 36 +1.20%. Structural counters match on 31/31 completed pairs except this timer. Peak WS flat to +3.8%; 36 −1.7%.

Stress (no completion-speedup): 36-all CCD96 and 10-opt CCD32 both `Incomplete(Cancelled)`, deadline fired, layouts 8 and 0 both sides. After processed slightly fewer decisions (36-all 15,596,863→15,149,135; 10-opt 2,980,481→2,955,242).

## Correctness and limitations

Analyzer: completed results match available references; capped stress remain explicitly incomplete. All 31 completed pairs have identical layout keys, preferred witness, status/N/L, and proof objects. 115 is 49 layouts both sides. Nested timers were off. Verification log group means are not paired medians; use the paired table.

No cell has a wall win whose CI excludes 0. The only CI-excluding wall result is the 36 regression. CPU also worse on 115 CCD96, 238 CCD96, and 36. Integer swell did not beat production Q GJ; 258 did not cap.

## Decision and next step

Rejected. Restored `crates/solver-core/src/canonical.rs` to HEAD `d6902a4`. Candidate preserved as `benchmarks/custom/variants/fraction-free-rref.patch` (`git apply --check` against `d6902a4`), manifest `benchmarks/custom/fraction-free-rref-screen.json`, and generator `scripts/generate-fraction-free-rref-screen.py`. Frozen binaries remain under `target/parallelism-ladder/fraction-free-rref-variants-20260904` and the official screen. Do not combine with checked64. Do not retry fraction-free RREF without a new arithmetic idea.

Original apply-all list in `mem:solver/experiments/49-wall-time-priorities`: items 1, 3, 4, 5 are done or rejected (49, 50+56, 51 early, 52+55). Item 2’s prep-template reuse was conditional on preparation still dominating after a reprofile. Experiment 53 is pre-54; do not implement templates from that profile. Next, if continuing, is a small hotspot-on profile of production 54, then templates only if preparation still owns wall. Park sharing, donation, checked64, fraction-free RREF, weighted quotients, duplicate-row removal, rollback-aware Bareiss, and early reachability. Recorded in `506bce8` `docs(custom): preserve rejected sharing and fraction-free RREF results`. No push.
