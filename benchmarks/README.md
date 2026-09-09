# Solver benchmarks

Build `cargo build --release -p synthetizer-app --example profile_solver --locked`.
The current runner takes `seconds workers max_nodes output.json scope case.json`.
Scopes: `one_min_nl` (One min N/L), `all_min_nl` (All min N/L), `all_min_n` (All min N).
The exact full-witness comparison uses `layout-v1` after the timed solve.

Prepare without timing:

```powershell
./scripts/start-benchmark-screen.ps1 -JobManifest benchmarks/smoke.json -OutputDirectory target/solver-plan -PlanOnly
```

Omit `-PlanOnly` to launch. The launcher freezes source, binaries, cvc5/DLLs,
cases, schedule and hashes. It runs sequentially, verifies saved results, writes
`BENCHMARK-STATUS.txt` and displays a completion dialog. After launching, end the
agent turn and leave the machine idle. No long experiment is authorized by this guide.

For paired experiments use `VariantBinaryMap`, a JSON object mapping variant
names to directories containing `profile_solver.exe`, with adjacent frozen
`solver-source` and metadata. Add two jobs per PairId with reference/candidate
roles, equal settings, distinct variants and balanced repeats. Worker count,
capacity, scope and instrumentation must match. Root traces use AstraDiagnostics;
keep them separate from ordinary timing pairs. Retained fields Stage=baseline and
Hotspots=off are fixed protocol metadata, not selectable search controls.

Current manifests require RunnerProtocol=layout-v1. `smoke.json` is a small
example. Earlier manifests and binary maps in this directory are historical
Astra experiments: they preserve original engine labels, scope names and binaries.
Use their original frozen harness under target to replay them; do not feed them
to the new runner. The rejected output-pairs patch uses historical crate paths.

Experiments and results: [memory index](../.serena/memories/experiments/index.md).
Full canonical objects and exact result sets must agree for enumeration. Optimal
ties may differ. Timeouts never become completed-time speedup denominators.
Process CPU/memory samples describe the parent only, excluding cvc5 children.
