# 20 - Constructor and medium-case results

Date: 2026-08-29. State: verified and analyzed.
Protocol: [experiment 20](20-constructor-and-medium-cases.md).

## Integrity

The final marker reports success. The frozen schedule contains 26 jobs and
`results.csv` contains 26 records; all processes completed with an exact optimum.
The verifier reports no failures. A fresh run of the frozen analyzer reproduced
`summary.json` exactly, including SHA-256
`32e6aed9cd0acce629127f544a9b25f8afc4089406b52f0b0571b8863f078a8a`.

Reference and candidate hashes match the prelaunch record. Every completed pair has
the same exact status, preferred witness, canonical layout-key set, full saved
solutions and proof data. Layout counts are 6 for 24, 13 for 65, 1 for 238 and 49
for 115.

Post-analysis validation reran the solver-core package suite, strict all-target
Clippy, the focused guard test and all 14 search-stage frontend tests successfully.

## Ordinary timings

All rows use p1, 32 workers and capacity 1200. Ranges are min-max. Optimal rows are
single samples and the guard does not change their code path.

| Case / mode      | Reference median            | Guard median                | Change |
| ---------------- | --------------------------- | --------------------------- | ------ |
| 24 all, n=3      | 5.162 s (5.104-5.217)       | 5.033 s (5.031-5.115)       | -2.51% |
| 65 all, n=3      | 5.933 s (5.899-5.949)       | 5.947 s (5.929-5.948)       | +0.24% |
| 238 all, n=2     | 101.059 s (100.814-101.303) | 110.513 s (110.022-111.004) | +9.36% |
| 115 all, n=2     | 160.802 s (160.672-160.932) | 159.587 s (159.550-159.624) | -0.76% |
| 238 optimal, n=1 | 73.350 s                    | 69.507 s                    | -5.24% |
| 115 optimal, n=1 | 11.125 s                    | 10.991 s                    | -1.20% |

The 238 difference is ordinary search variance: this cyclic case performs only
cheap constructor-ineligibility checks, and the structural work counters are
identical. The one-sample optimal differences also cannot come from the guard.
This screen therefore establishes exact neutrality and a small 24-all saving, not
a general completion-speed improvement. Experiment 16 independently measured
26.366 seconds of optional constructor work after the winning hard-36 group; the
guard removes that class of redundant work.

## Low-CPU intervals

The diagnostic aliases completed in 114.730 seconds for 238 and 164.203 seconds for 115. Their process samples and exact activity spans show search-tail imbalance:
CPU use tracks the number of unfinished root partitions.

- 238 begins near 32 active roots. It falls to about six around 20-25 seconds and
  one around 30 seconds. One root then runs alone until about 83 seconds. A second
  group restores parallel work before another shorter tail. Its single witness
  canonicalization occupies 7.95-65.06 seconds inside that long root; the same root
  continues alone for about 18 seconds afterward.
- 115 has repeated tails: roughly 35-45, 100-120 and 145-164 seconds. The last
  interval has one unfinished root. Witness canonicalizations overlap some busy and
  tail intervals but do not explain the overall shape.

Average utilization was 24.95% of 32 workers for 238 and 45.96% for 115. These are
instrumented diagnostic samples, not timing comparisons. No `acyclic_construct`
span appears in either trace because their eligible profiles are cyclic; the helper
only records fast `acyclic_ineligible` checks.

## Decision and next work

Make the post-winning guard permanent. The proof state already establishes that an
incumbent exists at the current N, so rerunning the optional constructor cannot
improve exact completeness or optimality. Preserve the explicit constructor phase
so future serial helper work is visible.

Keep 238 and 115 as medium completed controls. Pause scheduler policy changes as
agreed. The next calculation work should profile the slowest individual root spans
in these cases, especially the 238 root that remains after its witness is
canonicalized. That distinguishes expensive state canonicalization, algebra,
propagation and leaf validation before considering finer root splitting or donation.
