# 42. Complete propagation variable set

Date: 2026-09-01. State: validated; factorial screen pending.
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

Candidate implementation and this record: `520b352`. Benchmark infrastructure is
separate and not yet committed in this state.

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
the integer-row candidate under experiment 41's existing combined seven-sample gate.
If both pass, require the combined variant to preserve exact results and avoid a
credible interaction regression. Scheduling remains paused.

Do not infer a result until the detached runner and frozen analyzer finish.
