# 01. Parallelism ladder and overhead

Date: 2026-08-27. State: analyzed; superseded for current timing by later kernel runs.
Hypothesis: deeper root frontiers, completed-state sharing, DFS donation and
overlapping remaining L-groups can feed more workers and finish enumeration sooner.

## What was tried

Implemented the four independently selectable controls and their proof-safe
coordination. Tested baseline, p1, p12, p123 and p1234 at 32 workers, then isolated
sharing/groups and baseline/full-stack at one and four workers. See [stage names](../controls.md).
Hardware: Ryzen 9 7950X3D, 32 logical workers. Historical baseline revision `fda1a5d`.

The 10 pilot and 20 repeat runs provide three fresh-process samples per 32-worker
stage/case. Eight worker-scaling and four ablation runs are single samples.
All 42 performance records completed with the same 13/6 exact layout keys and
preferred witnesses for 65/24 respectively. `script-smoke` was a separate capped
incomplete harness check, not a performance result.

## Results

Find-all median seconds at 32 workers, before the calculation-kernel optimization:

| Stage    | 65 = 40+25 | 24 = 7+6+5+4+2 |
| -------- | ---------: | -------------: |
| baseline |      67.58 |          83.29 |
| p1       |      31.34 |          27.45 |
| p12      |      26.16 |          32.11 |
| p123     |      28.78 |          33.12 |
| p1234    |      20.76 |          16.10 |

Full-stack median speedups were 3.26x/5.17x, but process CPU rose about 1.53x/1.15x.
Its sampled peak working sets were 676/730 MiB versus baseline 214/223 MiB.
Partitions alone peaked at 168/175 MiB. 65 baseline varied from 52.44 to 68.88 s
despite identical work counters, so small apparent gains are uncertain.

Single-sample controls, seconds, each cell is 65 / 24:

| Comparison                           |  Baseline or simpler case | Added scheduling |
| ------------------------------------ | ------------------------: | ---------------: |
| Four workers, baseline vs full stack |             67.56 / 62.11 |    70.61 / 80.70 |
| One worker, baseline vs full stack   |           164.98 / 177.42 |  161.25 / 167.29 |
| 32 workers, sharing alone            | Not a new baseline sample |    65.04 / 79.57 |
| 32 workers, groups alone             | Not a new baseline sample |    32.67 / 31.85 |

## Conclusion

Deeper frontiers and overlapping groups helped these large-worker cases. Sharing
and donation were not consistently faster. Extra CPU/memory and four-worker
regressions ruled out enabling the full stack globally. All defaults stayed off.
Reducing exact calculation cost became the next intervention, not more scheduler complexity.

## Evidence and follow-up

Local `target/parallelism-ladder/`: `pilot-final/results.csv`,
`repeats-final/results.csv`, `worker-scaling/results.csv`, `ablations/results.csv`,
plus per-run JSON, schedule, hashes and compiler/revision files. `stage0/` and
`script-smoke/` preserve setup checks; `baseline-bin/` preserves early executables.
These historical runs must not be silently pooled with later kernel/capacity settings.
Follow-up: [02, exact kernel and both solve modes](02-exact-kernel.md).
