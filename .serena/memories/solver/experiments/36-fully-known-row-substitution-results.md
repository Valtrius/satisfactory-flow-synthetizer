# 36. Fully known row substitution results

Date: 2026-08-31. State: promoted. Related: experiment design (`mem:solver/experiments/36-fully-known-row-substitution`).

## Verification

The frozen analyzer verifies all 14 records with no failures or watchdog kills.
Twelve runs complete optimally; the hard-10 pair cancels cleanly at the shared cap.
The candidate and reference binary hashes match the prelaunch record.

- Every 115 run returns `Optimal(N=7,L=10)` with the same 49 exact layouts.
- Every 238 run returns `Optimal(N=8,L=13)` with the same single layout.
- Both hard-10 runs exhaust through N=10, close the same 13 profiles and root
  partitions, reach N=11/L=18, and retain no witness.

Run: `target/parallelism-ladder/fully-known-substitution-20260831`.
Verified summary SHA-256:
`A22884EF6F706C47F76CFC2E0494D98CAB46181BFDFDF6F4A7130E8B2111EAAA`.

## Completed timings

All rows below are three-sample medians from hotspot-off fresh processes.

| Workload    |    Before median (range) |     After median (range) | Improvement | First witness | CPU improvement |
| ----------- | -----------------------: | -----------------------: | ----------: | ------------: | --------------: |
| 115 all     | 41.569 s (38.623-43.963) | 37.526 s (37.409-38.367) |       9.73% |        10.59% |           8.51% |
| 238 optimal |    8.685 s (8.118-8.967) |    7.260 s (7.243-7.665) |      16.41% |        16.41% |          14.39% |

Structural work is identical within each completed workload: 115 records 1,722,102
states and 4,114,026 decisions; 238 records 173,603 states and 627,595 decisions.
The wall and CPU reductions therefore measure cheaper equivalent calculations rather
than changed search order or coverage.

## Capped hard work

The hard-10 candidate processes 1,460,769 states and 3,645,686 decisions versus the
reference's 1,370,887 states and 3,424,482 decisions. That is 6.56% more states and
6.46% more decisions within the common cap while process CPU falls 1.12%. Sampled
peak working set rises 6.99%, from 2.87 to 3.07 GiB.

This pair supports higher throughput but does not establish time to first witness or
completion. Both runs remain explicitly incomplete.

## Decision

Promote the fully-known integer path. It preserves exact canonical row outcomes,
improves both completed workloads beyond noise, reduces CPU, and advances more capped
hard work. Mixed-row substitution remains unchanged.

The next isolated calculation candidate is the no-known-variable path: clone the
already primitive row instead of rebuilding and normalizing it. Measure it separately
against this promoted baseline. General integer substitution for mixed rows remains a
later experiment. Canonicalization is still the larger hard-run bucket, and scheduler
work stays paused.
