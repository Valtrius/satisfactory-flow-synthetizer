# Solver benchmarks

The current solver-core source is the baseline. Scheduling experiments are paused
after the [hybrid promotion](optimization/results-hybrid-20260910.md).
The [experiment index](optimization/README.md) records the accepted improvements
and adverse results; [evidence.json](evidence.json) locates frozen local artifacts.

## Current tools

Build the runner with:

```powershell
cargo build --release -p synthetizer-app --example profile_solver --locked
```

Its arguments are `seconds workers max_nodes output.json scope case.json`.
Scopes are `one_min_nl` (One min N/L), `all_min_nl` (All min N/L), and
`all_min_n` (All min N). The case corpus is in [cases](cases).

Prepare the eight-worker smoke example without timing:

```powershell
./scripts/start-benchmark-screen.ps1 -JobManifest benchmarks/smoke.json -OutputDirectory target/solver-plan -PlanOnly
```

Omit `-PlanOnly` only for an authorized timing run. The launcher freezes source,
executables, cvc5, inputs and schedule; it runs sequentially, verifies results,
writes durable status and gives a completion dialog. End the agent turn after
launch. No new performance campaign is authorized by this guide.

For a bounded queue, retain `freeze-optimization-campaign.ps1`,
`start-optimization-campaign.ps1` and `start-optimization-suite.ps1`. They accept
explicit manifests and verified binary maps, preserve failures and enforce the
three-hour session limit including cleanup and verification. Completed sweep
generators and candidate source templates were removed; their exact revisions
and frozen copies remain reproducible through the experiment index.

## Comparison and diagnostics

- Use at least eight total workers for performance. Correctness contracts also
  exercise supported smaller budgets. Prioritize hard exact completion; the user
  accepts reasonable easy-case penalties up to roughly ten seconds.
- Pair identical problems, capacities, scopes, worker counts, deadlines and
  instrumentation. Each PairId has reference/candidate roles and distinct
  variants. Keep runs adjacent and balance their order. `VariantBinaryMap` maps
  names to runner directories with adjacent source and metadata.
- Require `RunnerProtocol=layout-v1`. Stage=baseline and Hotspots=off are fixed
  protocol metadata. Full canonical sets and saved witnesses must agree for
  enumeration; One min N/L permits any validated equal optimum.
- Never use a timeout as a measured completion time. Keep adverse pairs and
  incomplete proofs even when every known layout was found. Compare within a
  campaign, without pooling baselines from different sessions.
- `SOLVER_CVC5` selects the backend. `SOLVER_DIAGNOSTICS=1` enables root traces;
  keep these probes separate from timing. `audit-optimization-results.py` checks
  independent proof ownership and complete parent/child covers. Root durations
  overlap; process CPU/memory samples exclude backend children.

Run the maintained harness checks with
`python -m unittest discover -s scripts -p "test_*.py"`.
Frozen results retain their matching harness. Reverify them with that copy,
not a subsequently changed implementation.
