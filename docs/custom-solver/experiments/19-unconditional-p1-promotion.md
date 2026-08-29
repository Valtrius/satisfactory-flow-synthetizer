# 19 - Unconditional p1 promotion

Date: 2026-08-29. Permanent production change.
[Measured evidence](17-results.md).

## Decision

Enable adaptive deep partitions in the shared production Custom adapter for every
node count and for both find-optimal and find-all. Keep sharing, DFS donation and
parallel remaining groups off. Direct native `SolveOptions::default()` remains
baseline scheduling for tests and explicit native callers.

Experiment 17 measured large completion-time reductions at 16 and 32 workers on
every completed non-tiny workload. Hard-36 optimal fell 64.1-72.6%, short all-mode
cases fell 55.4-72.9% at 16/32 workers, and short optimal cases fell 50.1-65.1%.
Exact results and proof summaries passed the frozen comparisons.

Hard-10 capped runs used 2.84-6.32x the baseline peak memory and much more CPU.
This remains a measured cost. The user chose not to block the completion-time policy
on memory use, so the earlier N<=9 guard was removed before commit. No claim is made
that p1 improves hard-10 completion because those runs did not complete.

## Implementation

`solver_core::solve_problem`, the shared Custom adapter used by the application,
selects `deep_partitions: true` without an N limit. The other three scheduler flags
retain their false defaults. The public `ParallelismOptions` shape remains unchanged
because the rejected limit field was never committed.

The change affects partition boundaries only. It does not alter profiles, pruning,
solution validation, cancellation accounting or proof-ledger rules.

## Validation

- `cargo test -p solver-core --all-targets --features bench-internals`: 211 passed,
  two manual benchmarks ignored.
- `cargo test -p synthetizer-app`: six unit and ten shared-API tests passed.
- `python -m unittest scripts/test_analyze_parallelism.py`: 32 passed.
- Strict all-target solver-core Clippy passed with and without `bench-internals`.
- `npm run format` and `git diff --check` passed for the committed files.

Logs: `target/unconditional-p1-tests.log`, `unconditional-p1-app-tests.log`,
`unconditional-p1-tool-tests.log`, `unconditional-p1-clippy.log` and
`unconditional-p1-default-clippy.log`.

No new performance run is needed for this policy change. Experiment 17 measured the
same unbounded p1 algorithm through the benchmark API. This commit changes which
existing policy the shared production adapter selects.

## Follow-up

Treat hard-10 memory as separate optimization work if it becomes operationally
important. Do not restore an N gate without new evidence and a policy decision.
Next prioritize a completed all-mode A/B for the post-winning constructor guard,
then use the completed hard-10 depth-10/pick-4 prefix to test calculation changes.
