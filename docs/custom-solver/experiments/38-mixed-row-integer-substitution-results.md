# 38. Mixed-row integer substitution results

Date: 2026-09-01. State: promoted.
Related: [experiment design](38-mixed-row-integer-substitution.md).

## Verification

The frozen analyzer verifies all 14 records with no failures or watchdog kills.
Twelve runs complete optimally and preserve their exact outputs. The hard pair
cancels cleanly at the shared cap. Both executable hashes match the prelaunch record.

- Every 115 run returns `Optimal(N=7,L=10)` with the same 49 layouts.
- Every 238 run returns `Optimal(N=8,L=13)` with the same single layout.
- Both hard runs exhaust through N=10, close the same 13 profiles and root partitions,
  reach N=11/L=18, and retain no witness.

Run: `target/parallelism-ladder/mixed-integer-substitution-20260831`.
Verified summary SHA-256:
`B22803509404B85810F5AAB56AFB7F884CD6B40AF70384FCB39CA4A5A725AAD8`.
The 14 processes record 6.674 minutes of solver wall time.

## Completed timings

All rows use three hotspot-off fresh-process samples per variant. Positive percentages
favor the integer candidate.

| Workload    |    Before median (range) |     After median (range) | Wall improvement | First witness | CPU improvement | Algebra improvement |
| ----------- | -----------------------: | -----------------------: | ---------------: | ------------: | --------------: | ------------------: |
| 115 all     | 38.294 s (38.201-39.275) | 38.525 s (37.755-39.181) |           -0.60% |         0.91% |           1.15% |               2.43% |
| 238 optimal |    7.763 s (6.739-7.896) |    7.518 s (7.365-7.642) |            3.16% |         3.16% |           0.16% |               3.05% |

Structural work is identical within each completed workload: 115 records 1,722,102
states and 4,114,026 decisions; 238 records 173,603 states and 627,595 decisions.
The established sparse algebra timer favors the candidate in both workloads. The 115
wall regression is 0.231 seconds and sits inside heavily overlapping ranges. Sampled
peak process memory falls 0.80% on 115 and rises 1.87% on 238. Exact cache size and
cache-byte diagnostics are unchanged.

## Capped hard work

The hard candidate processes 1,426,919 states and 3,563,961 decisions versus the
reference's 1,484,357 states and 3,712,239 decisions. This is 3.87% and 3.99% less
work at the same 60-second cap. Process CPU falls 3.61%, but CPU per state rises 0.27%.
Algebra time per state improves 3.33% while canonicalization time per state varies in
the opposite direction.

This single capped pair cannot establish time to a witness or completion. It confirms
only the same proof obligation and no result-set difference.

## Decision

Promote the mixed-row integer path. The exact rational oracle and all solver tests
pass, completed structural work is unchanged, and both completed workloads reduce the
targeted algebra cost. Whole-solve evidence is weaker than Experiments 36 and 37: one
wall median improves 3.16% and the other regresses 0.60%, while both CPU medians favor
the candidate. The small 115 wall movement does not justify restoring repeated
`BigRational` normalization.

This promotion closes the sparse-substitution sequence. Return to canonicalization
profiling before changing DFS behavior. Scheduling stays paused.
