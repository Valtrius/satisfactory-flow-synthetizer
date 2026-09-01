# 21 - Adaptive root-tail profiling

Date: 2026-08-29. State: completed and verified.
Related: experiment 20 results (`mem:solver/experiments/20-results`),
exact prefix workloads (`mem:solver/experiments/15-prefix-workloads`), results (`mem:solver/experiments/21-results`).

## Question

Which calculation dominates the longest serial root left by the 238 medium case,
and does collect-all add a distinct cost relative to retaining one best witness?
Scheduling policy remains unchanged during this diagnosis.

## Selected work

The completed experiment 20 trace contains 38 adaptive roots for 238 at N=8/L=13,
profile S2=1, S3=4, M2=0, M3=3. Root ordinal 28 took 82.731 seconds. The next
longest root took 29.321 seconds. The same ordinal is reconstructed from production
p1 planning with target 128; its 38-key plan and selected key are frozen in every
benchmark request.

This selection uses solver-visible problem, N/L/profile, worker budget and canonical
partition keys. The cyclic corpus label is not used by production behavior.

## Benchmark-only implementation

`solver::benchmark::prepare_root` rebuilds the production adaptive plan, selects one
canonical ordinal and retains the full ordered plan-key list plus the selected key.
Execution regenerates and compares that identity before replaying the root with fresh
production search state. Exhaustion has `scope: selected_root`, one local root, and
cannot discharge the containing profile, N/L group or whole solve.

`profile_obligation`, `hard-profile.py` and its verifier now preserve and validate
the frozen root identity. Prefix and root selections are mutually exclusive. Exact
comparisons include the selection identity, so unrelated roots cannot become
references for each other.

## Screen

Manifest: `benchmarks/custom/root238-screening.json`, six serial jobs.

- all and best: two ordinary repeats each, 120-second search caps;
- one instrumented diagnostic per mode, 150-second caps;
- every job must exhaust the exact selected root;
- all use p1 planning identity with 32 workers, while root execution itself is
  deliberately serial.

The total cap is 13 minutes. Instrumented jobs locate calculation costs but stay out
of ordinary timing medians. Result verification requires the frozen request/problem,
profile, plan identity, exact witness objects, one-root accounting, no watchdog kill
and no open activity spans.

## Validation before launch

- The new Rust coverage proves deterministic plan identities, changed-identity
  rejection, local one-root exhaustion and the union of all tiny adaptive roots
  matching full selected-profile enumeration.
- The solver-core bench-internals suite passes 182 tests with two manual benchmarks
  ignored. Strict all-target Clippy and release example builds pass.
- All 33 Python analyzer/runner tests pass. The native executable test covers valid
  root replay, frozen-identity rejection, independent witness validation and scope.
- A 238 probe reconstructs 38 partitions, target 128, ordinal 28. The selected key
  SHA-256 is `7f4c94106307e880770a4b59c138e71022d37aff5b4fbe525753c9f77324072f`.

No calculation or production scheduling code changed in this experiment. The run
completed and its frozen verification passed; see the results (`mem:solver/experiments/21-results`).
