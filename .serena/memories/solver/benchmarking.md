# Benchmark workflow and priorities

Goal: reduce terminal wall time of the hardest exact requests. The user accepts reasonable easy penalties up to about ten seconds and explicitly accepted the hybrid's case-97 uncertainty (`mem:experiments/13-hybrid-final`). Keep adverse samples; the timeout is not established to be a fluke. Scheduling experiments are paused. Future timing requires a new request, >=8 total workers, and <=3 hours including cleanup and verification. Smaller-worker correctness still matters; performance below eight cannot block promotion.

Current guides: benchmarks/README.md and benchmarks/optimization/README.md. Evidence IDs resolve through benchmarks/evidence.json. The corpus and eight-worker smoke manifest remain; completed sweep recipes/manifests are reproducible from Git 2dccf50 and frozen artifacts. Never edit a completed campaign to reflect a later promotion.

Build the release profile_solver example. Freeze source/binary/backend/cases/schedule before timing. Match input, scope, cap, workers, instrumentation and balanced adjacent order. layout-v1 compares complete canonical sets and saved witnesses; One min N/L accepts any validated equal optimum. Keep first witness, objective proof and exact scope completion distinct. Include worker cleanup in terminal time; comparison canonicalization is outside it.

A cap is censored, never a completion-time denominator. Preserve regressions and incomplete proofs even if layout counts match. Compare within a campaign, without pooling session baselines or treating scaled inputs as independent evidence. Root durations overlap; parent-process CPU/memory excludes cvc5 children. Timing root traces are disabled; diagnostic probes are separate.

Retained tools: start/run-benchmark-screen, benchmark_policy, analyze-benchmarks, audit-optimization-results, and bounded freeze/start-optimization campaign/suite scripts. Python tests cover exact-result comparison, proof covers, cancellation and session limits. Launch authorized benchmarks with durable status and a completion dialog, then end the turn.

The user authorized the five release-audit performance opportunities on 2026-09-11 with no total session time constraint. Finish and commit the remaining fixes first, then use a separate benchmark branch. This campaign may exceed the usual three-hour budget but must retain individual watchdogs, exact comparisons, adverse samples, frozen provenance and completion notification. Historical 1.0.0 release runs are postponed until this campaign is reviewed.
