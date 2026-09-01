# 42. Complete propagation variable set

Date: 2026-09-01. State: analyzed; candidates retained, not promoted.
Results: `mem:solver/experiments/42-complete-propagation-variable-set-results`.
Related: primitive integer rows (`mem:solver/experiments/41-primitive-integer-inequality-rows`).

## Question and measured basis

Can propagation stop rediscovering sparse-row variables on every exact analysis?
`PropagationState::registered_ports` already stores every production flow variable in
sorted order. `SparseSystem::analyze_over` still copies those keys into a `BTreeSet`,
scans every stored row into the same set, removes known variables, and copies the set
into a vector.

The experiment 40 profile measured this work after the three row-substitution
optimizations:

| Workload    | Sparse calls | Variable collection | Share of sparse time |
| ----------- | -----------: | ------------------: | -------------------: |
| 115 all     |    4,212,718 |             9.489 s |                6.24% |
| 238 all     |      969,352 |             2.600 s |               12.35% |
| Hard 36 cap |    6,770,880 |            20.579 s |               15.32% |
| Hard 10 cap |    4,182,485 |            20.486 s |                8.48% |

Times are aggregate worker seconds from instrumented runs. They establish repeated
cost, not the candidate's completion gain.

## Candidate and soundness

Add a crate-private complete-variable analysis path. It accepts a strictly increasing
variable list that already contains every variable in every stored row. It removes
known variables directly into a vector and skips the `BTreeSet` plus row scan. The
public defensive path remains unchanged for callers that cannot prove completeness.

Production propagation supplies `registered_ports.keys()`. Every production sparse
row comes from registered physical ports: initial balances, known-port constants,
node equations, and physical-link equalities. Debug and test builds verify that the
list is sorted, contains every row variable, and contains every known variable. These
checks compile out of release builds.

The candidate changes neither equations nor elimination. It does not touch canonical
keys, inequality bytes, DFS order, proof accounting, result collection, or scheduling.

## Exactness gate

- Compare complete and defensive analysis objects for profiled and ordinary paths.
- Cover known-variable substitution and explicit free zero columns.
- Extend the exhaustive small integer matrix oracle to compare the whole
  `SparseAnalysis` from both paths.
- Run solver-core, exhaustive Reference, parallelism, workspace, strict Clippy, and
  analyzer suites before benchmarking.
- Require every completed benchmark variant to preserve status, proof, preferred
  witness, full solution objects, canonical layout-key set, validation, and checked
  structural counters.

## Prelaunch validation

- Solver-core all-target tests pass with 184 default and 189 benchmark-feature tests;
  three manual benchmarks are ignored in each configuration.
- The exhaustive outer Reference differential and all four parallelism integrations
  pass in both configurations.
- Full workspace tests pass with six ignored manual benchmarks.
- Strict solver-core all-target Clippy passes with and without `bench-internals`.
- All 34 analyzer and runner tests pass, including named variants and hotspot-off
  process sampling.
- Formatting, the release `profile_case` build, a completed combined tiny smoke, the
  36-job plan, and a four-variant runtime/analyzer smoke pass.

Evidence: `target/parallelism-ladder/registered-variable-combined-smoke.json`,
`target/parallelism-ladder/factorial-plan-check-20260901-a`, and
`target/parallelism-ladder/named-factorial-smoke-20260901`.

Candidate implementation: `520b352`. Benchmark infrastructure: `4de03bc`.
Both are committed locally; this turn has not pushed. The docs were migrated to Serena
by `4a9b28d`/`81116c4`; no docs folder is maintained.

## Factorial screen

Manifest: `benchmarks/custom/registered-variable-factorial.json`.

The four frozen variants separate this candidate from experiment 41:

| Variant     | Integer inequality rows | Complete propagation variables |
| ----------- | ----------------------: | -----------------------------: |
| `before`    |                      no |                             no |
| `integer`   |                     yes |                             no |
| `variables` |                      no |                            yes |
| `combined`  |                     yes |                            yes |

The screen schedules 36 randomized fresh processes, all p1/32 with solver hotspots
off:

- four samples per variant for 115 full minimum-N enumeration, N <= 12, 120-second cap;
- four samples per variant for 238 optimal, N <= 12, 60-second cap;
- one sample per variant for `258 = 195+63` minimum-link enumeration, N <= 12,
  600-second cap.

The 115 and 238 records complete experiment 41's repeat and test both main effects and
their interaction. The user reports about 4m33 for the 258 minimum-link workload. Its
single samples add exact coverage and one CPU-tail trace per variant; they do not by
themselves establish a timing distribution. The runner now records one-second process
CPU, working-set, and thread samples for hotspot-off jobs without enabling solver
instrumentation.

## Decision gate

Make the complete-variable path permanent if all exact checks pass and repeated
completed CPU or wall medians are neutral or faster without a consistent regression.
The removed work is provably redundant, so a small repeatable gain is enough. Judge
the integer-row candidate on the four fresh matched samples first. The three earlier
samples are secondary evidence: the rebuild produces different executable hashes and
this runner adds process-sample recording. Pool to seven only if source, build settings,
request and sampling comparability checks support it.
If both pass, require the combined variant to preserve exact results and avoid a
credible interaction regression. Scheduling remains paused.

