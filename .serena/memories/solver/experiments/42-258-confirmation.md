# 42. Focused 258 confirmation and diagnostics

Date: 2026-09-01. State: analyzed; no promotion.
Results: `mem:solver/experiments/42-258-confirmation-results`.
Prior results: `mem:solver/experiments/42-complete-propagation-variable-set-results`.
Source/build provenance: `mem:solver/experiments/42-complete-propagation-variable-set`.

## Question and authorization

The user approved the focused follow-up after the 36-job factorial analysis.
Repeat the adverse complete-variable 258 observation and separate root-search time
from calculation and teardown costs. Do not reopen scheduler experiments or alter
solver policy. Both source changes remain candidates.

## Comparison

Manifest: `benchmarks/custom/registered-variable-258-confirmation.json`.
Exact case: 258 = 195+63; max link rate 1200; minimum_links.
All jobs use p1/32 workers, N<=12, a 600-second solver cap and 60-second cleanup
watchdog. N is an upper search bound, not a supplied optimum. Seed 270826.

- Eight timing jobs: two per variant, repeats 2 and 3, hotspots off.
  Case label `medium258_minimum_links` matches the previous repeat 1.
- Two diagnostics: reference and complete variables, repeat 1, hotspots on.
  Case label `medium258_diagnostic` keeps these out of the ordinary timing groups.

All ten jobs run sequentially in recorded randomized order. Diagnostic-only aliases
do not change the input or policy. The diagnostics record root/group activity,
root finish and cache-drop spans, plus aggregate calculation subphases. They can
identify long roots and teardown; assigning a particular calculation to one root
may still require a later fixed-root replay.

Expected completed result: the same two exact layouts at N=9/L=14, full preferred
witness and proof. A cap is incomplete, never completion-speed evidence.
Combine only hotspot-off samples with the previous run to obtain n=3 per variant.
Do not discard an adverse sample or mix diagnostic timing into the medians.

## Reused binaries

No rebuild and no solver-source changes. Map:
`target/parallelism-ladder/registered-variable-factorial-variants-20260901-a/variants.json`.

| Variant   | Source                                    | profile_case.exe SHA-256                                         |
| --------- | ----------------------------------------- | ---------------------------------------------------------------- |
| before    | 90df7e2                                   | 8b2dd2768976469ba74617cf5e0d31e72324cf2b41e831f8473777dc837d2ea2 |
| integer   | cd46fae                                   | 208a62f389289e54cd50e501dbaf8e2bf12e5cf37d1858da3c3ff5f16dc682b8 |
| variables | 90df7e2 + sparse/propagation from 520b352 | b19b601f29e8f9bed6b3427a89152c376466ab68408e0b201d87ad384f594acf |
| combined  | 520b352                                   | a9d7959021059a75fa07626c465dd95bed1730713812d303877719fdcc3e638c |

The source-map binaries match those copied into the completed factorial run.
All 280 source-file hashes match their recorded metadata. Compiler/configuration
and source overlays are unchanged. The launcher freezes new copies of all four
binaries, source trees, metadata, scripts, case and manifest.

## Validation

Ten-job plan passed:
`target/parallelism-ladder/registered-variable-258-confirmation-plan-20260901`.
All 34 analyzer/runner tests passed in 11.890s; log:
`target/parallelism-ladder/registered-variable-258-tool-tests-20260901.log`.

Two separate two-second diagnostic smoke runs passed startup and cancellation.
Both captured nonzero sparse-analysis/variable-collection counters, closed root,
finish and cache-drop spans, zero dropped records and no active spans on return.
Reference captured 314 activity records, variables 312. These incomplete smoke
runs are instrumentation checks, not exact-work or performance comparisons.
Evidence: `target/parallelism-ladder/registered-variable-258-{before,variables}-diagnostic-smoke-20260901.{json,log}`.

No Rust source changed, so no rebuild or full Rust suite repeated. Previous frozen
binary exactness validation remains applicable.

## Launch and decision gate

Output: `target/parallelism-ladder/registered-variable-258-confirmation-20260901`.
Completion/failure dialog plus authoritative BENCHMARK status files, then end turn.
Allow roughly 35-45 minutes based on previous timings; this is not a guarantee.

After completion, verify all ten scheduled records and the original/new identities.
Report n=3 wall/first-emission/process-CPU medians and ranges, preserving all samples.
Compare complete variables against reference and against integer; compare integer
against reference and against variables. Then inspect diagnostic root and teardown
timelines. Memory remains outside the promotion gate.

Promote only if repeated completed work supports a useful gain without a consistent
completion regression. If 258 repeats confirm a slowdown, isolate its long root
before choosing retention or restoration. Scheduler defaults remain unchanged.

Manifest, prior results and preparation memories committed as `4933048`.
Earlier candidate commits remain cd46fae/520b352; no promotion or push.
Formatting and explicit ten-job/alias/repeat checks passed before commit.
Source manifest SHA-256:
`347b29ff01c50518d9f468957f64ea497dc1a93272dd0fb8abeb86277583ac6f`.

## Actual launch

Started 2026-09-01 23:18:58 Europe/Paris, runner PID 63360. The frozen launcher
validated all ten jobs. Initial check: job 1/10, before hotspot-off repeat 2 running;
runner stderr empty. This is only a launch check, not a benchmark outcome.

Completion/failure dialog enabled. Authoritative run-root files:
`BENCHMARK-STATUS.txt`, `BENCHMARK-FINISHED.txt` or `BENCHMARK-FAILED.txt`.
`results/BENCHMARK-STATUS.txt` identifies the current job.
No builds or tests run during the screen. End turn and wait for user completion.
These launch-state memory changes follow commit 4933048 and remain uncommitted.

## Outcome

All ten jobs complete and verify. Three-sample whole medians do not support either
candidate; diagnostics identify roots 7/23, not cleanup, as the long tail. Variable
collection itself is 82.30% cheaper. The user authorized extended root/affinity
isolation in experiment 43 while AFK. See the results memory for evidence.
