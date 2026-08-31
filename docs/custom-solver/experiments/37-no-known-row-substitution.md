# 37. No-known row substitution

Date: 2026-08-31. State: completed and promoted; see [results](37-no-known-row-substitution-results.md).
Related: [fully-known promotion](36-fully-known-row-substitution-results.md).

## Question and hypothesis

Experiment 36 makes fully known rows cheaper and improves completed medians
9.73-16.41%. For a row containing no known variables, substitution cannot alter the
already primitive equation. Does returning its clone outperform rebuilding the map,
constructing a rational RHS, and repeating gcd/sign normalization?

## Change and comparison

One coverage scan classifies each row as no known coefficients, all known, or mixed.
No-known rows return `self.clone()`. Fully known rows retain experiment 36's integer
evaluation, including its single denominator LCM. Mixed rows retain the established
`BigRational` substitution.

The coverage scan stops at the first unknown after a known coefficient. If unknowns
come first, it searches only until it finds a later known coefficient or proves that
the row has no known values. This avoids adding a complete extra scan to the common
fully-known path.

## Frozen comparison

Manifest: `benchmarks/custom/no-known-substitution-screening.json`. It repeats the
experiment-36 workload exactly:

- 115 all-L enumeration, p1/32, N<=12: three samples per variant, 120-second cap;
- 238 optimal, p1/32, N<=12: three samples per variant, 60-second cap;
- hard 10 optimal, p1/32, N<=11: one sample per variant, 60-second cap.

All 14 jobs disable hotspot recording. Reference commit: `532aaa1`. Frozen reference
`profile_case.exe` SHA-256:
`70D2819AEB676133060FAAEEA86D8FBC6051735F92047973CBFEB60ADBE003EE`.
Candidate `profile_case.exe` SHA-256:
`B8DD2DE7DA699E4BD75F365998AE219A687D9C188EA2849AABFCBA357B7B6F07`.
Run directory: `target/parallelism-ladder/no-known-substitution-20260831`.
The plan-only runner accepted all 14 jobs. The detached runner freezes both binaries,
solver source, manifest, cases, and analyzer and provides a completion notification.

## Results

[All 14 records verify](37-no-known-row-substitution-results.md). Completed medians
improve 5.37% on 115 all and 3.42% on 238 optimal with identical exact structural
work. The single hard-cap candidate processes 2.40% fewer states, a conflicting
diagnostic that cannot establish completion speed. The candidate is promoted under
the predefined repeated-completion rule.

## Correctness and limitations

Sparse rows are normalized on construction and immutable outside this module. If no
coefficient has a known value, substitution is the identity and cloning preserves the
exact canonical representation. Coverage-path tests compare no-known, mixed in both
variable orders, and fully-known results against the previous rational oracle.

This experiment does not change mixed-row arithmetic. The hard pair remains capped
and cannot establish completion speed.

Validation completed before launch:

- focused coverage and 28,125-case fully-known rational-oracle tests pass;
- 187 benchmark-feature library tests pass, with two manual tests ignored;
- the exhaustive outer Reference differential and four parallelism integrations pass;
- all workspace tests pass with five manual tests ignored;
- strict all-target solver-core Clippy and all 33 analyzer tests pass;
- source and record formatting, the release build, and a completed hotspot-off tiny
  smoke solve pass. The smoke returns one validated `Optimal(N=2,L=1)` layout.

## Decision and next step

The promotion gate passes. Keep the clone path and test mixed-row integer substitution
separately. Scheduling stays paused.
