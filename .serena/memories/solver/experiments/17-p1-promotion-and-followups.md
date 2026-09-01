# 17 - P1 promotion and focused follow-ups

Date: 2026-08-29. Finished and verified. Results (`mem:solver/experiments/17-results`).
Experiment 16 findings (`mem:solver/experiments/16-post-calculation-results`),
diagnostic recommendations (`mem:solver/experiments/16-diagnostics-and-next-steps`).
Production scheduling defaults remained unchanged during measurement.

## Questions

1. Does adaptive partitioning, p1, deserve a current production default for
   find-optimal, find-all, or both?
2. Can enumeration skip the optional existence constructor after a winning N is
   known, without changing complete results, proof or find-optimal behavior?
3. On completed fixed work, does state sharing help, and does bounded donation add
   enough completion benefit to pay for sharing?
4. Do deeper exact hard-10 prefixes produce a useful completed control?

Case names are corpus labels only. Solver policy receives exact inputs, current N,
profiles and runtime state; it receives no cyclicity classification.

## Changes under measurement

The production candidate adds one guard in `solver.rs`: do not run the optional
acyclic constructor when `winning_node == Some(node_count)`. The solver already has
a validated witness at that N, and complete enumeration must still search every
exact profile. Earlier groups, the first witness, find-optimal, exhaustive search,
validation and proof folding are unchanged. This is a candidate, not permanent yet.

The `bench-internals` fixed-work API now accepts p1, p12 and p123. It passes the
existing `ParallelismOptions` to production group search and rejects remaining-group
concurrency or donation without sharing. Exact prefix replay still requires baseline,
one worker and one profile. This access is benchmark-only; it does not alter policy.

## P1 promotion matrix

`benchmarks/custom/p1-promotion-whole.json`, 160 jobs. The candidate binary compares
baseline and p1. Instrumentation is off and capacity is 1200 throughout.

| Work                  | Workers   | Repeats per baseline/p1 cell | Cap                                   |
| --------------------- | --------- | ---------------------------- | ------------------------------------- |
| Tiny optimal/all      | 1/4/16/32 | Three                        | 5 s, must complete                    |
| 24 and 65 optimal/all | 4/16/32   | Three                        | 15 s optimal, 90 s all, must complete |
| 24 and 65 optimal     | 1         | Three                        | 15 s, must complete                   |
| Hard 36 optimal       | 16/32     | Three                        | 150 s, must complete                  |
| Hard 10 optimal/all   | 16/32     | One                          | 45 s, incomplete allowed              |

The hard-10 samples measure current memory, CPU, cancellation and bounded proof
progress. They are not completion comparisons. There is no hard single-worker run.

Treat optimal and all as separate promotion decisions. Exact saved results and
proofs must pass first. A p1 promotion requires a repeatable hard completion gain,
no completed 24/65 regression exceeding both 5% and 0.1 s, and tiny medians below
20 ms. If hard-10 peak memory exceeds baseline by more than 2x, do not enable p1
unconditionally. A narrower runtime policy needs its own measured condition and
must use facts available to the solver. These gates are decisions recorded before
timing, not guarantees about the pending run.

## Constructor comparison

The whole manifest also has two reference and two candidate p1/32 repeats for
hard-36 optimal at 60 s and all at 120 s. Reference and candidate source snapshots
differ in production code only by the post-winning constructor guard. Optimal must
retain its complete result and timing distribution. All may remain capped; compare
first witness, full saved partial objects, proof progress, CPU and memory separately.
Do not turn the earlier 26.366 s diagnostic span into an expected speedup.

## Fixed work and prefixes

`benchmarks/custom/p1-promotion-fixed.json`, 20 jobs. P1, p12 and p123 each run
best/all twice on the complete hard-36 N=9/L=12 group with 32 workers and 120 s
caps. Completed group runs must have the same exact results; a cap remains an explicit
negative scheduling outcome and does not stop the whole phase. P12 isolates sharing;
p123 adds donation. Local exhaustion is not a whole-solve proof.

Six hard-10 all-mode discovery jobs use depths 10/12, picks 0/2/4, one worker and
8 s caps. Freeze any useful completed certificate before repeating it. Two p123
tiny controls must exhaust. Capped prefixes remain recorded failures of selection,
not evidence of improvement. Nested prefixes are never summed as disjoint proof.

## Provenance, validation and launch

Reference snapshot: `target/parallelism-ladder/p1-promotion-variants-20260829/reference/`.
It contains benchmark access but not the constructor guard. Candidate snapshot:
`target/parallelism-ladder/p1-promotion-variants-20260829/candidate/`.
Each contains 78 hashed files and the three release examples. The whole comparison
uses the candidate for current baseline/p1 and preserved variants for the constructor A/B.

Before freezing each variant, 211 solver-core library/integration/example tests
passed, two ignored. Strict all-target Clippy passed with `bench-internals`; the
candidate also passed default-feature Clippy. 32 Python tooling tests passed.
Release examples built. `cargo fmt --all` passed before both freezes. Logs:
`target/p1-reference-*`, `target/post-win-constructor-*` and
`target/p1-promotion-tool-tests.log`.

The final plan check passes 20 fixed and 160 whole identities at
`target/parallelism-ladder/p1-promotion-plancheck-final-20260829/`. Search caps total
143.0 minutes; prior timings suggest roughly 45-60 minutes. Jobs run sequentially,
with no build/test activity during timing. The launcher writes durable status and
one final dialog. First launch failed after fixed work because its run-local candidate alias did not match the runner's `basis` path. Evidence is preserved at `target/parallelism-ladder/p1-promotion-20260829/`. The corrected run root is `target/parallelism-ladder/p1-promotion-20260829-retry1/`.
