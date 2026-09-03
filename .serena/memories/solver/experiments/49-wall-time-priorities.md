# 49. Wall-time priorities and redundant inequality key payload

Date: 2026-09-02. Updated 2026-09-03: completed and retained; results in `mem:solver/experiments/49-derived-key-results`. Remaining approved stages continue in `mem:solver/experiments/50-checked-rref`. Historical preparation/launch entries below are preserved.
Related: `mem:solver/experiments/39-canonicalization-reprofile-results`, `mem:solver/experiments/47-rref-arithmetic-profile-results`, `mem:solver/experiments/48-negative-unit-promotion`.

## Question and hypothesis

User first requested recommendations, then explicitly authorized applying all recommendations. Implement and assess them in order, with separate candidates and no further permission requests. Benchmarks require an end to the turn after launch per AGENTS.md.

First recommendation: remove the derived inequality payload from internal StateKey/SccSummaryKey generation. Current canonical.rs::primitive_inequality_basis takes only capacity, variable_count and the equality basis. encode_partial_state already serializes capacity and physical topology; encode_semantic_system serializes variable_count and the complete equality basis before constructing the inequalities. Thus the current key is K=(P,E,F(P,E)); the proposed key is (P,E), retaining all SCC annotations. Under the present uniform positivity/capacity rules this preserves the key equality relation. This is a source-based mathematical argument, not a tested change or measured speedup.

Canonical labeling happens in select_canonical_with_canonaut before serialization; the proposed deletion need not alter ranks or open-port coordinates. SCC's length-delimited embedded state will change length. Internal protocol versions and key-dependent fixtures/certificates need deliberate updates. Prove old-key equality iff new-key equality, rather than requiring impossible byte identity across protocol versions. Public witness/layout identities must remain unchanged.

## Source inspection

Inspected clean HEAD f89b6d95b54320e4bd8dd691212495649e2297be. Current negative-unit RREF and reverse ordering are present.

- canonical.rs:1986 encode_partial_state includes exact capacity.
- canonical.rs:2084 encode_semantic_system serializes E then deterministic bounds.
- canonical.rs:2303 ordinary inequality builder; profiled twin at 2343.
- canonical.rs:2435 rational_rref retains zero/+1 skips, negative-unit addition, reverse order.
- canonical.rs:725 select_canonical_with_canonaut chooses labeling before encoding.
- search.rs:1348 deferred cache is disabled whenever sharing OR donation is present.
- search.rs:1635 child preparation orders propagation, dynamic SCC, then topology-only reachability.
- lower_bound.rs:151 profile_impossibility already includes branch/merge, pure-profile arithmetic and splitter determinant magnitude bounds.

## Ranked follow-ups

1. Isolate redundant inequality-key removal. Keep actual propagation, positivity/capacity feasibility, SCC analysis, equality basis, cache and proof accounting.
2. Reprofile current code after that candidate. Consider immutable per-problem/profile canonical input templates and scratch reuse only if preparation remains material.
3. Larger mathematical candidate: canonical RREF with cleared denominators/fraction-free elimination and final exact normalization. Alternatively test a local checked-small-integer kernel with BigInt fallback; do not combine both. Observed operand sizes are not overflow guarantees. Existing sparse propagation Bareiss and canonical RREF are separate computations.
4. Measure pruning order: topology-only reachability currently follows SCC work. Moving it earlier can save algebra on structural rejections but adds work on branches propagation would already reject. Require conditional rejection/work counters before claiming benefit.
5. Resume scheduler experiments only after a current tail profile and explicit follow-up scope. Existing sharing/donation disable the successful deferred-cache path, so flag toggles conflate load balancing with added canonicalization. Preserve proof-ledger joins and completed-state-only sharing.

Do not revisit whole state-cache removal, sparse duplicate-row removal, restored integer inequality rows, or held zero-destination arithmetic without new evidence. Do not propose determinant/denominator bounds or +1/zero skips as new.

## Evidence and limitations

Historical Serena records, not newly rerun raw benchmarks: experiment39 inequality construction was the largest individual canonicalization phase. Experiment47 RREF elimination was 66-69% of the instrumented RREF accumulator, with observed numerator/denominator maxima only 11-16 / 6-7 bits. These are nested diagnostic timings and sampled operands, not present-day wall shares or fixed-width bounds.
Current code verifies the mechanism remains; no present-day profile, test or speedup claim.

External primary reference: https://docs.sympy.org/latest/modules/polys/domainmatrix.html documents exact fraction-free Gauss-Jordan and denominator clearing, with different performance tradeoffs by sparsity/denominator shape. This is algorithm support, not evidence of speedup here.

## Validation proposed

For key removal: old/new cache equivalence classes across relabeling, arbitrary rational rates/capacity, contradictions, rank deficiency, derived known flows, and SCC known-value/region permutations. Keep exact full public solution objects, canonical layout-key sets, preferred witnesses, objective, proof and cancellation contracts. Key protocol/certificate bytes may change; distinguish deliberate protocol changes from coverage changes.
Then fresh hotspot-OFF completed paired runs for 115 all, 238 optimal, 258 minimum_links and 36 optimal, with tiny overhead control. Match worker counts and verified placement, balance A/B and B/A, at least five initial pairs for small-gain decisions, split across sessions to keep each <=1h including cleanup. Hard10 caps are diagnostic only. Record first witness, proof completion, all-enumeration completion, process wall and cleanup separately. Launch with completion signal, then end turn.

