# 42. Factorial results and the 258 tail

Date: 2026-09-01. State: analyzed; both changes retained as candidates, not promoted.
Design and build history: `mem:solver/experiments/42-complete-propagation-variable-set`.

## Verification

Run: `target/parallelism-ladder/registered-variable-factorial-20260901`.
Finished 23:02:48 Europe/Paris. All 36 jobs completed optimally, no caps, kills or
verification failures; 25.973 sequential process minutes.

The frozen analyzer rerun reproduces the original summary byte for byte:
`67b436d9ee7b466b1ce794ab063fa54b7959443ba7157f4d01c0de18fdb635da`.
A separate audit verifies all four executable hashes, 280 frozen source-file hashes,
the 36-entry schedule, exact requests, complete outcome/proof objects, preferred keys,
full solution objects and layout-key sets. Fifteen structural counters match within
each workload, including states, decisions, roots, contradictions and SCC work.

| Workload          | Proven optimum | Exact layouts |
| ----------------- | -------------- | ------------: |
| 115 all           | N=7, L=10      |            49 |
| 238 optimal       | N=8, L=13      |             1 |
| 258 minimum_links | N=9, L=14      |             2 |

All use max link rate 1200, N<=12, p1/32, hotspots off. No cyclicity-dependent policy.

## Completion times

Seconds. Four fresh processes per variant on 115/238, one on 258.
The reference has rational inequalities and defensive variable discovery.

| Workload                  |     Reference |  Integer rows | Complete variables |          Both |
| ------------------------- | ------------: | ------------: | -----------------: | ------------: |
| 115 all, median           |        39.162 |        38.586 |             38.712 |        38.683 |
| 115 range                 | 38.310-39.459 | 37.256-39.718 |      38.567-39.855 | 38.021-38.769 |
| 238 optimal, median       |         7.920 |         8.104 |              7.596 |         7.935 |
| 238 range                 |   7.655-8.139 |   7.840-8.218 |        7.567-7.674 |   7.809-8.031 |
| 258 minimum_links, single |       198.178 |       177.753 |            242.696 |       190.015 |

Positive percentages below mean less elapsed time/cost than the reference.

| Workload / variant |    Wall | First emitted witness | Process CPU | Canonical timer | Algebra timer |
| ------------------ | ------: | --------------------: | ----------: | --------------: | ------------: |
| 115 integer        |  +1.47% |                -1.83% |      +0.36% |          +1.98% |        -0.99% |
| 115 variables      |  +1.15% |                +2.17% |      +1.37% |          +2.07% |        +2.17% |
| 115 both           |  +1.23% |                +1.18% |      +1.60% |          +2.46% |        +1.84% |
| 238 integer        |  -2.32% |                -2.32% |      -0.82% |          +1.27% |        +3.71% |
| 238 variables      |  +4.10% |                +4.10% |      +3.66% |          +6.41% |        +7.09% |
| 238 both           |  -0.18% |                -0.18% |      +0.92% |          +2.34% |        +7.79% |
| 258 integer, n=1   | +10.31% |               +10.31% |      +4.25% |          +7.70% |        -0.61% |
| 258 variables, n=1 | -22.46% |               -22.46% |      -9.51% |          +4.37% |        +4.33% |
| 258 both, n=1      |  +4.12% |                +4.12% |      +3.31% |          +5.58% |        -0.98% |

Canonical/algebra numbers are existing aggregate elapsed timers, not additive CPU
accounting. Hotspot subphase timers are disabled. Do not subtract these timers from
process CPU to claim ownership of the remainder.

115 first-emission medians: 2.860 / 2.913 / 2.798 / 2.826 s in the table's variant
order. 238 and 258 emit their first result essentially at completion; this does not
measure the earliest internal discovery separately.

## Factorial interpretation

Complete variables improve both repeated controls in isolation. With integer rows
already enabled, they improve 238 wall 2.09% and CPU 1.72%; 115 wall changes -0.25%
and CPU improves 1.25%. These are small gains worth keeping under investigation.

Adding integer rows to complete variables changes 115 wall by +0.08%, but makes 238
4.46% slower and uses 2.85% more process CPU. The combined 238 wall is approximately
neutral against the original reference. Do not assume isolated effects add.

The 258 variable-only run is a real adverse observation, not discarded as noise:
wall is 22.46% longer and CPU 9.51% higher despite identical structural work and
lower algebra/canonical timers. Each variant has only one sample, so neither a
regression distribution nor a universal 258 speedup is established.

Experiment 41 independently showed the same wall sign pattern for integer rows:
a small 115 improvement and about 2.2% slower 238. Fresh four-sample data remains
primary because binary hashes and runner sampling changed. Exploratory pooling to
seven gives only 0.19% CPU and 0.10% canonical improvement on 238, while wall is
2.08% slower. Build equivalence for formal pooling was not established, so those
pooled figures do not decide promotion.

## 258 CPU tail

Every variant starts near 30 busy logical cores, then drops as OS thread count falls.
After 160 seconds, mean busy cores are 1.81-1.98, about 5.7-6.2% of 32.
The variable-only process spends another 83.8 sampled seconds in that band; reference
39.2, integer 18.8, combined 31.1. The thread count falls from 38 to roughly 5.

The solver's p1 root worker loop exits when the initial queue has no more tasks;
without donation, a finished worker cannot subdivide another worker's ongoing root.
This makes uneven remaining roots a plausible explanation. The samples do not tell
which function, root, allocation/teardown phase or CPU placement causes the time.

The low-CPU tail occurs before solver completion. Process return exceeds solver wall
by only 0.082-0.098 s. This excludes final result writing/process exit as the source
of the multi-minute tail, but not work or cache destruction inside the solve.
Sampled peak working sets are 1,823 / 1,966 / 1,883 / 1,892 MiB. Memory is not a
promotion gate; the user has deferred that issue.

## Decision and next experiment

Keep `cd46fae` and `520b352` active as committed candidates. Do not yet mark either
permanent or restore either based on one 258 sample. The hold is about completion
performance uncertainty, not memory usage. Scheduler options remain unchanged.

Recommended, not launched:

1. Two further hotspot-off 258 minimum_links samples per variant, eight jobs, to
   reach three completed samples each. Reuse these exact frozen binaries/settings.
2. One separate hotspot-on 258 diagnostic each for reference and complete variables.
   Capture root activity and phase costs; never mix diagnostic timing into the
   uninstrumented comparison.
3. Use those traces to identify a reproducible long root and its dominant calculation.
   Optimize or replay that calculation before reopening scheduler experiments.

No need to repeat the full 115/238 suite now. The proposed ten-job follow-up is
roughly 35-45 minutes at observed timings, not a runtime guarantee.

## Evidence and commit state

Raw results and analysis are local/ignored under the run directory.
`audit-results.py` produces `analysis/verified-report.json`.
`plot-tail.py` produces `analysis/258-cpu-tail.{png,svg}`; missing final working-set
samples after process exit are omitted, not treated as zero. CPU chart smooths five
sample intervals. Plot dependencies were installed only under ignored
`target/benchmark-analysis-python`, not into the project environment.

Prior commits: integer `cd46fae`, variables `520b352`, runner `4de03bc`, Serena
preparation `f0dca96`. Analysis changes only Serena memories and ignored artifacts.
No new solver changes, benchmark launch, promotion, commit or push in this analysis turn.

## Authorized follow-up

The user approved the next run. Preparation, scope and live-state pointers are in
`mem:solver/experiments/42-258-confirmation`; it reuses the same four frozen binaries.
The measurements and conclusions above are unchanged.
