# 52. Tail-scheduling results

Date: 2026-09-04. State: analyzed, rejected, source restored to experiment51. Related: `mem:solver/experiments/52-tail-scheduling-plan`.

## Question

Can production p1 donate DFS siblings after the root claim queue is empty, with private deferred caches and no `shared_state_cache`, cut wall?

## Screen

Official run `target/parallelism-ladder/tail-help-screen-20260904`. All 200 jobs attempted. Finished 2026-09-04T12:07:46+02:00. Marker `BENCHMARK-FAILED.txt`: verification did not pass. Analyzer `--allow-incomplete` still reports identity failures; those are not stress caps.

Frozen binaries match launch hashes: before `b3ff7c287a9bf6e823f03c716cd67cf27692c95b93396ce8fa3349f050e3350e`, after `fb2ef6a899eb979127d06d5e9dd030dca63af1d88e2a8a52f8d01bab44c3bc06`. CSV 200 rows, 100/100 variants, 196 Optimal and 4 intended stress Incomplete. Affinity observed equals requested on every pinned job; `affinity_before_resume` true on 200/200. Unrestricted 36 jobs observed mask `ffffffff`.

## Correctness

Hard reject. Every completed 115 all-mode after job lost layouts versus its before reference: 49 → 43, except one CCD96 sample at 44. Preferred key still matched; N=7/L=10 status still matched. Saved solution objects therefore differ. Failures: all 16 CCD96 pairs, all 8 CCD32 pairs, and the hotspot-on 115 diagnostic (25 unique after files; 50 analyzer failure strings counting both key-set and solutions checks).

tiny/huge all-mode, 238/36 optimal, and 258 minimum_links kept layout counts and preferred keys (2/1/1/2). Stress 36-all at the cap found 7 after vs 8 before; incomplete pairs have no completion-speedup claim and are not a second identity reference.

Cause in the candidate, not a runner artifact: `DonationPool` `Outcome` stores only `best_witness`. `join()` calls `retain_witness` on that one graph and never merges the helper's `witnesses` map. Optimal mode can still agree; all-mode drops layouts found only on donated branches. Public p123 was already on that join path, but production p1 did not donate until this experiment.

## Timing (hotspot-off, completed pairs; candidate over reference)

These medians are from the official paired summary. They are not a promotion case. 115 all-mode timings describe an incomplete enumerator.

| Cell                        | Pairs |       Wall median | Wall bootstrap 95%            | CPU median |
| --------------------------- | ----: | ----------------: | ----------------------------- | ---------: |
| 115 all 16w CCD96           |    16 |           +24.09% | +23.75% to +24.98%            |    +21.28% |
| 115 all 16w CCD32           |     8 |           +23.28% | +22.77% to +24.17%            |    +19.15% |
| 238 optimal 16w CCD32       |    16 |          +138.03% | +133.22% to +142.12%          |    +93.14% |
| 238 optimal 16w CCD96       |     8 |          +142.77% | +126.38% to +144.85%          |    +95.18% |
| 258 min-links 16w CCD96     |    16 |            −0.37% | −0.81% to +0.23% (includes 0) |     +8.27% |
| 36 optimal 32w unrestricted |    16 |           +64.63% | +63.94% to +65.61%            |    +26.13% |
| tiny all CCD96              |     8 | +544% (~9ms→56ms) | overhead                      |        n/a |
| huge all CCD96              |     8 | +390% (~9ms→45ms) | overhead                      |        n/a |

Identity-passing optimal cells still regress: 238 about 2.4× wall, 36 about 1.65×. 258 wall is flat while CPU rises 8.27% on 16/16 CPU pairs. Sample r1 donated_tasks: before 0, after 787909 (115), 470161 (238), 1680092 (258), 352464 (36). Structural decisions and deferred visits rise with donations. Duplicate private-cache DFS, not idle-tail filling, owns the extra work.

Hotspot-on jobs stay diagnostic. Stress 10/36 capped as planned. Do not compare ON walls with OFF medians.

## Decision

Reject. Restore production source to the frozen before tree (experiment51 skip+borrowed-verdict). Do not keep tail-only help. Do not treat helper-witness merge as a retry of this candidate: even modes that kept exact keys got slower.

Working tree restored 2026-09-04 from `target/parallelism-ladder/tail-help-variants-20260904/before/solver-source` into `crates/solver-core/src/{solver.rs,search.rs,search/donation.rs}`. Experiment51 remains applied, uncommitted. Experiment52 code is absent from production source. No commit, no push.

Next leftover from `mem:solver/experiments/49-wall-time-priorities` is not this scheduler. Sharing-plus-donation still forces exact keys; a later scheduler trial needs completed-state sharing that preserves deferred owners, or a different mechanism. Arithmetic stays parked after experiment50.