## Decision and state

The user authorized all staged recommendations. Derived-key removal is now implemented in the main working tree and a detached candidate build. No promotion decision or speedup yet. Remaining approved work follows the first measurement/reprofile, then separate arithmetic/pruning/scheduler candidates. Keep existing rejected/held candidates unchanged.

## Implementation preparation

Fresh reference built at f89b6d9 with bench-internals and the existing ignored Cargo config; profile_case SHA256 98c8361a28e7d391dbb3c2804ae108ee8e14b63fa987aeb5268501da01761c4a.
Frozen root target/parallelism-ladder/derived-key-variants-20260902/before contains 70 source files, three executables and provenance.
Candidate code is in main plus detached C:/Users/jakez/.codex/tmp/sf49-derived, using its separate sf49-derived-build target. Tests/builds use VS2019 and the existing config via --config, not a copied config.

The internal sparse-exact-rows protocol advances to version 2. Shared encode_partial_state outer bytes remain unchanged because Layout uses that encoding too. Old inequality builders are test-only oracles; production keeps zero-valued historical inequality snapshot fields for format continuity. New generated key-bijection tests cover 300 state inputs and 1200 SCC inputs, plus canonical relabeling and large rational capacities. Initial key oracle passed. Full validation in progress; initial dead-code timer warnings are being corrected before final strict Clippy.

A first draft mixed paired timings and unpaired diagnostics, which the existing runner rejects. Caught by reading policy before launch; replaced with 48 paired records in benchmarks/custom/derived-key-screen.json. 40 timing records and eight separately labeled hotspot diagnostics. All timing and 115/238 diagnostic controls must finish; four hard diagnostic records may cap. 2500s search + 720s cleanup at 15s/job = 53m40 plus overhead, within one hour. Five pairs per 115/238/36 timing cell, two 258 pairs, three tiny pairs. More long258 repeats remain possible after this initial screen if effects are small. No launch yet.

41 Python tooling tests pass. No commit or push. No performance result inferred from builds or tests.

## Exact validation before freeze

Final production source passes 224 release solver-core all-target tests with bench-internals and 214 default, two ignored in each. This includes the exhaustive outer Reference comparison, parallelism integration, cancellation/proof tests and the new 300-state/1200-SCC legacy-key bijection. Logs: target/exp49-candidate-final-checks.log.
Strict Clippy first found retired inequality timer variants, then a needless pass-by-value in the test helper. Both were fixed without suppressions. The corrected helper's targeted oracle passes; strict release all-target Clippy passes both configurations. Log: target/exp49-finalize.log. No remaining lint warning. Release example build is pending at this entry.

npm run format passed; its incidental release-version.mjs formatting was restored to preserve unrelated code. 41 Python tool tests pass; manifest policy validates all 24 pairs. Live Windows topology confirms 96MiB mask ffff and 32MiB mask ffff0000. No production affinity policy change.

Post-run audit helper: target/exp49-audit.py RUN_DIRECTORY. It verifies all frozen hashes, complete public results/outcomes/proofs, before-resume placement, zero removed counters, and reports structural differences separately. A change in adaptive planning must be inspected rather than misreported as fixed structural work. The original analyzer remains unchanged.

## Ready to launch

Release build completed successfully. Candidate profile_case SHA256 16737f36ff769659ed2766b3d9788f9bc9e203d5680feda2d9b9a8d2c4a799e3; before 98c8361a28e7d391dbb3c2804ae108ee8e14b63fa987aeb5268501da01761c4a. Both use bench-internals, the same rustc/config and separate targets. Candidate source has 71 frozen files. Normalized differences are only canonical.rs, hotspot_profile.rs and new canonical/derived_bounds_tests.rs; current main matches frozen candidate.

The first tiny CLI smoke failed its coverage assertion because the constructor solved optimal without calling canonical RREF/bounds. This was a diagnostic-fixture error, not an output mismatch. Preserve target/exp49-release-smoke. The corrected cyclic65 smoke, target/exp49-release-smoke-v2, passes all 12 actual CLI combinations: three scopes x before/after x hotspots off/on. Full public objects/outcomes/proofs agree; before/on exercises bounds; after retains zero bound counters. Log target/exp49-release-smoke-v2.log. No smoke timing interpreted as performance.

PlanOnly succeeds in target/parallelism-ladder/derived-key-plan-20260902. target/exp49-verify-preparation.py verifies 347 variant/plan hashes, same build settings, only intended source changes, all 24 adjacent pairs, balanced-as-possible order and 3220-second cap-plus-cleanup budget. Manifest benchmarks/custom/derived-key-screen.json; mapping target/parallelism-ladder/derived-key-variants-20260902/variants.json; intended run target/parallelism-ladder/derived-key-screen-20260902. Launcher supplies completion/failure dialog and durable markers.

No new commit or push. Candidate is active in working source and uncommitted. Performance pending; no blanket promotion of the remaining arithmetic/pruning/scheduler work. All subsequent staged work remains authorized.

## Launched, awaiting analysis

Launched 2026-09-02 Europe/Paris, runner PID 33320, target/parallelism-ladder/derived-key-screen-20260902. Startup verified active with durable status and completion/failure dialog. No result inferred. Main candidate, manifest and Serena handoff remain uncommitted; no push. End turn now per AGENTS.md, no builds/tests during the screen. Remaining approved recommendations continue after result analysis.
