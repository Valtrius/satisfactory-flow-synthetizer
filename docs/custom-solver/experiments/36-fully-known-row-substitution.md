# 36. Fully known row substitution

Date: 2026-08-31. State: completed and promoted; see [results](36-fully-known-row-substitution-results.md).
Related: [preparation profile results](35-sparse-preparation-profile-results.md).

## Question and hypothesis

Experiment 35 attributes 89.99-91.42% of sparse preparation to substitution and
primitive normalization. Experiment 34 shows that 79.7-91.9% of stored row
instances become tautologies after substitution. Can fully known rows be evaluated
more cheaply without changing exact sparse results?

## Change and comparison

`SparseRow::substitute` first checks whether every retained coefficient has a known
value. For such a row, the candidate computes one positive denominator LCM and the
exact integer residual

`D * rhs - sum(coefficient * numerator * D / denominator)`.

A zero residual returns canonical `0 = 0`; any nonzero residual returns canonical
`0 = 1`. If any coefficient is unknown, the established `BigRational` substitution
path runs unchanged. The candidate does not alter propagation facts, mixed rows,
elimination, canonicalization, DFS, proof accounting, or scheduling.

An exhaustive unit comparison retains the prior rational implementation as a test
oracle across small two-variable rows and rational assignments.

## Frozen comparison

Manifest: `benchmarks/custom/fully-known-substitution-screening.json`.

- 115 all-L enumeration, p1/32, N<=12: three samples per variant, 120-second cap;
- 238 optimal, p1/32, N<=12: three samples per variant, 60-second cap;
- hard 10 optimal, p1/32, N<=11: one sample per variant, 60-second cap.

All 14 jobs disable hotspot recording. Completed results must preserve every exact
output. The hard pair measures work within a common cap and cannot establish
completion speed.

Reference commit: `0b594a5`. Frozen reference `profile_case.exe` SHA-256:
`1E7B837244875D9DB2087886CDD8A525848838BDAE6FA5355CD5F4DEA9834B06`.
Candidate `profile_case.exe` SHA-256:
`70D2819AEB676133060FAAEEA86D8FBC6051735F92047973CBFEB60ADBE003EE`.
Run directory:
`target/parallelism-ladder/fully-known-substitution-20260831`.
The plan-only runner accepted all 14 jobs. The detached runner freezes both binaries,
solver source, manifest, cases, and analyzer and provides a completion notification.

## Results

[All 14 records verify](36-fully-known-row-substitution-results.md). The completed
three-sample medians improve 9.73% on 115 all and 16.41% on 238 optimal, with
identical exact outputs and structural work. Hard 10 processes 6.56% more states in
the common cap while reaching the same proof obligation. The candidate is promoted.

## Correctness and limitations

The integer residual is the original equation multiplied by the LCM of the known
denominators. That multiplier is positive and nonzero, so zero and nonzero residuals
are exactly equivalent to the rational path's tautology and contradiction results.

This experiment does not optimize rows with zero or some known variables. It cannot
be used to infer the value of a general mixed-row rewrite.

Validation completed before launch:

- the exhaustive substitution oracle passes across 28,125 row/value combinations;
- 186 benchmark-feature library tests pass, with two manual tests ignored;
- the exhaustive outer Reference differential and four parallelism integrations pass;
- all workspace tests pass with five manual tests ignored;
- strict all-target solver-core Clippy and all 33 analyzer tests pass;
- source and record formatting, the release build, and a completed hotspot-off tiny
  smoke solve pass. The smoke returns one validated `Optimal(N=2,L=1)` layout.

## Decision and next step

The promotion gate passes. Keep the fully-known integer path and test the
no-known-variable clone path separately. Scheduler work stays paused.
