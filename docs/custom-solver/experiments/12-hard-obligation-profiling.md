# 12 - Profile fixed hard obligations

Date: 2026-08-28. State: 24 results verified; see [findings](12-hard-obligation-results.md).

## Question and hypothesis

After the constructor and compact-key promotions, what dominates the remaining
hard search: canonical graph work, exact basis generation, witness processing,
partition planning or teardown? Fixed N/L/profile work removes the earlier solve
prefix and optional constructor so these costs can be examined directly.

This is diagnosis, not an optimization comparison. No new scheduling default or
calculation change is promoted. Benchmark names never guide solver policy.

## Baseline and implementation

Constructor commit `099cc12` and compact-key commit `2716fac` each include their
related docs. This protocol, runner and profiling entry are in `1b558a6`.
The experiment 09 manifest and unapplied variant patches are also preserved in
the tooling commit; neither patch is active in the profiling binary.

`bench-internals`, disabled by default, exposes exact accounted profile selection
and calls production `ProfileGroupRun`: normalization, partition/root planning,
search, ledger folding, joining and caller-unit witness validation are reused.
Empty or mismatched selections fail. It bypasses earlier N/L groups and the
constructor without changing the ordinary solver path.

`best` retains the preferred witness per selected profile; `all` retains every
distinct witness. Both exhaust selected work or return explicit incompleteness.
Best is not a whole find-optimal run, and no first-witness latency is measured by
this entry. Exhausted means only the selected profiles, never global optimality
or complete minimum-N enumeration. Missing earlier proofs are not inferred.

## Bounded plan

Manifest: `benchmarks/custom/hard-obligations.json`. Every capacity is 1200.

| Work                                  | Modes/stages                    | Jobs | Search cap each    |
| ------------------------------------- | ------------------------------- | ---- | ------------------ |
| Tiny 2+3=1+4, N=2/L=1                 | best/all, baseline/1 and p1/4   | 4    | 10 s; must exhaust |
| 36=11+9+7+5+3+1, N=9/L=12             | best/all, baseline/32 and p1/32 | 4    | 90 s               |
| 10=6.04+3.96, N=11/L=18               | best/all, baseline/32 and p1/32 | 4    | 90 s               |
| Each of 36's five accounted profiles  | all, p1/32                      | 5    | 30 s               |
| Each of 10's seven accounted profiles | best, p1/32                     | 7    | 30 s               |

Total: 24 fresh processes, randomized with seed 280826, 1120 s (18.7 minutes)
of search caps. Each process has 60 s extra cleanup grace before a watchdog kill;
cleanup and launch overhead can extend total time. No hard single-worker baseline.
Actual profiles come from normalized production accounting, not handpicked labels.

All samples have instrumentation on, one sample per configuration. Group and
single-profile runs allocate workers differently. Do not rank completion speed
from these samples, compare differently scoped counters, or add overlapping timers
as CPU. Use them to choose subsequent repeated, uninstrumented complete workloads.

## Run, completion and evidence

```powershell
cargo build --release -p solver-core --features bench-internals --example profile_obligation
./scripts/start-hard-profile.ps1 -OutputDirectory target/parallelism-ladder/hard-obligations-20260828
```

The output directory must not exist. The launcher freezes the executable/hash,
source, workspace Cargo files, revision/diff/status, scripts, cases, manifest and
expanded schedule. `profile_obligation --list request.json` validates selections
without solving. No builds/tests should overlap the actual diagnostic run.

The hidden runner updates `BENCHMARK-STATUS.txt` and shows a completion/failure
dialog. End the agent turn after launch. On completion, inspect status,
`verification.log`, `summary.json`, `process-metrics.csv` and every job result;
retain `failed-jobs.json` and stderr if present. Raw evidence is under
`target/parallelism-ladder/hard-obligations-20260828/`.

Every result retains exact problem/request/profile identities, local root and
profile exhaustion, validated witnesses, counters, hotspots and phase spans.
Five-second heartbeat logs and 15-second live snapshots survive cancellation
tails. Reporter threads join before final output; dropped trace counts remain
visible. Whole-process CPU, sampled peak memory and watchdog outcomes are saved.

The verifier rejects identity changes, false exhaustion, unexpected incomplete
reasons, kills, changed binaries and witness mismatches. Complete all-profile
references compare full saved solutions, not only counts; partials must be subsets
where such a reference exists. Completed best witnesses must match the preferred
all-mode witness. Without a complete reference, validation does not prove coverage.

Recheck with `python scripts/hard-profile.py verify <run-directory>`.

## Validation before launch

- `cargo test -p solver-core --features bench-internals --lib --tests --examples`:
  208 passed, two ignored. Log: `target/fixed-workload-tests.log`.
- Strict all-target solver-core Clippy with and without the feature passed:
  `target/fixed-workload-clippy.log`, `target/fixed-workload-default-clippy.log`.
- Release build passed: `target/fixed-workload-build.log`.
- 26 Python tooling tests passed: `target/hard-profile-tool-tests.log`. Includes
  actual tiny completion and one-second hard cancellation, not timing evidence.
- PowerShell syntax and 24-job expansion/cap validation passed. Plan-only evidence:
  `target/parallelism-ladder/hard-obligations-plan-check-20260828/`.
- Repository formatting runs before commit. No new full-workspace test claim.

## Findings and next decision

All 24 jobs verified in 14.028 process minutes: 14 local exhaustions, ten capped
incomplete, no kills. No hard whole solve completed. The [results](12-hard-obligation-results.md)
identify a full-witness tail for 36 and distributed exact-search costs for 10.
See [next experiments](12-next-experiments.md); no scheduler default changed.
