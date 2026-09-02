# Controls and code map

All `ParallelismOptions` flags default to false. The shared production Custom
adapter enables deep partitions without an N limit. Native options retain baseline
scheduling unless a caller selects a stage; no UI switch was added. Native
`SolveOptions` defaults to one worker, but callers and benchmarks may request another
count.

## Stage names

| Stage      | Enabled controls                                              |
| ---------- | ------------------------------------------------------------- |
| `baseline` | None                                                          |
| `p1`       | Adaptive static partitions, `deep_partitions`                 |
| `p12`      | Partitions plus `shared_state_cache`                          |
| `p123`     | Partitions, sharing, DFS sibling donation via `work_stealing` |
| `p1234`    | All four, including `parallel_remaining_groups`               |
| `shared`   | Completed-state sharing only                                  |
| `groups`   | Parallel remaining L-groups only                              |
| `p14`      | Partitions and groups                                         |
| `p124`     | Partitions, sharing and groups                                |

Donation requires sharing; invalid combinations are rejected. Remaining-group
parallelism applies only to enumeration, after the first satisfiable L-group at
the minimum N has completed. It does not speed the first SAT group directly.

## Algorithm boundaries

The adaptive planner replaces a prefix with legal continuations. It stops at
depth four, four roots per requested worker per profile, or its 250 ms budget.
The deadline is checked between operations. Timing can alter the final frontier;
canonical-sorted frontier IDs are local to the registered plan, not global identities.
Production and experimental p1 both use it without an N limit.

Completed-state sharing permits concurrent duplicate work and never waits on an
in-flight cache owner. Exact-L scope and a group-owned witness registry preserve
borrowed SAT results. Donation uses a bounded group deque and executes queued work
while joining, including with one worker. All children must finish before parent caching.

The current experiment-29 working tree defers local state canonicalization until a
cheap invariant bucket repeats. A fingerprint never proves equality. Repeated buckets,
shared-cache paths, and donation paths use authoritative canonical state keys. The
frozen A/B passed and the implementation is permanent.
[Experiment 30](experiments/30-no-state-cache-ablation.md) removed this local cache in
a validated fail-fast trial. Structural work rose enough to make 238 optimal 3.00x
slower, so the uncached policy is absent from production source.

Remaining groups have a bounded number of slots. Their worker budgets sum to the
requested search workers; reporter/coordinator threads are additional. Allocations
do not move between groups after a group finishes. A common coarse pool is only a
proposal. Release builds use `panic = "abort"`; a panic can terminate the process.

## Where to work

Paths are relative to the repository root.

| File or area                                                                         | Responsibility                                                                 |
| ------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------ |
| `crates/solver-core/src/solver.rs`                                                   | Solve modes, current N/L, constructor coordination, group scheduling           |
| `crates/solver-core/src/solver/benchmark.rs`                                         | Feature-gated exact N/L/profile entry using production group search            |
| `crates/solver-core/src/solver/benchmark/root.rs`                                    | Frozen adaptive-root identity and exact serial replay                          |
| `crates/solver-core/src/search.rs`                                                   | Root planning/DFS, caches, donation, finalization and teardown                 |
| `crates/solver-core/src/canonical.rs`                                                | Full-witness key, partial canonicalization, state/SCC key serialization        |
| `crates/solver-core/src/acyclic_incumbent.rs`                                        | Optional exact certificate constructor, eligibility, reuse and subset sums     |
| `crates/solver-core/src/algebra/`                                                    | Exact equality/inequality calculations                                         |
| `crates/solver-core/src/lower_bound.rs`                                              | Proven lower bounds and required source denominator                            |
| `crates/solver-core/src/proof_ledger.rs`                                             | Exhaustion accounting                                                          |
| `crates/solver-core/src/hotspot_profile.rs`                                          | Optional aggregate timers/counters                                             |
| `crates/solver-core/src/diagnostics.rs`                                              | Bounded activity trace, phase spans, in-flight/heartbeat observations          |
| `crates/solver-core/examples/profile_support/mod.rs`                                 | Real API profiling, independent validation, saved results, diagnostic writers  |
| `profile_case.rs`, `profile_witness.rs`, `profile_tiny.rs` in the examples directory | File cases, isolated saved-witness replay, fixed-overhead regression case      |
| `crates/solver-core/tests/parallelism.rs`                                            | Complete result maps, preferred witnesses, stages and worker counts            |
| `scripts/parallelism-ladder.ps1`, `start-benchmark-screen.ps1`                       | Randomized fresh processes, provenance, freeze/launch/watchdog/notification    |
| `scripts/analyze-parallelism.py`, `test_analyze_parallelism.py`                      | Exact result and capped-run verification, regression tests                     |
| `profile_obligation.rs`, `scripts/hard-profile.py`, `scripts/start-hard-profile.ps1` | Fixed-work profiling, strict local-scope verification, freeze and notification |