Do not infer a result until the detached runner and frozen analyzer finish.

## Frozen-build preparation and failures

Variant root: `target/parallelism-ladder/registered-variable-factorial-variants-20260901-a`.
`before` archives `90df7e2`; `integer` archives `cd46fae`; `variables` archives
`90df7e2` with committed `520b352` sparse.rs/propagation.rs overlaid; `combined`
archives `520b352`. Each has a separate Cargo target and frozen source tree.

The initial long-path bundled Z3 builds failed before producing solver binaries.
Default VS and a VsDevCmd retry reported no usable C++ compiler. A Ninja retry
identified the installed compiler but hit a PDB/path-length error (263-character
CMake object directory). Short-path Ninja then rejected z3-src's MSBuild `-m` option.
The successful recipe uses short targets under
`C:/Users/jakez/.codex/tmp/sfv42-20260901-a/{bvs,ivs,vvs,cvs}`, VS2019's
`VsDevCmd.bat -arch=amd64`, and the default Visual Studio generator. VS2019 is
installed; do not describe it as missing. No source workaround or evidence deletion.

Rust 1.96.0, x86_64-pc-windows-msvc; MSVC 19.29.30159. Builds run from the main
repository so its ignored `.cargo/config.toml` supplies target-cpu=znver4 and
C/C++ `/O2 /Ob2 /Oi /Ot /arch:AVX512`. All four variants use the same settings.
The first named-variant smoke used one combined executable under four names; it
validated the runner only. A final smoke must use all four actual frozen binaries.

## Final prelaunch verification

All four isolated release builds succeeded. SHA-256:

| Variant   | profile_case.exe                                                 |
| --------- | ---------------------------------------------------------------- |
| before    | 8b2dd2768976469ba74617cf5e0d31e72324cf2b41e831f8473777dc837d2ea2 |
| integer   | 208a62f389289e54cd50e501dbaf8e2bf12e5cf37d1858da3c3ff5f16dc682b8 |
| variables | b19b601f29e8f9bed6b3427a89152c376466ab68408e0b201d87ad384f594acf |
| combined  | a9d7959021059a75fa07626c465dd95bed1730713812d303877719fdcc3e638c |

Per-variant metadata includes compiler/configuration, all 70 crate/workspace file
hashes, source revision/overlay, and executable identity. Pairwise hashes prove only
canonical.rs differs for integer rows, and only algebra/sparse.rs + propagation.rs
differ for complete variables. Evidence: variant root's `verified-source-differences.json`,
`binary-identities.json`, and each `metadata.json`.

All 12 actual frozen-binary tiny jobs pass exact analyzer verification across
optimal, minimum_links and all. Evidence:
`target/parallelism-ladder/frozen-factorial-smoke-20260901` and
`target/parallelism-ladder/frozen-factorial-smoke-summary-20260901.json`.
The earlier same-binary smoke is not the evidence for this result.

A separate two-second 258 startup/cancellation smoke reaches N=9/L=14, then returns
Incomplete(Cancelled) with the deadline fired. It confirms the case parses exactly,
starts substantive search under N<=12, and cancels; it proves neither optimum nor
completion speed. Evidence: `target/parallelism-ladder/medium258-start-smoke-20260901.{json,log}`.

Completed and analyzed. Historical launch output:
`target/parallelism-ladder/registered-variable-factorial-20260901`.
36 jobs, seed 270826, per-job cancellation grace 60 seconds. Expect completed
reference-class workloads; a cap is incomplete and cannot support a completion gain.
At the user's observed 258 timing, allow roughly 30–35 minutes; this is only an estimate.

## Launch and handoff

Started 2026-09-01 22:36:47 Europe/Paris (20:36:47 UTC), runner PID 48724.
Launcher validated all 36 jobs and froze binaries, all four source trees/metadata,
manifest, case inputs and scripts under `target/parallelism-ladder/registered-variable-factorial-20260901`.
At the launch check, job 1/36 was running and runner stderr was empty. No result is
inferred from that observation. Completion/failure dialog is enabled; authoritative
files are `BENCHMARK-STATUS.txt`, `BENCHMARK-FINISHED.txt` or `BENCHMARK-FAILED.txt`;
`results/BENCHMARK-STATUS.txt` records the current job.

Serena preparation committed as `f0dca96`. Launch-state updates follow that commit
and are not yet committed; no push this turn. No builds/tests are running alongside
the screen. Scheduler remains unchanged.

A final working-tree/raw-hash assertion initially found 41 mismatches: every one
was CRLF versus the LF git archive. CRLF-normalized comparison of all 70 files has
zero differences. Evidence: variant root's `working-source-comparison.json`.
Frozen-to-frozen comparisons and binary checks retain exact byte hashes.

Result: all 36 records complete and verify exactly. Repeated controls show small
mixed gains; the new 258 variable-only sample is 22.46% slower despite identical
structural work. Both changes remain candidates pending focused 258 repeats and
separate diagnostics. Full analysis and next steps:
`mem:solver/experiments/42-complete-propagation-variable-set-results`.
