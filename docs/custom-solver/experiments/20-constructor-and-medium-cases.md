# 20 - Post-winning constructor and medium cases

Date: 2026-08-29. State: completed; guard selected for production.
Related: [experiment 17 results](17-results.md),
[unconditional p1 promotion](19-unconditional-p1-promotion.md),
[measured results](20-results.md).

## Questions

1. Does skipping the optional acyclic constructor after find-all has established
   the winning N preserve complete results and shorten completed enumeration?
2. Do 238 = 60+20+108+50 and 115 = 75+40 provide useful medium-duration controls?
3. Are the user's low-CPU intervals the serial optional constructor or another phase?

The screenshot shows parallel plateaus separated by long low-CPU intervals. That
shape is consistent with serial constructor work observed on hard 36, but the image
alone cannot identify a solver phase.

## Changes

The candidate skips the optional existence constructor only when
`winning_node == Some(node_count)`. The solver has already validated a witness and
proved all smaller N at that point. Exact search still exhausts every remaining
profile and link group. Find-optimal and all pre-witness behavior are unchanged.

Live progress now reports `constructing_incumbent` before this serial helper.
The frontend labels it "Trying an acyclic incumbent". Instrumented benchmark jobs
also record one-second process CPU, working-set and thread-count samples beside the
existing exact `acyclic_construct` and `root_search` activity spans.

The policy uses only the current proof state. The cyclic/acyclic corpus labels never
enter solver behavior.

## Cases and matrix

Both new cases use capacity 1200.

| Case        | User-observed first / complete | Enumeration shape |
| ----------- | ------------------------------ | ----------------- |
| `cyclic238` | about 1 min / 1 min 34 s       | One solution      |
| `cyclic115` | about 10 s / 2 min 30 s        | Many solutions    |

Manifest: `benchmarks/custom/constructor-medium-screening.json`, 26 jobs, p1/32.
The cases, manifest, process sampler and benchmark guide are committed in `f70c060`.

- 24 and 65 all: before/after, three repeats, 45 s caps.
- 238 and 115 all: before/after, two repeats, 240/360 s caps.
- 238 and 115 optimal: one before/after check, same caps.
- 238 and 115 all diagnostics: one candidate sample each with hotspots and process
  sampling enabled. These aliases are excluded from timing comparisons.

All timing cohorts must complete and match exact status, canonical layout-key set,
preferred witness, full saved solutions and proof consistency. The stated caps total
79 minutes; user timings suggest about 20-25 minutes of actual solver work. A cap or
watchdog kill is not a completed comparison.

## Variants and validation

Frozen variants live under
`target/parallelism-ladder/constructor-medium-variants-20260829/`. Reference and
candidate include the new phase reporting; they differ in solver behavior only by
the post-winning constructor guard. The launcher freezes binaries, source, scripts,
manifest and cases again at launch.

The frozen source comparison differs only in `solver.rs`: the guard helper and its
unit test. Binary SHA-256 values:

- Reference: `137B81D43C50408318715DA5F42EC08935F1DD34FCB8B4D8F95BFF6E6BE04B95`
- Candidate: `C6312A01527D8CC30523B8EB15CD8EF593D1698FAE9BCCE61AD8698E6F179313`

Prelaunch validation:

- 212 solver-core library/integration/example tests passed, two manual benchmarks
  ignored. This includes the new guard test, exact reference enumeration,
  cancellation and every scheduler stage.
- Six synthetizer-app unit and ten shared-API tests passed.
- 89 frontend tests passed; Svelte check reports zero errors and warnings.
- 32 benchmark analyzer tests passed.
- Strict solver-core Clippy passed with and without `bench-internals`.
- `npm run format`, `git diff --check` and the frozen 26-job plan passed.

Logs use the `target/constructor-medium-*` prefix. Plan check:
`target/parallelism-ladder/constructor-medium-plancheck-final-20260829/`.
Performance conclusions are recorded only in the [results](20-results.md).

Run root: `target/parallelism-ladder/constructor-medium-20260829/`. The background
launcher freezes all inputs, shows a completion/failure dialog and writes the
authoritative status and final marker files.

## Decision

Promote the guard. It removes an optional existence check that cannot alter exact
enumeration after the current N has a validated witness. The completed A/B preserves
all exact results; timing is mixed and does not establish a broad speedup. Keep the
two medium cases for later calculation screens. Scheduler sharing, donation and
remaining-group work stay paused.
