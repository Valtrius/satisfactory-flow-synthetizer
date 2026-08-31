# 31. Remove duplicate physical-flow bounds from propagation

Date: 2026-08-31. State: successful; promotion recommended, uncommitted.

## Hypothesis

Every registered port currently has its positivity and capacity constraints checked
twice during synchronization. The direct variable scan rejects a known value when it
is nonpositive or above capacity. A second pass evaluates stored one-variable exact
inequalities for the same two conditions. An unknown inequality never rejects a state.

Remove the stored inequality path while retaining the direct checks. This should
reduce propagation cost without changing deductions, branching, cache identity, or
scheduling. Do not combine it with sparse-equation or union-find changes.

## Diagnostic split

Feature-gated hotspot telemetry now separates propagation port scanning, suffix
registration, fixed-point work, sparse analysis, and physical-bound checks. These
subtimers overlap the existing top-level propagation timer and must not be summed
with it.

A 30-second hard-10 optimal run used production p1/32 at N<=11. It cancelled cleanly
after 31.055 seconds without a layout, so it is throughput evidence only. Across
worker threads it recorded:

| Propagation work        | Aggregate time | Count                      |
| ----------------------- | -------------: | -------------------------- |
| Port scan               |        6.758 s | 1,959,654 synchronizations |
| Suffix registration     |        6.404 s | 1,959,654 synchronizations |
| Fixed point             |      255.201 s | 2,229,099 passes           |
| Sparse analysis         |      186.098 s | included in fixed point    |
| Physical-bound checking |      120.254 s | 1,959,654 synchronizations |

The run attempted 1,959,428 structural decisions and reported 677,516 propagation
contradictions. The large bound bucket justifies the isolated removal; it does not
predict whole-solve speedup.

Frozen reference binary:
`target/parallelism-ladder/propagation-bounds-before-20260831/bin/profile_case.exe`,
SHA-256 `73B2038CB9CAAE361E3D159FA1B0DDF8827F545B77B94A4237FF06A51EF7B80B`.

## Candidate and soundness

- Remove `ExactInequality` storage, checkpoint length, rollback, and evaluation from
  `PropagationState`.
- Keep weighted-union-find negative-ratio rejection.
- Keep the direct known-value checks for strict positivity and capacity.
- Keep equality propagation, sparse analysis, SCC solving, cache identity, proof
  accounting, validation, and production p1 scheduling unchanged.

Each removed constraint is exactly `x > 0` or `x <= capacity` for one registered
variable. When `x` is known, the retained direct scan checks both predicates. When it
is unknown, the removed evaluator returned unknown and could not prune. The two paths
therefore have the same accept/reject result for every propagation state.

## Correctness gate

- 179 solver-core library tests passed; two manual benchmarks were ignored.
- With `bench-internals`, 184 library tests passed and two were ignored. The exhaustive
  outer Reference differential and all four parallelism integrations passed.
- All workspace tests passed with five ignored.
- Strict solver-core all-target Clippy passed with and without `bench-internals`.
- All 33 analyzer tests passed.

The first strict Clippy pass found that the diagnostic phase enum was moved twice.
Deriving `Copy` fixed this instrumentation-only compile error before any candidate
benchmark. No solver result failed.

## Frozen screen

Manifest: `benchmarks/custom/propagation-bounds-screening.json`.

The randomized fresh-process screen contains 38 jobs:

- three samples per variant for 36 optimal;
- three samples per variant for 115 all L and minimum L;
- three samples per variant for 238 optimal, minimum L, and all L;
- one 60-second hard-10 diagnostic per variant with hotspots enabled.

Completed pairs must preserve status, proof, preferred witness, every solution object,
canonical layout-key sets, validation, and layout count. The hard pair is fixed-time
throughput evidence and supplies no completion claim. The detached runner freezes
the manifest, cases, scripts, binaries, source, and working-tree diff and provides a
completion dialog.

The plan-only runner accepted all 38 jobs. Frozen `profile_case.exe` SHA-256 values:

- experiment-29 reference with propagation telemetry:
  `73B2038CB9CAAE361E3D159FA1B0DDF8827F545B77B94A4237FF06A51EF7B80B`;
- experiment-31 candidate:
  `3E0D1674A89CFFDFE6313CFDC4D89384ACB171CA1F16F47E85AF254D5EB5F047`.

Run directory:
`target/parallelism-ladder/propagation-bounds-screen-20260831`. Do not infer a
result until its frozen analyzer reports completion.

## Decision gate

Make the removal permanent if exact verification passes and repeated completed
workloads are neutral or faster without a consistent regression. The operation is
provably redundant, so a small gain is sufficient. Keep weighted-representative
equation elimination and scheduler work separate.

## Results

The detached runner finished all 38 jobs and its frozen analyzer reports zero
failures. Thirty-six runs completed optimally; the two hard-10 diagnostics cancelled
cleanly at their common 60-second cap. No solver process failed or was killed.

Every completed comparison preserves status, proof, preferred witness, every solution
object, canonical layout-key set, validation, layout count, and the checked structural
counters. Each row has three fresh processes per variant:

| Workload      |    Before median (range) |     After median (range) | Wall reduction |
| ------------- | -----------------------: | -----------------------: | -------------: |
| 36 optimal    | 14.067 (13.962-14.159) s | 13.313 (13.197-13.412) s |          5.36% |
| 115 all L     | 45.147 (44.879-45.245) s | 42.482 (42.239-42.508) s |          5.90% |
| 115 minimum L |    3.402 (3.370-3.421) s |    3.109 (3.103-3.134) s |          8.61% |
| 238 all L     | 19.278 (18.835-19.309) s | 17.586 (17.571-17.744) s |          8.78% |
| 238 minimum L |    9.390 (9.374-9.432) s |    8.561 (8.551-8.587) s |          8.83% |
| 238 optimal   |    9.440 (9.367-9.446) s |    8.552 (8.551-8.587) s |          9.40% |

Median process CPU falls 6.43-9.27% across these workloads. Median first-witness
latency improves 5.36-9.40%; all six rows favor the candidate.

The capped hard-10 pair supplies fixed-time throughput evidence only:

| Metric                     |      Before |       After |      Change |
| -------------------------- | ----------: | ----------: | ----------: |
| Structural decisions       |   3,577,410 |   3,707,055 |  3.62% more |
| Retained states            |   1,433,027 |   1,483,168 |  3.50% more |
| Aggregate propagation time |   722.275 s |   639.618 s | 11.44% less |
| Bound-check time           |   216.155 s |   103.009 s | 52.34% less |
| Propagation time per sync  |   201.89 us |   172.53 us | 14.54% less |
| Sampled peak working set   | 3,074.6 MiB | 3,166.7 MiB |  3.00% more |

The remaining bound timer is the retained direct variable scan. Its per-sync cost
falls about 54%, despite the candidate completing more synchronization calls. The
hard pair does not establish completion time, and the working-set difference is not
a memory conclusion from one capped sample.

Summary SHA-256:
`9FD65D79DA76D374B241882C317CECA14BD6B26CB37FF800085FEF1FAF18D96C`.

## Decision

Promote the duplicate-bound removal. It passes the declared exactness gate, improves
every repeated completion and first-witness median, and advances more hard work within
the fixed cap. Keep the propagation subphase telemetry because it is feature-gated and
identifies the next cost. Keep weighted-representative equation elimination as a new,
separately validated experiment. Scheduling remains paused.
