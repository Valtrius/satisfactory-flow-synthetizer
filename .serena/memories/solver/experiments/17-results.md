# 17 results - P1 promotion and focused follow-ups

Date: 2026-08-29. Protocol and predeclared gates (`mem:solver/experiments/17-p1-promotion-and-followups`).
The corrected run finished 180/180 records in 38.135 process minutes. Frozen
verification passes 20 fixed and 160 whole jobs. No watchdog kills, process failures
or exact-result failures. Full saved solution objects agree wherever keys overlap.

The first launch completed its fixed phase, then failed because the manifest copied
the candidate under `candidate` while the sequential runner expected `basis` for
whole solves. That evidence remains at `p1-promotion-20260829/`. The alias was
corrected and a fresh full run used `p1-promotion-20260829-retry1/`.

## P1 against current baseline

Candidate binary, three fresh completed processes per cell except capped hard 10.
Median solver wall seconds and p1 change from baseline:

| Work            |     Workers |              Baseline |                    P1 |                 Change |
| --------------- | ----------: | --------------------: | --------------------: | ---------------------: |
| 24 all          |           4 |                19.361 |                18.389 |                  -5.0% |
| 24 all          |          16 |                19.033 |                 5.933 |                 -68.8% |
| 24 all          |          32 |                18.377 |                 4.986 |                 -72.9% |
| 65 all          |           4 |                18.402 |                18.409 |                 +0.04% |
| 65 all          |          16 |                18.405 |                 8.211 |                 -55.4% |
| 65 all          |          32 |                16.694 |                 5.991 |                 -64.1% |
| 24 optimal      | 4 / 16 / 32 | 1.357 / 1.397 / 1.392 | 1.021 / 0.687 / 0.694 | -24.8 / -50.8 / -50.1% |
| 65 optimal      | 4 / 16 / 32 | 1.088 / 1.089 / 1.083 | 0.776 / 0.417 / 0.378 | -28.7 / -61.7 / -65.1% |
| Hard 36 optimal |          16 |                83.677 |                30.022 |                 -64.1% |
| Hard 36 optimal |          32 |                90.343 |                24.766 |                 -72.6% |

One-worker optimal changes are -0.9% for 24 and -3.7% for 65. P1 tiny medians
remain below the 20 ms gate: all is 10.1-11.7 ms and optimal 1.9-2.6 ms across
1/4/16/32 workers. Relative tiny overhead reaches 96%, but absolute latency passes
the recorded gate. No completed 24/65 cell regresses by both 5% and 0.1 s.

All completed results retain exact status, keys, preferred witness, saved solutions
and proof consistency. P1 wins every measured N<=9 completion at 16/32 workers.
At four workers it improves optimal 25-29%, improves 24 all 5%, and leaves 65 all flat.

## Hard-10 memory gate

Single 45 s capped processes; all remain at N=11/L=18 with no witness and the same
completed proof summary. The values are useful resource observations, not completion
ratios.

| Mode / workers | Baseline CPU s / peak MiB | P1 CPU s / peak MiB | Peak ratio |
| -------------- | ------------------------: | ------------------: | ---------: |
| Optimal / 16   |             134.8 / 243.8 |       708.6 / 692.3 |      2.84x |
| Optimal / 32   |             134.3 / 182.2 |   1,275.1 / 1,105.7 |      6.07x |
| All / 16       |             134.9 / 184.2 |       715.7 / 756.0 |      4.10x |
| All / 32       |             134.6 / 179.7 |   1,329.4 / 1,135.8 |      6.32x |

Unconditional p1 fails the predeclared 2x hard-memory gate. The initial recommendation
was therefore to use adaptive partitions through current N=9 and baseline partitioning
from N=10. That guarded implementation passed validation but was not committed.

The user later chose completion performance over the measured memory cost and accepted
the hard-10 memory increase as deferred work. The final decision is to promote
unconditional p1 for both modes. Sharing, donation and parallel remaining groups stay
off. Experiment 18 (`mem:solver/experiments/18-guarded-p1-promotion`) records the rejected guard, and
experiment 19 (`mem:solver/experiments/19-unconditional-p1-promotion`) records the permanent policy.

## Post-winning constructor guard

Reference and candidate differ in production source only by skipping the optional
constructor after `winning_node` is set. Two samples per cell, p1/32:

| Mode / variant      |    Wall range s | First witness range s | CPU range s | Layouts |
| ------------------- | --------------: | --------------------: | ----------: | ------: |
| Optimal / reference |   24.840-24.909 |         24.839-24.908 | 354.4-361.6 |       1 |
| Optimal / candidate |   23.653-24.345 |         23.653-24.344 | 351.2-356.3 |       1 |
| All / reference     | 120.014-120.029 |         23.891-25.203 | 1,659-1,680 |       3 |
| All / candidate     | 120.085-120.095 |         24.495-24.713 | 2,282-2,308 |       8 |

Optimal outcome and proof agree, as expected because that path is unchanged.
At the all-mode cap, reference repeats stop in L=12 with 20 groups/45 profiles/
2,046 roots folded. Candidate repeats finish L=12 and reach L=13 with 21 groups/
50 profiles/2,437 roots and eight validated partial solutions. This confirms that
the removed helper work advances enumeration, but both runs are capped and perform
different work. Keep the candidate uncommitted until a completed all-mode A/B
establishes completion time and the full exact set.

## Sharing, donation and prefixes

The fixed comparison selected one complete UNSAT N=9/L=12 profile, not the complete
five-profile group. Two samples per stage/mode:

| Stage | Best median s | All median s | Maximum peak MiB |
| ----- | ------------: | -----------: | ---------------: |
| p1    |         3.965 |        3.977 |      71.6 / 68.0 |
| p12   |         4.114 |        4.045 |    114.8 / 113.4 |
| p123  |         3.248 |        3.443 |    123.1 / 120.7 |

Sharing alone is slower than p1. Adding donation makes p123 18.1% faster in best
and 13.4% faster in all than p1, with separated two-sample ranges, but peak memory
rises about 69-78%. One UNSAT profile cannot override earlier cross-case donation
regressions. Do not promote sharing or donation. Next test p1/p12/p123 on the full
five-profile group, with its exact scope stated correctly.

Depth-10 picks 0/2 cap at 8 s. Depth-10 pick 4 exhausts in 2.038 s; all depth-12
picks exhaust in 0.346-1.562 s. Every completed prefix has an empty exact witness
set and 1/1 local root exhaustion. Freeze depth-10/pick-4 as the next repeatable
hard-10 calculation control. Its proof remains limited to that certificate.

Evidence: `summary.json`, `summary-rechecked.json`, `whole-results/summary.json`,
`whole-results/summary-rechecked.json`, `analysis-rechecked.json`, schedules,
binary/source hashes and process metrics in the corrected run root.
