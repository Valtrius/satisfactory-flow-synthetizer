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

The optional constructor has no elapsed-time deadline in the permanent version.
The previous five-second policy is a separate, unapplied benchmark patch. See
[experiment 09](experiments/09-serializer-and-find-all.md) for source variants.

The `bench-internals` feature is off by default. Its fixed-work entry bypasses the
constructor and earlier groups; it does not change ordinary solver defaults.
It supports baseline/p1/p12/p123 and best/all collection. All exhaust selected profiles;
neither produces global optimality. See [12](experiments/12-hard-obligation-profiling.md).

The exact [contracts](contracts.md) take precedence over scheduler heuristics.
Before editing, check [status](status.md) for the committed/uncommitted boundary.
