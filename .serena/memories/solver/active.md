# solver/active

Experiment 43. Validated; preparing launch on 2026-09-02.
Record: `mem:solver/experiments/43-root258-affinity`.
Prior results: `mem:solver/experiments/42-258-confirmation-results`.

## Question and plan

User authorized extra tests while AFK. Isolate calculation gains from possible
CPU-placement sensitivity on 258's two longest roots. No solver/scheduler change.
All four candidates use new frozen profile_obligation binaries built from the
previous exact source variants; their hashes and original 76-key plans match.
Keep p1/32 planning, target 128, N9L14, S2=5/S3=1/M2=0/M3=3, rate1200.
Each selected root executes serially; exhaustion is local, never a whole proof.

Manifest: `benchmarks/custom/root258-affinity-screening.json`.
28 jobs: root 23 four variants x CPUs 0/31 x two repeats; root 7 one repeat per
variant/CPU; four separate before/variables root 23 diagnostics.
Timing caps 300s, diagnostics 360s; sum 144min plus at most 28min cleanup allowances.
All jobs must exhaust. Never compare hotspot-on/off or pool the two CPU masks.

## Validation and launch

37 runner/analyzer tests pass, including native Windows affinity verification.
Four release builds, frozen plan, four 2s real affinity/cancel smokes and four tiny
completed exact-result controls pass. CPU mask application occurs after launch;
smoke delays 8.4-28.5ms are recorded, so initial steps may be unpinned.
No claim that CPUs 0/31 map to particular cache types.

Frozen variant root: `target/parallelism-ladder/root258-variants-20260902`.
Planned run: `target/parallelism-ladder/root258-affinity-20260902`.
Launch with completion/failure dialog. Watch root `BENCHMARK-STATUS.txt` and
`BENCHMARK-FINISHED.txt` / `BENCHMARK-FAILED.txt`; the root status names the current job.
End turn after startup check. No builds/tests during timing.
Do not infer results from a running benchmark.

## After results

Verify schedule, frozen identities, root certificates, masks/application delays,
full result objects, local proof fields, and structural work. Report within-CPU
medians/ranges; root 7 has only one sample per variant/CPU.
Analyze diagnostic phase changes independently of ordinary timings.
Both candidates remain unpromoted. If controlled work regresses, prefer restoring
the offending candidate; local work saved alone does not justify slower completion.
Memory cost is not a gate. Scheduler extras stay paused.

Tooling/manifest/results memories pending commit; no promotion or push.
