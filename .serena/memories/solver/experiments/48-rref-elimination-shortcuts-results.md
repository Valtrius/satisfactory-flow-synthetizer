# 48. RREF elimination shortcut results

Date: 2026-09-02. Completed and independently verified.
Implementation, validation failures/repairs and frozen candidate identities:
`mem:solver/experiments/48-rref-elimination-shortcuts`.

## Evidence and correctness

Run `target/parallelism-ladder/rref-elimination-20260902` finished
20:20:55.038 Europe/Paris after about 33m45s, within the one-hour limit.
All 32 reference jobs completed optimally; no cap, kill, failure or invalid result.
Sixteen adjacent pairs follow the frozen schedule, with AB/BA balance in each cell.
Hotspots OFF; p1; capacity 1200. Placement verified before resume for all jobs.
Affinity application took 0.0068231 to 0.0586137s, outside solve timing.

Independent frozen analyzer passes. Original and rechecked summary SHA256:
`3387537fdc14efd9f36359b4861b566be2e3f4ac399e421e3f03d07b0841a458`.
All 263 frozen artifact hashes match. Independent audit checks full solutions,
layout-key sets, preferred witness, complete outcomes and proofs across all four
exact problem/mode groups, including both candidate aliases. All 15 checked
structural counters agree within every pair. These are complete-work comparisons.

- 115 all: 49 layouts, N=7, L=10.
- 238 optimal: one layout, N=8, L=13.
- 258 minimum_links: two layouts, N=9, L=14.
- 36 optimal: one layout, N=9, L=11.

Evidence: `results/results.csv`, `results/summary.json`,
`results/summary-rechecked-20260902.json`, `results/audit-20260902.json`,
`frozen-hashes.json`, `input/schedule.json`.
Read-only audit helper/log: `target/exp48-analysis.py`, `target/exp48-analysis.log`.
Reverification log: `target/exp48-reverification.log`.
Original evidence is unchanged.

## Completed timing

Each cell has only two pairs. Percentages are median candidate/reference changes,
not ratios of unpaired medians. Negative means less time. Never pool placements.

| Workload / placement                 | Zero wall | Minus wall | Zero CPU | Minus CPU |
| ------------------------------------ | --------: | ---------: | -------: | --------: |
| 115 all, CCD96, 16 workers           |    +0.52% |     -1.16% |   -1.92% |    -1.07% |
| 238 optimal, CCD32, 16 workers       |    -2.64% |     -0.65% |   -2.65% |    -3.31% |
| 258 minimum_links, CCD96, 16 workers |    +4.71% |    -0.001% |   +0.63% |    -1.23% |
| 36 optimal, unrestricted, 32 workers |    -2.28% |     -1.14% |   -4.69% |    -1.06% |

Pair detail below preserves both observations. Values in seconds.

| Candidate / case | Reference -> candidate, pair A | Reference -> candidate, pair B | Paired wall changes |
| ---------------- | ------------------------------ | ------------------------------ | ------------------- |
| minus / 115      | 48.137 -> 47.493               | 50.691 -> 50.194               | -1.34%, -0.98%      |
| minus / 238      | 7.810 -> 7.718                 | 7.561 -> 7.552                 | -1.17%, -0.12%      |
| minus / 258      | 175.144 -> 173.840             | 198.170 -> 199.643             | -0.75%, +0.74%      |
| minus / 36       | 13.053 -> 12.896               | 12.701 -> 12.565               | -1.21%, -1.07%      |
| zero / 115       | 47.932 -> 46.827               | 51.371 -> 53.086               | -2.31%, +3.34%      |
| zero / 238       | 7.722 -> 7.455                 | 7.732 -> 7.592                 | -3.46%, -1.81%      |
| zero / 258       | 175.105 -> 172.994             | 176.151 -> 194.855             | -1.21%, +10.62%     |
| zero / 36        | 11.720 -> 11.289               | 11.501 -> 11.401               | -3.68%, -0.87%      |

Pair A/B denotes table order, not repeat or execution role. Audit retains actual
pair IDs, roles and schedule indices. Both order directions occur in every cell.

## First witness and resource observations

115 first-witness paired median changes: zero -3.37%, minus +0.81%.
Minus has one witness improvement and one regression, so no first-witness gain
is established there. On the other workloads, witness timing tracks completion.
This does not establish performance for untested modes or hard10.

Minus lowers whole-process CPU in all eight pairs and completion wall in seven.
Zero lowers CPU in seven and completion wall in six. These counts describe this
screen, not a pooled effect or significance test. 258 baseline times also vary,
including 175.144 versus 198.170s in the minus comparison. Fixed CCD placement
does not eliminate every source of timing variation. The cause of the zero
candidate's 194.855s run is unknown; keep it as adverse evidence, not an outlier
to discard. Equal search counters alone cannot distinguish arithmetic cost,
OS scheduling, thermal/boost effects or background load.

Sampled peak memory median changes for zero/minus, respectively:
115 -3.11%/+4.42%; 238 +0.19%/-1.62%; 258 -0.48%/-1.36%; 36 +1.35%/+0.27%.
Sampling is coarse and these differences do not establish a memory effect.
Memory is not a promotion gate. No new hotspot or root-tail causality claim.

## Decision and recommendation

Nothing promoted from this first screen. Exactness checks pass, but two pairs
per cell are too few to establish these small completion gains.

1. Prioritize negative-unit confirmation, using the same isolated frozen binaries.
   Four additional AB/BA pairs per workload give 32 jobs and six observed pairs
   per cell across the two screens. Analyze the new screen separately first;
   any combined view must retain screen identity, placement and matched pairing.
   Same caps give 2800s search + 480s cleanup = 54m40, leaving 5m20 overhead inside
   one hour. Keep 258 as the long completed regression control.
2. Hold zero-destination and keep its exact patch/evidence. Do not combine it with
   minus or promote it until its mixed 115/258 results have been resolved by
   independent repeats. It is not yet proven harmful or beneficial overall.
3. If minus repeats its small completion benefits without a sustained regression,
   make that isolated change permanent with its related documentation. Lower CPU
   supports the mechanism, but is not a substitute for completion evidence.
   Scheduler extras remain paused.

This is a recommendation, not an approved or launched follow-up.

## Source and commit state

Preparation/tests/patches/manifest committed as fadb316, prior experiment47 results
as da2c89f. Current checkout HEAD is unrelated history commit 6683546; preserved.
Independent audit confirms current canonical.rs and solver.rs production prefixes
still match the frozen before source. Neither arithmetic shortcut is in production.
Only Serena records changed during this analysis; results/handoff uncommitted.
No new commit, push, build, test suite or benchmark launch. Nothing in flight.
