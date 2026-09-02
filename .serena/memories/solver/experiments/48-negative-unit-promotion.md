# 48. Permanent negative-unit RREF shortcut

Date: 2026-09-02. User explicitly approved permanent integration and commit.
Permanently integrated, validated and committed as 6c10b35.
Commit: `perf(custom): specialize negative-unit RREF elimination`.
Evidence: `mem:solver/experiments/48-rref-negative-unit-confirmation-results`.

## Change and correctness

Seven added lines in `crates/solver-core/src/canonical.rs`.
The ordinary reducer classifies factor -1 once per eliminated row using the exact
sign and numerator/denominator magnitudes. For that factor it adds the normalized
pivot coefficient instead of multiplying and subtracting. The diagnostic twin
uses its existing negative-unit classification to take the same arithmetic path.

Preserves arbitrary-size Rational arithmetic, zero/+1/general-factor behavior,
pivot selection, canonical row order, contradiction normalization and diagnostics.
No feature flag or case/N/cyclicity/affinity/memory condition. Zero-destination
shortcut is not included. No scheduler change.

Shared matrix-oracle and heartbeat tests already landed in fadb316. No duplicated
tests or solver.rs edits. Current canonical.rs matches the frozen minus source
exactly after newline normalization. Normalized SHA256:
`12ff39f8926e06c69263576bec0fb097dca722968b786f51d3e75689ba0a7408`.

## Performance basis and limits

Two screens, six matched pairs per workload, identical frozen binaries/settings.
48 negative-unit comparison runs completed with matching exact outputs/proofs
and 15 structural counters. 19/24 completion pairs and 23/24 CPU pairs improved.
New confirmation completion medians: 115 all -1.59%, 238 optimal -2.05%,
258 minimum_links -1.96%, hard36 optimal +0.39%.

Hard36 has no demonstrated completion improvement across both screens, with a
six-pair median of -0.05% and one +4.70% adverse pair. Keep those observations.
This promotes a modest measured benefit, not a universal speedup.
No new timed benchmarks are needed for this source-identical integration.

## Integration validation

Release solver-core all-targets tests pass: 223 with bench-internals and 213 with
default features, 2 ignored per suite. Strict release Clippy passes with and without
bench-internals. No failed tests or lint warnings. Existing exact tests cover
20 signed/zero/large coefficient systems, the 384-system generated oracle,
diagnostic parity, solve scopes, enumeration, cancellation and proof contracts.
Helper/log: `target/exp48-promotion-checks.cmd`,
`target/exp48-promotion-checks.log`. Uses the main checkout's Cargo target and
existing ignored Cargo configuration with VS2019 BuildTools; no config changed.
No benchmark runs active during validation.

## Commit and handoff

Base edb4f0b includes unrelated user changes; preserve them.
Source and related confirmation/promotion documentation committed together as
`6c10b35eee8b25fa7fabd55b29d042f8e67da74b`. npm run format and staged diff checks passed.
A documentation-only follow-up records this resulting hash and clears the handoff.
No push performed.
Keep zero-destination held and scheduler extras paused. Future benchmark runs
remain <=1h including cleanup.
