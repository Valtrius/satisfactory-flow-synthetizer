# Benchmark workflow

Goal: reduce terminal wall time on the hardest exact problems. Build release, run correctness and strict Clippy, then freeze source, executable, cvc5/DLLs, cases, manifest and hashes before timing. Case names never select solver behavior.

Current corpus: benchmarks/cases. Historical Astra manifests and binary maps are in benchmarks; old Custom experiment suites are removed. Historical files retain original engine/scope metadata as evidence. Do not silently run an old manifest against a different protocol.

Current runner: cargo build --release -p synthetizer-app --example profile_solver.
Use scripts/start-benchmark-screen.ps1 for frozen runs with a completion dialog and durable status. End the turn after launch; let the machine idle. PlanOnly prepares without measuring.

Use balanced paired variants, exact same problem/capacity, scope, worker budget, timeout and diagnostics settings. Full result-set and saved witness equality for enumeration; any validated equal optimal tie for One min N/L. Record first witness, first optimum proof, All min N/L completion and All min N completion separately. Include worker/process cleanup in terminal wall time. Root times overlap and are not CPU time; parent-only process metrics exclude cvc5.

A timeout is censored, not a measured completion time or a speedup denominator. Keep regressions, failures, missing independent references and incomplete scopes. Compare within matched screens, not medians from different days. Two/three repeats are screening evidence.

Experiment history: `mem:experiments/index`. Historical runner protocol used exhaustive byte-canonical witnesses; the new comparison protocol is layout-v1 and must be matched between variants.
