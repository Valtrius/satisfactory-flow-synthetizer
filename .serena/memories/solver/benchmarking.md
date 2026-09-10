# Benchmark workflow

Goal: reduce terminal wall time on the hardest exact problems. The user accepts reasonable easy-case penalties up to roughly 10 additional seconds for substantial hard-case gains. Future performance benchmarks must use at least 8 total solver workers; performance below 8 workers must not block promotion. The next session is limited to three hours including cleanup and verification. Normal exactness and cancellation contracts still apply. The current solver-core code is the baseline. Build release, run correctness and strict Clippy, then freeze source, executable, cvc5/DLLs, cases, manifest and hashes before timing. Case names never select solver behavior.

Corpus: benchmarks/cases. Experiment manifests and binary maps are in benchmarks. benchmarks/evidence.json maps evidence IDs in memories to exact frozen run or source directories. Recorded artifacts preserve the paths, schema keys and labels required for reproducibility; use their matching frozen harness.

Runner: cargo build --release -p synthetizer-app --example profile_solver.
Use scripts/start-benchmark-screen.ps1 for frozen runs with a completion dialog and durable status. End the turn after launch; let the machine idle. PlanOnly prepares without measuring.
Current manifests require RunnerProtocol=layout-v1 and use Diagnostics for root traces. The runner emits diagnostics_enabled and roots; SOLVER_DIAGNOSTICS controls root tracing and SOLVER_CVC5 selects the backend.

Use balanced paired variants, exact same problem/capacity, scope, worker budget, timeout and diagnostics settings. Full result-set and saved witness equality for enumeration; any validated equal optimal tie for One min N/L. Record first witness, first optimum proof, All min N/L completion and All min N completion separately. Include worker/process cleanup in terminal wall time. Root times overlap and are not CPU time; parent-only process metrics exclude cvc5.

A timeout is censored, not a measured completion time or a speedup denominator. Keep regressions, failures, missing independent references and incomplete scopes. Compare within matched screens, not medians from different days. Two/three repeats are screening evidence.

Experiment history: `mem:experiments/index`. The layout-v1 witness comparison protocol must match between variants.