Diagnostics are disabled for ordinary timing runs. The current diagnostic example
writes independent heartbeat counters every five seconds without a trace mutex or
JSON allocation. Full post-cancellation sidecars use a separate thread and 15-second
cadence. Both writers are joined; their work is not hidden after process completion.
Feature-gated hotspot output also splits propagation into port scanning, suffix
registration, fixed-point passes, sparse analysis, and direct physical-bound checks.
These nested timers overlap the top-level propagation timer.
Experiment 34 further splits sparse analysis when hotspot recording is active. It
records preparation, forward elimination, back reduction, deductions, pass cause, and
flat row/variable/term size buckets. Ordinary solves keep the timer-free sparse path.
Experiment 35 further splits the hotspot-only preparation path into variable
collection, substitution and normalization, tautology filtering, sorting, and
working-row conversion. The timer-free production path remains unchanged.
Experiment 36 permanently evaluates fully known sparse rows with one exact integer
denominator LCM and returns canonical tautology or contradiction rows directly.
Experiment 37 permanently returns the existing primitive row when no coefficient has
a known value. Experiment 38 permanently substitutes mixed rows with one exact integer
denominator LCM and one final primitive normalization.
Experiment 39 shows that state keys now own nearly all graph-purpose work and that
equality plus inequality encoding dominate current state/SCC canonicalization.
Experiment 40 splits those broad hotspot timers into indexing, row construction,
rational RREF, primitive normalization, and sort/deduplication. Hotspot-off solves keep
the original timer-free paths.
Experiment 32 tested quotienting sparse rows through exact weighted representatives.
The candidate was exact but slower on 36/238 and is absent from production source.
Experiment 33 tested identical normalized-row deduplication and found no duplicates in
the hard sample. The candidate and its counters are absent from production source.
Live progress reports the optional serial helper as `constructing_incumbent`; its
activity trace kind remains `acyclic_construct`.

The optional constructor has no elapsed-time deadline in the permanent version.
After find-all validates a witness at the current N, later link groups skip this
optional existence helper and continue exact enumeration directly.
The previous five-second policy is a separate, unapplied benchmark patch. See
`mem:solver/benchmarking` (Source variants / patches) and
`mem:solver/experiments/09-serializer-and-find-all` for source variants.

The `bench-internals` feature is off by default. Its fixed-work entry bypasses the
constructor and earlier groups; it does not change ordinary solver defaults.
It supports baseline/p1/p12/p123 and best/all collection. All exhaust selected profiles;
neither produces global optimality. See [12](experiments/12-hard-obligation-profiling.md).
Frozen adaptive-root replay regenerates the production p1 plan and verifies its full
canonical-key identity before searching one serial root. Root exhaustion cannot
discharge the containing profile. See [21](experiments/21-adaptive-root-profiling.md).

The exact contracts (`mem:solver/contracts`) take precedence over scheduler heuristics.
Before editing, check status (`mem:solver/status`) for the committed/uncommitted boundary.

Experiment47 splits canonical RREF only with hotspot recording: normalization,
suffix construction, elimination, finalization, factor/update counts and maximum
observed rational bit lengths. The ordinary reducer is unchanged. These nested
phase times include local instrumentation and overlap outer RREF/equality timers.
Integer/unit/zero categories overlap; factor row counts differ from weighted update
counts. See `mem:solver/experiments/47-rref-arithmetic-profile` before interpreting them.
