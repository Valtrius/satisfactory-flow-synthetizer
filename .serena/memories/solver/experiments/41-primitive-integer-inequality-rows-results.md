# 41. Primitive integer inequality row results

Date: 2026-09-01. State: inconclusive; repeat recommended.
Related: experiment design (`mem:solver/experiments/41-primitive-integer-inequality-rows`).

## Verification

The frozen analyzer verifies all 16 records with no failures or watchdog kills. All
12 completed runs match their references exactly. The four hard runs cancel cleanly
at their caps and remain explicitly incomplete.

- Every 115 run returns `Optimal(N=7,L=10)` with the same 49 layouts.
- Every 238 run returns `Optimal(N=8,L=13)` with the same single layout.
- Both hard-36 runs retain eight validated layouts; both hard-10 runs retain none.

Run: `target/parallelism-ladder/primitive-integer-inequality-20260901`.
Candidate source: `cd46fae`. Candidate/reference executable hashes match the design.
Verified `summary.json` SHA-256:
`94E9B559BA629CD7D30C167D481AA77BC0981A9E7D32204793931CEA10BF2EB7`.
Independent reanalysis produces the same hash.

## Completed timings

Each variant has three fresh-process hotspot-off samples. Positive percentages favor
the integer candidate. Canonicalization is the existing aggregate Custom diagnostic.

| Workload    | Before wall median (range) | After wall median (range) |   Wall | First witness |    CPU | Canonicalization |
| ----------- | -------------------------: | ------------------------: | -----: | ------------: | -----: | ---------------: |
| 115 all     |   40.219 s (36.746-41.471) |  39.737 s (37.972-43.950) | +1.20% |        -3.22% | +0.29% |           +0.54% |
| 238 optimal |      7.872 s (7.843-8.569) |     8.045 s (7.721-8.227) | -2.20% |        -2.20% | -1.28% |           -2.17% |

Structural work is identical within each completed workload: 115 records 1,722,102
states and 4,114,026 decisions; 238 records 173,603 states and 627,595 decisions. Both
wall ranges overlap substantially. The isolated calculation remains a repeatable
9.49-10.02% improvement, but its whole-solve contribution is smaller than run noise.

## Capped work

Hard 36 processes 3.65% fewer states and 3.57% fewer decisions with the candidate;
its first witness is 3.13% later. Hard 10 processes 11.66% more states and 11.71% more
decisions with the candidate and still finds no witness. These opposing single-pair
movements cannot establish completion speed or a general hard-case gain.

## Decision

Do not call the candidate permanent yet. It preserves exact bytes, passes every
correctness check, and wins two isolated measurements, but the predeclared whole gate
requires consistent completed CPU or canonicalization gains. Both improve slightly on
115 and regress on 238.

Keep `cd46fae` as the active candidate and run four additional samples per variant on
the completed 115 all and 238 optimal workloads only. Combining old and new records
gives seven samples per variant without spending more time on capped work. Promote if
the combined CPU or canonicalization medians favor the candidate on both workloads
without a credible wall regression; otherwise restore the rational representation.
Scheduling stays paused.
