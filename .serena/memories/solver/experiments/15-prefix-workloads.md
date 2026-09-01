# 15 - Exact prefix workloads for hard 10

Date: 2026-08-28. Implemented in `809f7c9`; initial discovery finished.
Motivation (`mem:solver/experiments/13-whole-results`), run protocol (`mem:solver/experiments/16-post-calculation-screen`).
The existing N=11/L=18 profile remains too long for completed comparisons.
The hypothesis is that a frozen subtree can supply a useful complete reference
without running the entire hard problem with one worker.

## Implementation

All new Rust entry points are behind `bench-internals`. Production defaults and
the three promoted calculation changes are unchanged.

- `search/prefix_benchmark.rs` uses production MRV decisions, exact propagation,
  pruning and canonical state keys. At each level it sorts and deduplicates the
  surviving child keys, then selects `pick % child_count`. Depth is bounded to 12.
- The selection certificate retains every visited child frontier, selected indices,
  exact decision debug strings and final canonical key. The debug-string format
  belongs to this versioned benchmark certificate, not a public graph protocol.
- `solver/benchmark/prefix.rs` requires one profile, one worker and baseline flags.
  Its prepared prefix uses fresh production root-search state for each replay,
  including normal prefix replay/key checks and independent caller-unit validation.
- `profile_obligation --list` returns the certificate. Preparation freezes it in
  the request. Execution regenerates and compares it before accepting any result.
  A changed route, decision, frontier or key fails instead of selecting new work.

Discovery is deterministic by structural state, with no wall-clock frontier cutoff.
The recipe and profile are benchmark inputs only; corpus labels never reach solver
policy. This does not change the production adaptive planner's existing time budget.

## Timing and proof scope

The result keeps `kind: fixed_obligation` but uses `scope: selected_prefix`.
`roots=1` and `roots_exhausted=1` mean exactly that selected subtree completed.
They cannot discharge the surrounding profile, N/L group or global solve.
Different depths may be nested; never sum these jobs as disjoint proof obligations.

`preparation_s` records certificate regeneration separately. Search `wall_s` starts
after preparation and includes fresh-state prefix replay and search teardown.
Process metrics also include preparation, parsing and startup. The cooperative
deadline applies to search. Preparation is depth-bounded and the process watchdog
still bounds the whole child; a watchdog kill is always a failed benchmark.

The verifier requires the exact scope, request, certificate, problem, profile and
one-root accounting. Exact result comparisons include prefix identity in their
grouping key, so different subtrees never become each other's complete references.
Incomplete prefixes remain incomplete, including requested cancellation.

## Discovery screen

Six hard recipes use depths 4/6/8 and picks 0/2 on profile S2=5,S3=2,M2=0,M3=4,
N=11/L=18, capacity 1200. The plan check found six distinct identities, with all
requested depths reached. No timing result follows from that check.

Each recipe runs best/all twice, 30 s per process, instrumentation off. Two tiny
prefix controls must exhaust. These are serial subtrees, not full hard single-worker
baselines. Select a completed workload lasting several to tens of seconds only
after results arrive. Very short or capped recipes are discovery outcomes, not
promotion evidence. Keep all outcomes instead of silently dropping unsuitable jobs.

Before another calculation change, freeze the selected exact certificate and run
fresh repeated before/after processes on it. Retain whole optimal/all SAT regressions.
If all candidates are too short or capped, adjust selection depth/path in a new
record rather than claiming better completion from more visited states.

The first screen (`mem:solver/experiments/16-diagnostics-and-next-steps`) passed
certificate/scope verification but all 24 hard jobs hit 30 s with no witnesses.
Both tiny controls exhausted. No hard prefix is yet a useful completed control.
Next, try deeper paths with one short all-mode trial per recipe before repeats.

## Validation and provenance

Rust coverage checks deterministic certificates, rejection of changed keys,
decisions/frontiers, cancellation, invalid budgets, and exact witness agreement.
The union of first-level tiny prefixes matches full selected-profile enumeration.
Native CLI tests check frozen identity, local result scope and rejection before
writing a result. Python tests reject scope/identity/accounting mismatches.

Logs: `target/prefix-tests.log`, `prefix-clippy.log`, `prefix-default-clippy.log`,
`prefix-build.log`, `prefix-tool-tests-final.log`. Final counts are recorded in status.
The first plan is preserved at `target/parallelism-ladder/prefix-plan-check-20260828/`.
It predates the explicit rejection of ignored p1 flags for prefix jobs; all planned
prefix jobs already used baseline. Final preparation uses a separately frozen build.

`scripts/freeze-custom-variant.py DESTINATION` freezes already-built executables,
all crate files, workspace Cargo files, git metadata and per-file hashes. It refuses
an existing destination. Build and validate first, with no source edits between
build and freezing. The helper records identity; it cannot prove a build is fresh.
