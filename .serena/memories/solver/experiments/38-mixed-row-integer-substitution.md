# 38. Mixed-row integer substitution

Date: 2026-08-31. State: promoted.
Related: no-known promotion (`mem:solver/experiments/37-no-known-row-substitution-results`).
Results: verified A/B (`mem:solver/experiments/38-mixed-row-integer-substitution-results`).

## Question and hypothesis

Experiments 36 and 37 optimize fully-known and no-known sparse rows. Mixed rows still
construct and normalize a `BigRational` after each known term. Can one integer common
denominator make the final substitution case cheaper while preserving the exact
canonical row?

## Change and comparison

The coverage scan now visits every coefficient once, counts known variables, and
computes the LCM of their positive denominators. Fully-known and no-known rows retain
their promoted paths. For a mixed row, the candidate computes:

- residual RHS `D * rhs - sum(known coefficient * numerator * D / denominator)`;
- each unknown coefficient as `coefficient * D`;
- one final primitive gcd/sign normalization through `SparseRow::from_parts`.

The previous path built `BigRational` products and normalized after each subtraction,
then cleared the final denominator. Propagation, elimination, canonicalization, DFS,
proof accounting, and scheduling are unchanged.

## Frozen comparison

Manifest: `benchmarks/custom/mixed-integer-substitution-screening.json`. It repeats
the prior 14-job workload:

- 115 all-L enumeration, p1/32, N<=12: three samples per variant, 120-second cap;
- 238 optimal, p1/32, N<=12: three samples per variant, 60-second cap;
- hard 10 optimal, p1/32, N<=11: one sample per variant, 60-second cap.

All jobs disable hotspots. Reference commit: `109667c`. Frozen reference
`profile_case.exe` SHA-256:
`B8DD2DE7DA699E4BD75F365998AE219A687D9C188EA2849AABFCBA357B7B6F07`.
Candidate `profile_case.exe` SHA-256:
`6618B0B55768D9E275A82BA0446EA7DE7F2799F21CD26CE390899C07AC022CA1`.
Run directory: `target/parallelism-ladder/mixed-integer-substitution-20260831`.

## Results

All 14 records verify. Completed structural work and exact outputs are unchanged.
The candidate reduces median algebra time 2.43% on 115 all and 3.05% on 238 optimal.
Wall medians move -0.60% and +3.16%, respectively. See the full result (`mem:solver/experiments/38-mixed-row-integer-substitution-results`).

## Correctness and limitations

Multiplying the substituted equation by the positive denominator LCM preserves its
solution set. Final primitive normalization is unchanged. A new exhaustive test
compares 28,125 three-variable row, known-variable, and rational-value combinations
against the previous rational implementation. Existing fully-known and coverage-path
oracles remain.

Validation completed before launch:

- the focused 28,125-case mixed-row rational oracle and coverage-path oracle pass;
- 188 benchmark-feature library tests pass, with two manual tests ignored;
- the exhaustive outer Reference differential and four parallelism integrations pass;
- all workspace tests pass with five manual tests ignored;
- strict all-target solver-core Clippy and all 33 analyzer tests pass;
- source and record formatting, the release build, and a completed hotspot-off tiny
  smoke solve pass. The smoke returns one validated `Optimal(N=2,L=1)` layout.

The candidate changes only mixed rows but also makes their coverage scan complete so
the denominator is available. Its A/B measures the net effect of saved rational work
and that extra scan. The hard pair remains capped.

## Decision and next step

Promote the candidate. Exact outputs pass, both completed algebra and CPU medians
improve, and the small 115 wall regression is inside overlapping ranges. This closes
the planned substitution sequence. Return to canonicalization afterward; scheduling
stays paused.
