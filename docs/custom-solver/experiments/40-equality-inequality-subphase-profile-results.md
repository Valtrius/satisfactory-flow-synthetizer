# 40. Equality and inequality subphase results

Date: 2026-09-01. State: completed and verified.
Related: [experiment design](40-equality-inequality-subphase-profile.md).

## Verification

The frozen analyzer verifies all four records. The 115 and 238 all-layout controls
complete optimally with 49 and one exact layouts and match their references. Hard 36
and hard 10 cancel cleanly at their declared caps; hard 36 retains eight validated
layouts and hard 10 retains none. No job fails or reaches the cleanup watchdog.

Run: `target/parallelism-ladder/equality-inequality-profile-20260901`.
Source: `90df7e2`. `profile_case.exe` SHA-256:
`9A1852C6A945DE535442336E99EB89085E070EC7D0DC81EF01CC679C585C3E35`.
Verified `summary.json` SHA-256:
`A9A6775D97B97025184717B7DDB604E5831EAF6A5225C55B0B2BFFC198926426`.
Independent reanalysis produces the same summary hash.

## Measured split

Times are aggregate worker seconds from single diagnostic runs. The final column uses
combined state and SCC canonicalization as its denominator.

| Workload          | Eq index |  Eq rows |   Eq RREF | Bound pivot | Bound rows | Normalize | Sort/dedup | RREF + sort / canon |
| ----------------- | -------: | -------: | --------: | ----------: | ---------: | --------: | ---------: | ------------------: |
| 115 all, complete |  1.939 s | 14.201 s |  44.124 s |     0.424 s |   26.987 s |  12.801 s |   43.975 s |               39.7% |
| 238 all, complete |  0.432 s |  4.784 s |  14.478 s |     0.151 s |    9.509 s |   4.715 s |   17.147 s |               40.7% |
| hard 36 all, cap  |  4.572 s | 42.700 s | 109.161 s |     1.263 s |   85.381 s |  36.216 s |  143.327 s |               40.7% |
| hard 10 opt, cap  |  6.372 s | 54.918 s | 150.536 s |     1.500 s |  117.019 s |  47.639 s |  182.243 s |               38.2% |

Equality RREF consistently consumes 69.6-73.3% of equality time. Inequality
sort/dedup consumes 52.1-54.3% of inequality time, followed by row construction at
30.1-33.5% and normalization at 13.6-15.2%. Pivot indexing is below 0.5%.

The child timers reconcile within 0.37% of the equality parent and 0.34% of the
inequality parent. The distribution is therefore stable enough to select a target.
The capped rows establish cost shares only; these instrumented times are not an A/B
completion comparison.

## Decision

Test integral inequality rows end to end before changing equality RREF. The existing
normalizer already computes primitive `BigInt` coefficients, then wraps them back into
integer `Rational` values for sorting, deduplication, and sparse encoding. Keep those
coefficients as integers through all three operations and emit the same compact
integer encoding.

This is narrower than replacing the equality basis and directly addresses the largest
repeated subphase. Preserve exact row order and encoded bytes: integer `Rational`
ordering must match `BigInt` ordering, duplicate removal must be identical, and the
existing exhaustive production/reference oracle must compare both paths. Begin with
an isolated replay or calculation benchmark, then run the completed 115/238 controls
and capped hard workloads only if the calculation result is positive.

If that candidate does not remove meaningful time, split comparison count from output
deduplication before attempting a new sort scheme. Equality RREF remains the next
larger but more invasive target. Scheduling stays paused.
