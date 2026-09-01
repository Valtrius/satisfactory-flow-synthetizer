# 18 - Guarded p1 proposal

Date: 2026-08-29. Implemented and validated, then rejected before commit.
Promotion evidence (`mem:solver/experiments/17-results`).

## Proposal

Enable adaptive deep partitions in the shared production Custom adapter through
the current N=9 obligation, for both find-optimal and find-all. At N>=10, use the
existing baseline partition plan. Keep sharing, donation and parallel remaining
groups off. Direct native `SolveOptions::default()` remains baseline scheduling.

This policy matches every completed experiment 17 winner at N<=9 and avoids the
measured N=11 memory failure. It uses `node_count`, which is current proof state
inside the solver. It does not use case names, graph cyclicity or future knowledge.

## Trial implementation

`ParallelismOptions` keeps `deep_partitions` and adds an optional inclusive
`deep_partition_max_node_count`. None preserves unbounded experimental p1. A set
limit without deep partitions is invalid. `ProfileGroupRun::plan_roots` selects the
adaptive planner only when both the flag and current-N limit allow it.

`solver_core::solve_problem`, the shared Custom adapter used by the application,
sets deep partitions with a maximum N of 9. Benchmark stages continue to use
unbounded p1, so historical stage names retain their meaning. The option remains
explicit in native test/profiling paths.

The post-winning constructor candidate was removed from the production working
tree. Its frozen source and measurements remain in experiment 17 for a completed
all-mode follow-up. Benchmark-only p12/p123 fixed-work access remains present.

## Correctness and validation

The limit changes task partition boundaries, not the exact profile set, pruning,
validation or proof ledger. Tests cover the N=9/N=10 boundary, unbounded p1 and
rejection of a meaningless limit without the flag. Existing independent reference,
all-stage, worker-count, cancellation and exact-result tests remain in force.

Validation after promotion:

- 212 solver-core library/integration/example tests passed, two ignored.
- Six synthetizer-app unit tests and ten shared-API integration tests passed.
- Strict all-target solver-core Clippy passed with and without `bench-internals`.
- `cargo fmt --all` passed; documentation formatting and links pass.

Logs: `target/guarded-p1-tests.log`, `guarded-p1-app-tests.log`,
`guarded-p1-clippy.log` and `guarded-p1-default-clippy.log`.

## Outcome

The guard was not committed. The user explicitly rejected memory use as a gate for
the measured completion-time improvement. The limit field, boundary logic and test
were removed. Experiment 19 (`mem:solver/experiments/19-unconditional-p1-promotion`) replaces this proposal
with unconditional production p1. The validation logs remain evidence that this
intermediate implementation compiled and preserved the exact-result contracts.
