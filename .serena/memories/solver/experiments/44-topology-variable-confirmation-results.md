# 44. Topology-controlled confirmation results

Date: 2026-09-02. State: permanent after user approval.
This is a recommendation, not a new promotion commit. Design and identities:
`mem:solver/experiments/44-topology-variable-confirmation`.
Accounting results: `mem:solver/experiments/45-cache-accounting-results`.

## Verification

Run: target/parallelism-ladder/topology-final-20260902.
Started 09:11:57, finished 12:41:03 Europe/Paris, about 3h29m.
198/198 optimal completions, no deadline, cap, kill or verification failure.
Frozen analyzer independently rerun without --allow-incomplete; all 905 frozen
artifact hashes and recorded topology/affinity evidence pass. Original/rechecked
summary SHA256:
d92a22be3d10147bd98180b0b7f56d79cc647a77c2b1525b3ad11ff392af3837.

All 99 pairs preserve exact proofs and 15 structural counters. Full solution
objects, preferred keys, complete key sets and optima also agree across aliases,
placements and variants for all five exact problem/mode groups.
Counter equality is evidence of equal compared search work, not a new proof rule.
Raw evidence and original summaries are unchanged.
Additional audit: results/audit-20260902.json; reproducible script:
target/exp44-analysis.py; recheck log: target/exp44-reverification.log.

## Complete variables versus before

150 jobs, 75 adjacent pairs. Five pairs per cell, p1, hotspots off, capacity 1200.
CCD96 = mask ffff, 16 workers; CCD32 = ffff0000, 16 workers;
all = unrestricted, 32 workers. Negative changes mean faster/lower.
Changes are medians of paired ratios, not ratios of independent medians.
Intervals are exploratory percentile-bootstrap 95% intervals with only five pairs.
No pooling across workloads/placements and no correction for multiple comparisons.

| Workload          | Placement | Wall change | Wall interval   | First valid change | CPU change |
| ----------------- | --------- | ----------: | --------------- | -----------------: | ---------: |
| 115 all           | CCD96     |      -1.09% | -1.63 to -0.50% |             +0.16% |     -1.12% |
| 115 all           | CCD32     |      -0.78% | -1.30 to -0.56% |             -1.54% |     -1.19% |
| 115 all           | all       |      +0.32% | -1.90 to +0.58% |             -0.61% |     -1.11% |
| 238 optimal       | CCD96     |      -1.28% | -1.48 to +1.34% |             -1.28% |     -0.85% |
| 238 optimal       | CCD32     |      -0.04% | -2.69 to +1.58% |             -0.04% |     -0.85% |
| 238 optimal       | all       |      -0.46% | -1.66 to +1.54% |             -0.46% |     -0.55% |
| 238 minimum_links | CCD96     |      -1.33% | -4.23 to +1.72% |             -1.33% |     -1.03% |
| 238 minimum_links | CCD32     |      -0.69% | -1.31 to +0.88% |             -0.69% |     -1.50% |
| 238 minimum_links | all       |      -0.19% | -0.95 to +3.05% |             -0.19% |     -0.67% |
| 258 minimum_links | CCD96     |      -1.31% | -1.87 to +0.74% |             -1.31% |     -0.94% |
| 258 minimum_links | CCD32     |      +1.07% | -2.21 to +2.78% |             +1.07% |     -0.22% |
| 258 minimum_links | all       |      -0.53% | -2.68 to +0.75% |             -0.53% |     -0.17% |
| 36 optimal        | CCD96     |      -1.05% | -1.36 to -0.27% |             -1.05% |     -1.31% |
| 36 optimal        | CCD32     |      +0.18% | -0.36 to +1.01% |             +0.18% |     -0.79% |
| 36 optimal        | all       |      -0.81% | -3.03 to +2.05% |             -0.82% |     -2.57% |

Descriptive counts: wall faster in 53/75 pairs and 12/15 cell medians.
First valid faster in 52/75 pairs; CPU lower in 67/75 pairs and all 15 medians.
These counts are not a pooled speedup or a general significance test.

The adverse 258/CCD32 cell is retained. Candidate/reference changes by repeat:
r1 -2.21%, r2 +2.78%, r3 +1.07%, r4 +2.53%, r5 -1.78%.
Reference-first pairs have median +2.66%; candidate-first -1.78%.
This small post-hoc order split suggests residual drift, but does not establish
thermal/background-load causality or justify deleting/adjusting samples.
Algebra time is lower in every pair, CPU in four of five. That alone does not
establish a wall gain. The earlier large unrestricted 258 regression did not
reappear in this matched run; its historical samples remain valid observations.

## Topology and the CPU trough

Reference wall medians, seconds:

| Workload          | CCD96, w16 | CCD32, w16 | Unrestricted, w32 |
| ----------------- | ---------: | ---------: | ----------------: |
| 115 all           |     50.788 |     47.043 |            36.163 |
| 238 optimal       |      9.107 |      8.444 |             7.044 |
| 238 minimum_links |      9.147 |      8.463 |             7.098 |
| 258 minimum_links |    193.189 |    287.085 |           244.719 |
| 36 optimal        |     14.977 |     13.615 |            12.110 |

258 favors CCD96, while other cases favor unrestricted execution. This is not a
universal production affinity recommendation. The two CCDs use the same w16
search plan/work; w32 changes some adaptive plans and work counts, so do not
interpret its difference as a pure placement effect.

Per-second process CPU deltas show unrestricted 258 spends its final half using
a median 2.31 logical cores before and 2.25 with variables. About 41% of total
elapsed time is at <=2.5 cores. This confirms a CPU-utilization tail, not its cause
by itself. Earlier selected-root diagnostics identify outstanding search roots.
No hotspot instrumentation in these runs. Balanced power scheme was recorded.
Affinity controls placement but does not remove boost, thermal or load variation.

## Recommendation and limits

Keep complete variables as a modest optimization. Evidence includes repeated
completed wall wins on both CCDs for 115, on CCD96 for 36, mostly favorable other
cells, consistent CPU medians, exact work preservation and prior root evidence.
Do not claim a universal speedup or a proved 258/CCD32 win. Small regressions remain
possible; lower CPU cost is supporting evidence, not a substitute for wall time.
No need to repeat the entire 198-job suite for this decision. Carry adverse cells
as regression controls, with exactly balanced order and more pairs if needed.

Next isolated calculation hypothesis: rational_rref currently sorts/deduplicates
its final rows. After exact reduction and zero-row removal, distinct unit pivots
already imply uniqueness and ascending pivot order, whose reverse is the existing
lexicographic row order. Test replacing only final sort/dedup with reversal, using
the retained dense oracle, row permutations/scalings, rank deficiency and contradictions.
No implementation or speed claim yet. If negligible, profile elimination arithmetic
next. Integer inequality rows remain rejected; scheduler extras remain paused.

Variable candidate already committed in 520b352; no new source edit, promotion
commit or push during analysis. Result/handoff memory updates are uncommitted.

## Accepted retention

The user approved applying recommendations after analysis. This change is now
permanent in the existing isolated source commit 520b352; no source rewrite was
needed. All measured limitations above remain. Promotion documentation is being
committed separately from the next RREF candidate. No push authorized.
