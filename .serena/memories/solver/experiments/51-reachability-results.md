# 51. Reachability results

Date: 2026-09-04. State: permanent in `7799c110e8266b96cb59dd0480b62e59c576cda7`, unpushed. Related: `mem:solver/experiments/51-reachability-plan`.
Raw: `target/parallelism-ladder/reachability-screen-20260903`. Official `results/summary.json` failures=[]. Finished 2026-09-04T03:48:28+02:00. Runner PID 34676.

## Question

Three isolated cumulative changes vs retained49/instrumented before: (1) skip reachability while remaining_profile nonempty, (2) borrowed TopologyState dead-verdict path, (3) run that path after propagation and before dynamic SCC. Hypothesis: skip removes proven no-op work; verdict cuts allocation; early only helps if extra prunes avoid SCC. Exact public results and proofs must match. 12 stress records may cap; no completion speedup from them.

## Comparison

312 jobs / 156 pairs. CancellationGraceSeconds 15. Frozen `profile_case` SHA256, all rev 482213a, 71 files: before `8e5d198b6ee6f8371d0878d0e9c322fd2ba9ebd03278b03e99716f7a5a906230`, skip `547599d023cdb17b42f44cb249588b7dbeb3ca6e9e5d32bb0cd68fe4a6d1ba3d`, verdict `cc302dd6fa1dbfb556d9a3dca930110817ab121eefb2703d3f7954eeaca06de3`, early `999fdaa8e6612cceea7c9422fa82aba1f1972f237487291a72c14091dab5c945`. Run `variant-bin` hashes match. Affinity before resume true on all 312. Watchdog empty. 288 OFF, 24 ON. Completions: 300 optimal, 12 Incomplete(Cancelled) all cohort=stress (diag10 optimal and diag36 all, both sides of each comparison). Analyzer verified exact problem/status/layout_keys/preferred_key/solutions vs completed references.

## Measured OFF wall (candidate/reference median, exploratory bootstrap 95%)

Skip before→skip. 115/all/16/ccd96 (8): −0.12% [−1.14,+0.02], CPU −0.42% [−0.58,−0.26], 6/8 wall. 115/all/16/ccd32 (4): −0.49% [−1.07,−0.08], CPU −0.31%, 4/4. 238/optimal/16/ccd32 (8): −0.24% [−0.95,+0.34], CPU −0.49%, 6/8. 238/optimal/16/ccd96 (4): +0.08% [−1.07,+0.41], CPU −0.87%, 1/4 wall / 4/4 CPU. 258/minimum_links/16/ccd96 (8): −0.30% [−0.60,−0.03], CPU −0.26%, 7/8. 36/optimal/32/unrestricted (8): −0.56% [−0.84,+0.25], CPU −1.19%, 6/8. Tiny/huge sub-10ms, ignore.

Verdict skip→verdict. 115 ccd96 (8): −3.08% [−3.19,−2.31], CPU −3.31% [−3.48,−3.07], 8/8 wall and CPU. 115 ccd32 (4): −2.54% [−3.31,−1.92], CPU −3.41%, 4/4. 238 ccd32 (8): −3.62% [−4.79,−2.46], CPU −3.30%, 8/8. 238 ccd96 (4): −3.00% [−3.73,−2.34], CPU −3.39%, 4/4. 258 (8): −1.52% [−1.79,−1.25], CPU −1.89%, 8/8. 36 (8): −0.41% [−1.56,+0.04], CPU −2.85% [−4.11,−1.10], 6/8 wall / 8/8 CPU. Absolute 115 ccd96 43.224s→41.884s; 238 ccd32 6.619s→6.365s; 258 176.846s→174.194s.

Early verdict→early. 115 ccd96 (8): −0.25% [−0.37,+0.49], CPU +0.23%, 5/8 wall / 0/8 CPU. 238 ccd96 (4): +0.96% [+0.13,+2.31], 0/4 wall. No extra prunes. Rejected.

## Production apply 2026-09-04

User authorized recommendations. Applied skip+borrowed dead verdict onto 482213a working tree. Did not apply early. Did not copy experiment-only `record_reachability_check` counters. Search skips analysis while `remaining_profile` is nonempty, then calls `is_proven_unreachable`. Public `analyze_reachability` / `PartialTopology` API unchanged. Added the two borrowed-verdict tests from the measured candidate.

Validation on this checkout, release, `.cargo/config.toml`, VS2019 amd64: solver-core `--all-targets --features bench-internals` 226 passed / 2 ignored; default `--all-targets` 216 passed / 2 ignored; both strict Clippy `-D warnings` configs pass. `cargo fmt --all` applied. User authorized making this permanent. Experiment52 tail-help was rejected and restored first, so this commit is skip+borrowed-verdict only. No push. Next is a hotspot-on cost-mix and root-tail profile on this tree, not another private-cache donation trial.
