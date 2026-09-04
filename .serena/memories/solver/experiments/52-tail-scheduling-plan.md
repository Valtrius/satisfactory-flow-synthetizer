# 52. Long-root tail scheduling

Date: 2026-09-04. State: analyzed, rejected. Results: `mem:solver/experiments/52-tail-scheduling-results`.
Parent leftover from `mem:solver/experiments/49-wall-time-priorities` after experiment51 apply.

## Question and hypothesis

After skip+borrowed reachability is in the working tree, can scheduling of long remaining roots cut wall without disabling deferred state canonicalization? Sharing and public `work_stealing` currently force the exact-key path, so a naive p12/p123 toggle is not this experiment.

Hypothesis: once every whole root is claimed, idle workers can help remaining in-flight roots through sibling donation while each thread keeps a private deferred cache. That should fill the experiment20 CPU trough without turning on `shared_state_cache`.

## Change and comparison

Parent revision: `482213af63a7343917c29770cfdba15e99dc9430` plus uncommitted experiment51 skip+borrowed-verdict.

Isolated after change, no public flag:

- `execute_roots` still spawns `min(requested, tasks.len())` workers for production p1.
- Public `work_stealing` still uses `DonationPool::new` and still requires sharing.
- Candidate p1 with more than one worker creates `DonationPool::tail_only`. Workers still claim whole roots first. The first out-of-range claim marks the pool empty. Only then does `donate()` enqueue DFS siblings.
- `help()` / `join()` take `Option<&SharedStateCache>` and leave helper `shared` unset when none is provided. Helper contexts still start with empty private caches.
- Deferred predicate is `state_cache == Deferred && shared.is_none()`. A donation pointer no longer forces exact keys on the owner.

Unchanged: production affinity, CCD policy, remaining-group parallelism, sharing, arithmetic, reachability skip+verdict, public `analyze_reachability`. Duplicate equivalent work across helper threads is accepted. Cross-thread completed-state sharing is a later candidate.

Files: `crates/solver-core/src/solver.rs`, `crates/solver-core/src/search.rs`, `crates/solver-core/src/search/donation.rs`.

Validation before launch, release, `.cargo/config.toml`, VS2019 amd64, target `C:\Users\jakez\.codex\tmp\sf52-after`:

- solver-core `--all-targets --features bench-internals`: 228 passed / 2 ignored (`target/exp52-after-tests-bench.log`).
- solver-core `--all-targets`: 218 passed / 2 ignored (`target/exp52-after-tests-default.log`).
- both strict Clippy `-D warnings` configs pass (`target/exp52-after-clippy-bench.log`, `target/exp52-after-clippy-default.log`).
- `cargo fmt --all` applied.

## Screen

Frozen variants: `target/parallelism-ladder/tail-help-variants-20260904/`.

- before `profile_case` sha256 `b3ff7c287a9bf6e823f03c716cd67cf27692c95b93396ce8fa3349f050e3350e`, target `C:\Users\jakez\.codex\tmp\sf52-before`.
- after sha256 `fb2ef6a899eb979127d06d5e9dd030dca63af1d88e2a8a52f8d01bab44c3bc06`, target `C:\Users\jakez\.codex\tmp\sf52-after`.
  Map: `target/parallelism-ladder/tail-help-variants-20260904/variants.json`.
  Manifest: `benchmarks/custom/tail-help-screen.json` (generator `scripts/generate-tail-help-screen.py`).
  Run: `target/parallelism-ladder/tail-help-screen-20260904`. Runner PID 6964.
  Launcher: `scripts/start-benchmark-screen.ps1` `-CancellationGraceSeconds 15`.
  Status: `target/parallelism-ladder/tail-help-screen-20260904/BENCHMARK-STATUS.txt`.
  Progress: `target/parallelism-ladder/tail-help-screen-20260904/results/BENCHMARK-STATUS.txt`.

200 jobs, one A/B. Production p1 both sides. Hotspot-OFF timing primary; ON diagnostics separate. User authorized up to eight hours. Worst-case search+cleanup 17480s (4.86h) at 15s grace.

Primary OFF, 16 pairs each: 115/all 16w CCD96 cap 75; 238/optimal 16w CCD32 cap 20; 258/minimum_links 16w CCD96 cap 250; 36/optimal 32w unrestricted cap 30.
Cross-cache and controls, 8 pairs each: 115 CCD32 cap 90; 238 CCD96 cap 20; tiny all N<=2 cap 5; mixed-huge all cap 5.
ON, one pair each: 115 all CCD96 cap 80; 238 optimal CCD32 cap 20; 36 all CCD96 cap 90 stress; 10 optimal CCD32 cap 90 stress.
AB/BA balanced in OFF cells; pair order shuffled with seed 52. Masks `ffff` / `ffff0000`; cache 96MiB / 32MiB. No speedup from caps. All non-stress jobs must finish and match full public results, keys, preferred witnesses and proof.

## Results

See `mem:solver/experiments/52-tail-scheduling-results`. Official screen finished with identity failure on every 115 all-mode after job (49→43/44 layouts). Optimal 238/36 completed but slower. Source restored to experiment51.

## Correctness and limitations

Identity tests cover p1-with-workers vs serial expected witnesses (`solver/benchmark.rs`), tail-only donate gating, and deferred visits with donations and no shared cache. Public p123 still requires sharing. Duplicate helper work is accepted. Join-stall if donate ran while roots remained unclaimed is the reason donate is tail-only.

## Decision and next step

Rejected. Restored to experiment51 skip+verdict. Not committed. Do not retry private-cache tail donation; join keeps only the helper best witness and duplicate DFS regresses even when keys match.
