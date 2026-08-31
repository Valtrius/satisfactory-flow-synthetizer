# 37. No-known row substitution results

Date: 2026-08-31. State: promoted. Related: [experiment design](37-no-known-row-substitution.md).

## Verification

The frozen analyzer verifies all 14 records with no failures or watchdog kills.
Twelve runs complete optimally and preserve their exact outputs. The hard pair
cancels cleanly at the shared cap. Both executable hashes match the prelaunch record.

- Every 115 run returns `Optimal(N=7,L=10)` with the same 49 layouts.
- Every 238 run returns `Optimal(N=8,L=13)` with the same single layout.
- Both hard runs exhaust through N=10, close the same 13 profiles and root partitions,
  reach N=11/L=18, and retain no witness.

Run: `target/parallelism-ladder/no-known-substitution-20260831`.
Verified summary SHA-256:
`385B69E4CA31D51419E06B5F1642140EEAD00E841866452BE41F1756E43009DC`.

## Completed timings

All rows use three hotspot-off fresh-process samples per variant.

| Workload    |    Before median (range) |     After median (range) | Improvement | First witness | CPU improvement |
| ----------- | -----------------------: | -----------------------: | ----------: | ------------: | --------------: |
| 115 all     | 38.299 s (34.756-40.492) | 36.242 s (35.507-38.182) |       5.37% |         1.41% |           1.80% |
| 238 optimal |    7.580 s (7.498-8.182) |    7.321 s (7.137-8.380) |       3.42% |         3.42% |           3.58% |

Structural work is identical within each completed workload: 115 records 1,722,102
states and 4,114,026 decisions; 238 records 173,603 states and 627,595 decisions.
The wall ranges overlap, but both CPU medians and both wall medians favor the candidate.

## Capped hard work

The hard candidate processes 1,506,156 states and 3,764,049 decisions versus the
reference's 1,543,147 states and 3,854,935 decisions: 2.40% and 2.36% less work.
Process CPU falls 2.02%, but CPU per state is 0.38% higher. Both runs reach the same
proof obligation.

This single capped pair conflicts with the completed result and is retained as a
limitation. It cannot establish time to first witness or completion, and it does not
override the predefined promotion rule based on repeated completed medians.

## Decision

Promote the no-known clone path. It is an exact identity operation, both completed
workloads improve in repeated wall and CPU medians, and their structural work is
unchanged. Retain the capped hard conflict when judging combined future changes.

The only remaining substitution case is a mixed row. Test one integer denominator
LCM and residual construction against the established rational path as a separate
candidate. Canonicalization remains the larger hard-run bucket, and scheduler work
stays paused.
