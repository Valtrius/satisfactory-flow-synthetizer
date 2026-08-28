# 05. Refinement removal and constructor reuse

Date: 2026-08-28. State: isolated refinement accepted; constructor changes remain candidates.
Hypotheses: remove work used only to order exhaustive witness permutations; avoid
optional construction that runtime facts already rule out or a previous L group completed.

## Implementation

- Full-witness labeling retains every permutation, ranks, exact leaf encoding,
  minimum comparison and cancellation checks. Removed color refinement/order-only
  helpers. Partial-state Canonaut labeling and public witness protocol stay unchanged.
- Constructor eligibility uses exact source-denominator divisibility and existing
  fixed-profile impossibility checks. No benchmark label or externally known cyclicity.
- Completed helper hits/misses are reused within the current N of one solve.
  Changing N clears them; cancelled attempts are not retained. Setup is lazy for
  small solves. Reuse never discharges exhaustive-search proof obligations.
- Diagnostics separate root search, finalization, state/SCC cache destruction,
  propagation/topology release and slow/cancelled partial-labeling calls.

## Isolated witness result

Two alternating before/after pairs from the same saved validated witness:

| Replay |    Before s |    After s |
| ------ | ----------: | ---------: |
| 1      |  39.7574213 |  6.0257465 |
| 2      |  39.7861714 |  6.1356622 |
| Mean   | 39.77179635 | 6.08070435 |

6.54x replay speedup, 84.7% less elapsed time. All four reproduced the exact key,
7,671,847 branches and 1,327,104 leaves. The cancellation replay returned no key
at about 211 ms. This is isolated canonicalization, not a whole-solve speedup.

## Failed first solver screen

Manifest `benchmarks/custom/recommendation-screening.json` planned 12 jobs:
eight complete smaller-case gates, hard 36 optimal at 360 s, two 10 optimal N<=11
runs at 180 s, and 36 all at 600 s. All used 32 workers; diagnostics were enabled.
The first solver job was 10 optimal/p1. It cancelled at 180 s, then failed to
return during an additional 180 s grace and was killed. The old runner stopped
there, leaving no final solver JSON and no completion timing for that job.

The deadline log reported 2,835,615 graph canonicalizations and about 1,252.1
summed state-canonicalization seconds. These are not CPU seconds. The helper
eligibility change allowed real search, but the log did not identify the blocked
cancellation operation. Later unstarted jobs were not failures or successes.

## Decision, delivery and evidence

Make only the witness-refinement removal permanent. Commit `3e804e8` changes
`canonical.rs` and its active-cancellation regression test, excluding compact keys,
constructor work, diagnostics and scheduling. The exact isolated source passed
workspace tests, strict Clippy and formatting in a separate checkout. The user
subsequently pushed it; remote `develop` was confirmed at that commit.

The isolated checkout shared the main target directory during validation. Later
Clippy saw stale metadata; a solver-package clean and full current rebuild fixed
it. Use separate targets for different working trees in future.

Local evidence: `target/parallelism-ladder/recommendations-20260828/`, particularly
`replay-summary.json`, four full replay JSON files, cancellation replay, hashes,
and the failed first-job logs. Independent commit validation logs are under
`target/isolated-witness-*.log`. Constructor whole-solve attribution remains combined.
Follow-up: [06, failure-preserving recovery diagnostics](06-cancellation-recovery.md).
