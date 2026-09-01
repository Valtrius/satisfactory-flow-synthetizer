# 08. Repeated bundle comparison and worker counts

Date: 2026-08-28. State: analyzed; 62 verified records, no watchdog kills.
See results and decisions (`mem:solver/experiments/08-repeat-scheduling-results`). The original protocol
and provenance remain here. Documentation and commit work during the run did not
alter its frozen inputs, scripts or executables.

## Questions

1. Do compact/constructor gains repeat against the preserved recovery binary,
   particularly the possible 24 all/baseline regression?
2. Does p1 help the now-faster hard 36 optimal path?
3. How do 16 and 32 workers compare on hard optimal/all work, memory and cancellation?

## Comparison

Manifest `benchmarks/custom/repeat-scheduling-screening.json`. All max rate 1200,
hotspots off, randomized fresh processes. No hard one-worker or old-binary baselines.

| Jobs | Cases/modes/stages                                | Variants/workers | Repeats |            Cap per job |
| ---: | ------------------------------------------------- | ---------------- | ------: | ---------------------: |
|   48 | 24/65 optimal baseline/p1 and all baseline/groups | before/after, 32 |       3 | optimal 10 s, all 45 s |
|    8 | 36 optimal baseline/p1                            | after, 16/32     |       2 |                  180 s |
|    2 | 36 all groups                                     | after, 16/32     |       1 |                  240 s |
|    4 | 10 optimal baseline/p1                            | after, 16/32     |       1 |                  120 s |

Search caps total 62 minutes plus 60 s cancellation grace per job if needed.
Expected duration from earlier samples was roughly 30-40 minutes, not a guarantee.
The hard jobs are stress observations, not promised complete solves.

## What changed for this run

No solver source or automatic policy change. The release binary and Rust sources
match experiment 07 exactly. Selected manifest `before` jobs now use a preserved
compatible `profile_case.exe`; expensive jobs are not automatically duplicated.
The launcher freezes scripts/manifest/cases before detaching and passes an explicit
repository root. The analyzer additionally checks binary hashes and full saved
solutions, and suppresses speedup ratios across different instrumentation settings.

This is a combined compact/constructor comparison, not isolated per-change
attribution. Both binaries already contain the committed refinement removal.
The optional five-second budget remains provisional. Case names never affect solver policy.

## Provenance and validation before launch

Before: `target/parallelism-ladder/recovery-20260828/bin/profile_case.exe`, source
snapshot in that run's `solver-source/`. SHA256:
`970E43B45D0C0DDB165FF024D52E90AA82701A113E4627C3650016E387777C8A`.

After: copied from `target/release/examples/profile_case.exe`, matching the compact
screen executable and source. SHA256:
`6CB1D7C68A7B5EA0C6FF41F06D20F4409E0215A58405C895A22100370A46E5DD`.

Fresh current validation passed 297 workspace tests, five ignored, 22 runner/
analyzer tests, strict Clippy, formatting and release build. Initial stale shared
target metadata was fixed by a solver-package clean and full revalidation.
The stronger analyzer also passed the previous 16-job screen.

Logs: `target/repeat-scheduling-tests-clean.log`, `repeat-scheduling-clippy-clean.log`,
`repeat-scheduling-build.log` and `repeat-scheduling-format-check.log`, all under target.

## Handoff and decision criteria

Run directory: `target/parallelism-ladder/repeat-scheduling-20260828/`.
Authoritative status: `BENCHMARK-STATUS.txt`; current job under `results/`.
Completion/failure dialog and `BENCHMARK-FINISHED.txt`/`BENCHMARK-FAILED.txt` are enabled.
Additional prepared context: `target/parallelism-ladder/repeat-scheduling-HANDOFF.md`.

The user reported completion. The preserved analyzer reproduced its original
summary exactly, including all 62 identities and binary hashes. Current Rust
sources equal the frozen benchmark snapshot. All eight hard-36 optimal solutions
and both partial enumeration solution objects match the original hard screen.
The six timed-out runs remain incomplete, with no completion speedup inferred.

While this screen ran, the user requested a benchmark/documentation baseline
commit. Commit `92b111c` includes the tools and diagnostic
hooks but excludes compact keys and constructor optimizations. It leaves the
frozen run untouched. Standalone build/test verification of that source split is
deferred until timing jobs finished; earlier validation applies to the full working version.
After completion, that commit passed 194 solver-core library/integration/example
tests, two ignored, strict solver-core Clippy and 22 tooling tests, using its own
target directory. Logs and validation scope are in current status (`mem:solver/status`).
