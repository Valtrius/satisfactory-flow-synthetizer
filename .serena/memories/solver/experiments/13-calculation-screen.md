# 13 protocol - Calculation comparison

Subsequent promotion is recorded in 14 (`mem:solver/experiments/14-calculation-promotion`). The results
and uncommitted-state descriptions below retain the analysis-time context.

Date: 2026-08-28. Finished and independently verified, 80/80 jobs, no kills.
Implemented candidates and validation (`mem:solver/experiments/13-calculation-changes`).
Isolated results (`mem:solver/experiments/13-calculation-results`) and whole results (`mem:solver/experiments/13-whole-results`)
support separate promotion of all three candidates. This file retains the launch protocol.

## Questions

1. Does earlier exact-L rejection remove the rejected-candidate witness cost?
2. Do cached witness leaves preserve exact keys/permutation coverage and shorten replay?
3. Do direct RREF bounds improve complete proof workloads and reduce the 10 basis cost?
4. Does the combined candidate improve real optimal/all completion without regressions?

## Frozen comparison

Manifest: `benchmarks/custom/calculation-screening.json`, 52 jobs, seed 280813.
Every case uses capacity 1200. Fixed hard jobs use p1/32; tiny controls use p1/4.
All timing jobs have hotspots off. Explicit diagnostics and witness replays have
instrumentation on and remain separate from ordinary timing samples.

| Work                                        | Comparison           | Jobs | Cap per job               |
| ------------------------------------------- | -------------------- | ---- | ------------------------- |
| Tiny N=2/L=1, best/all                      | All four variants    | 8    | 5 s, must exhaust         |
| 36 profile 4,2,3,0, best/all, three repeats | reference vs exact_l | 12   | 30 s, must exhaust        |
| Same profile, all diagnostic                | reference vs exact_l | 2    | 40 s, must exhaust        |
| 36 profile 5,2,0,2, best/all, three repeats | witness vs basis     | 12   | 40 s, must exhaust        |
| Same profile, all diagnostic                | witness vs basis     | 2    | 40 s, must exhaust        |
| 36 exact N=9/L=12 group, best/all           | reference vs basis   | 4    | 110 s, incomplete allowed |
| 10 profile 5,2,0,4, best/all diagnostic     | witness vs basis     | 4    | 30 s, incomplete allowed  |
| Saved 36 witness, three completed replays   | exact_l vs witness   | 6    | 60 s process budget       |
| Saved witness cancellation at 20 ms         | exact_l vs witness   | 2    | 5 s process budget        |

Profile tuples are S2,S3,M2,M3. All 36 profiles use N=9/L=12; 10 uses N=11/L=18.
The replay input is one validated N=9/L=12 witness from experiment 12. The wrapper
records its original selected-profile scope and source hash, never an invented
optimal result. Completed replay uses the existing key assertion and validation;
the verifier also compares leaf/branch counts between completed replays.

`best` remains profile-local exhaustive search retaining one preferred witness.
Local exhaustion does not establish a global optimum or minimum-N enumeration.
Root IDs remain local to their particular frontier; do not compare them across plans.

## Whole-solve regression phase

Manifest: `benchmarks/custom/calculation-whole-regression.json`, 28 jobs, existing
ladder seed 270826. All are fresh uninstrumented processes at 32 workers.
The frozen `reference` is before; the frozen combined `basis` is after.

- 24 and 65, actual optimal/all APIs, baseline scheduling, three repeats each:
  24 jobs, 45 s caps, must complete. N caps are seven and six respectively.
- 36, actual optimal with p1 and all with p14, one sample per binary:
  four jobs, 90/120 s caps, N<=9, incomplete allowed. These are bounded follow-ups,
  not repeatable performance proof by themselves.

Both phases run sequentially, with one completion/failure notification at the end.
Expected duration is roughly 20-30 minutes. Caps sum to 57.8 minutes, plus cleanup;
many controls should finish much sooner. Every process has 60 s watchdog grace.
Completed replay has no cooperative deadline; a budget overrun can be killed and
must fail verification. No hard single-worker baseline or new user case is needed.

## Running and resuming

```powershell
./scripts/start-hard-profile.ps1 -JobManifest benchmarks/custom/calculation-screening.json -RegressionManifest benchmarks/custom/calculation-whole-regression.json -OutputDirectory target/parallelism-ladder/calculation-screen-20260828
```

The manifests require the four frozen variant directories recorded in the change
document. Another checkout must obtain them or rebuild the sequential patches.
Existing output directories are rejected. `-PlanOnly` freezes and validates the
entire plan without solving; the successful check is under `calculation-plan-check-20260828/`.

The launcher freezes variant files, inputs, scripts, manifests and schedules.
Each job is bound to its variant executable. Per-file hashes are checked before
launch and verification. Exact completed solution maps and compatible preferred
witnesses are compared; partial sets are checked against complete references
where available. Kills, false exhaustion and unrequested incomplete replays fail.

Run root: `target/parallelism-ladder/calculation-screen-20260828/`.
Inspect `BENCHMARK-STATUS.txt`; during the second phase it points to
`whole-results/BENCHMARK-STATUS.txt`. Final verification files are `verification.log`,
`summary.json`, `whole-verification.log` and `whole-results/summary.json`.
Keep all original results, process metrics, stderr and any failure records.
End the agent turn after launch and analyze only after the user reports completion.

The two original summaries match independent rechecks. Total measured process wall
time was 28.669 minutes. See the result records for sample ranges, capped scopes,
exact-reference limitations and recommendations. Candidates remain uncommitted.
