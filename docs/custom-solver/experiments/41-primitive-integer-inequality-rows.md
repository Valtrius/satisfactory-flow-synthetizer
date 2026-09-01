# 41. Primitive integer inequality rows

Date: 2026-09-01. State: validated; whole A/B launch pending.
Related: [subphase results](40-equality-inequality-subphase-profile-results.md).

## Question and hypothesis

Experiment 40 attributes 52.1-54.3% of inequality time to sort/deduplication. The
normalizer already computes primitive `BigInt` coefficients, then converts them back
to integer `Rational` values for sorting and sparse encoding. Can the solver retain
the integers through those operations without changing canonical bytes?

## Change

The candidate returns primitive inequality rows as dense `BigInt` vectors. It sorts
and deduplicates those vectors directly, then writes the established sparse integer
tags and signed numerator bytes. Equality RREF and its rational representation are
unchanged. Physical predicates, canonical variable order, row order, cache semantics,
DFS, proof accounting, result validation, and p1 scheduling are unchanged.

The hotspot-only split follows the same integer path. Hotspot-off solves still perform
no subphase clock reads.

## Exact gate and isolated result

The generated matrix oracle compares four identities:

- current integer rows converted back to rationals;
- the previous pivot-aware rational implementation retained in tests;
- the dense exact reduction reference;
- the complete encoded inequality byte stream.

The release-only ignored calculation benchmark uses RREF-shaped systems with 18, 33,
and 42 variables. Seven alternating rounds, each containing 400 iterations over all
three systems, produce:

- rational median 0.128875 s, range 0.125130-0.158188 s;
- integer median 0.115967 s, range 0.114204-0.119642 s;
- 10.02% median improvement, with no overlap between ranges.

An independent repeat after final formatting and lint fixes improves 9.49%: rational
median 0.130612 s versus integer median 0.118221 s. Its seven ranges also do not
overlap.

This is an isolated calculation result, not a solver completion claim. It passes the
predeclared gate for a whole A/B screen.

## Frozen whole comparison

Manifest: `benchmarks/custom/primitive-integer-inequality-screening.json`.

- 115 all layouts, p1/32, three samples per variant, N<=12, 120-second cap;
- 238 optimal, p1/32, three samples per variant, N<=12, 60-second cap;
- hard 36 all and hard 10 optimal, p1/32, one sample per variant, 60-second caps;
- hotspots off for completion timing.

Reference solver source: `90df7e2`; later commit `7a50a40` changes docs only. Frozen
reference `profile_case.exe` SHA-256:
`9A1852C6A945DE535442336E99EB89085E070EC7D0DC81EF01CC679C585C3E35`.
Candidate `profile_case.exe` SHA-256:
`68C3BA8889DE7B16F9CEBDC92677F296D5CC99C2B8DFFE8036ECE9BB0B56194B`. Planned run:
`target/parallelism-ladder/primitive-integer-inequality-20260901`.

## Prelaunch validation

- 188 benchmark-feature solver-core tests pass, with three manual benchmarks ignored;
- the exhaustive outer Reference differential and four parallelism integrations pass;
- all workspace tests pass, with six manual benchmarks ignored;
- strict all-target benchmark-feature Clippy and all 33 analyzer tests pass;
- Rust, JSON, and Markdown formatting checks pass;
- the release profiler builds and the manifest validates all 16 planned jobs;
- a hotspot-off tiny smoke returns one validated `Optimal(N=2,L=1)` layout;
- a capped 115 hotspot smoke records the new integer normalization and sort paths and
  cancels cleanly.

## Decision rule

Every completed pair must preserve the full exact result and structural work. The
candidate advances if completed canonicalization or CPU medians improve consistently
without a credible whole-solve regression. Capped work can show progress per fixed
time only. Restore the rational representation if the whole screen rejects it.
Scheduling stays paused.
