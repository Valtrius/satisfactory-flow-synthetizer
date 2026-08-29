# 16 - Scheduling after cheaper calculations

Date: 2026-08-28. Finished; all 38 jobs verified after correcting reference selection.
[Promoted calculations](14-calculation-promotion.md), [prefix implementation](15-prefix-workloads.md).
[Results and verifier correction](16-post-calculation-results.md),
[diagnostics and next steps](16-diagnostics-and-next-steps.md). Original failure evidence retained.
The sections below preserve the launch protocol. Scheduling defaults remain unchanged.

## Questions

1. Does p1 alone finish hard-36 all sooner than p14 with the new calculations?
2. Which phases and worker activity explain any remaining whole-enumeration tail?
3. Does the observed hard-36 optimal gain repeat against the preserved reference?
4. Which exact hard-10 subtrees provide completed, repeatable measurement work?

## Fixed prefix phase

`benchmarks/custom/prefix-screening.json`, seed 280815, 26 jobs.
Twenty-four hard jobs cover six depth/pick recipes, best/all, two repeats each,
30 s search caps. Two tiny controls have 5 s caps and must exhaust. All use one
worker per selected subtree, capacity 1200 and no hotspot recording.
See [15](15-prefix-workloads.md) for certificate, timing and local-proof semantics.
This is discovery on the promoted calculations, not another before/after speed claim.

## Whole-solver phase

`benchmarks/custom/post-calculation-scheduling.json`, existing seed 270826, 12 jobs.
All use capacity 1200, 32 workers, N<=9 and actual solver APIs.

| Work                              | Comparison                      | Repeats           | Search cap          |
| --------------------------------- | ------------------------------- | ----------------- | ------------------- |
| 36 all, instrumentation off       | Combined p1 versus combined p14 | Two per policy    | 240 s               |
| 36 all, separate diagnostic alias | Combined p1 versus combined p14 | One per policy    | 120 s               |
| 36 optimal, instrumentation off   | Reference p1 versus combined p1 | Three per variant | 60 s, must complete |

Whole all may remain incomplete. Compare first validated witness, full completion,
exact saved results and proof scope separately. Diagnostic spans/counters explain
work; they are not added to timing repeats or interpreted as CPU shares.
The reference is experiment 13's unchanged solver at `1b558a6`, not an old
constructor/deadline variant. Only combined variants enter the p1/p14 comparison.

## Duration and source identity

The two phases run sequentially, 38 jobs total. Search caps sum to 38.167 minutes,
plus prefix preparation/startup and cleanup. Expect roughly 25-40 minutes depending
on which prefixes complete. Every process has 60 s watchdog grace; kills fail verification.

Combined snapshot: `target/parallelism-ladder/prefix-variants-final-20260828/basis/`.
Reference: `target/parallelism-ladder/calculation-variants-20260828/reference/`.
Each snapshot retains source files, release examples, revision/diff and per-file
hashes. New prefix entry points are feature-gated and do not change whole-solver policy.
Frozen run scripts, cases, certificates, schedules and binaries are authoritative.
Combined profile_obligation SHA-256:
`12e533f9360e2be1b97dd3ff2ad2778ac890ddf1aa35076b7a8816030b89e935`.
The snapshot records base `af1682c` plus exact benchmark-source copies/diff; all
source files, including new modules, are hashed. Promotion commits are unchanged.

Final validation: 211 Rust tests passed, two ignored; strict all-target Clippy
passed with and without bench-internals; 30 Python tests passed, including native
prefix rejection. Release examples and `npm run format` passed. The final frozen
38-job plan passed at `target/parallelism-ladder/prefix-final-plan-check-20260828/`.
This plan check does not run timed searches or establish performance.

```powershell
./scripts/start-hard-profile.ps1 -JobManifest benchmarks/custom/prefix-screening.json -RegressionManifest benchmarks/custom/post-calculation-scheduling.json -OutputDirectory target/parallelism-ladder/post-calculation-20260828
```

## Completion and resume

Run root: `target/parallelism-ladder/post-calculation-20260828/`.
`BENCHMARK-STATUS.txt` reports the current phase. During whole solves it points to
`whole-results/BENCHMARK-STATUS.txt`. The runner writes `BENCHMARK-FINISHED.txt`
or `BENCHMARK-FAILED.txt` and displays one completion/failure dialog after both phases.
Do not build or test during timings. End the agent turn after launch.

When the user reports completion, rerun frozen verification into separate recheck
summaries. Preserve originals, verify hashes and all 38 identities, then analyze.
For prefixes, group by full certificate, report both repeats and classify each as
too short, useful completed work or capped. Do not equate different/nested prefixes.
For whole solves, report medians/ranges and separate the diagnostic aliases.

Keep defaults opt-in. Only a fresh trace showing idle workers alongside an expensive
DFS tail would justify another bounded-donation trial. Existing difficult cases suffice.
