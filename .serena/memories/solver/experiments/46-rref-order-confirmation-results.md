# 46. Focused RREF confirmation results

Date: 2026-09-02. State: permanent after user approval.
Design: `mem:solver/experiments/46-rref-order-confirmation`.
Prior screen, including retained failure: `mem:solver/experiments/46-rref-order-results`.
The user accepted promotion after analysis. Permanent source remains in a79ecf7.

## Verification

Run target/parallelism-ladder/rref-order-confirmation-20260902.
Started 17:48:36, finished 18:26:23.904 Europe/Paris, about 37m48s.
20/20 optimal completions, no caps, crashes, kills or verification failures.
Independent frozen analyzer recheck passes all 976 frozen artifact hashes and
recorded topology/affinity checks. Original and rechecked summary SHA256:
4d9838bb82b3c8c34952da2a8be56fb47c7a31c598a896a75d1405704ffdc7cf.

All ten pairs preserve exact proofs and 15 structural counters. Full solution
objects, preferred keys, complete layout sets and optima agree across aliases
for all three exact problem/mode groups. Outputs: 115 all has 49 layouts N7/L10;
238 optimal one N8/L13; 258 minimum_links two N9/L14.
No source edits or new tests/builds in this analysis-only turn.

Evidence: original results/summary.json, separate summary-rechecked-20260902.json,
results/audit-20260902.json, target/exp46-confirmation-reverification.log.
Reproducible audit: target/exp46-confirmation-analysis.py.
Old failed run and current raw/frozen evidence are unchanged.

## Paired timings

Capacity1200, p1, hotspots off, 16 workers on verified CCD masks.
Negative changes mean faster/lower. These are medians of individual paired
ratios, not ratios of independently computed variant medians.
Intervals are exploratory bootstrap 95% intervals; two/four pairs are small.

| Comparison/workload          | Placement | Pairs | Wall change | Wall interval   | CPU change | First valid |
| ---------------------------- | --------- | ----: | ----------: | --------------- | ---------: | ----------: |
| RREF, 115 all                | CCD96     |     2 |      -1.49% | -1.55 to -1.44% |     -1.14% |      -2.96% |
| RREF, 238 optimal            | CCD32     |     4 |      -1.44% | -5.60 to +0.38% |     -2.68% |      -1.44% |
| RREF, 258 minimum_links      | CCD96     |     2 |      +0.38% | -0.84 to +1.60% |     -1.30% |      +0.38% |
| Variables, 258 minimum_links | CCD32     |     2 |      -1.09% | -1.43 to -0.74% |     -0.59% |      -1.09% |

RREF compares accounting -> rref; variable discovery compares before -> variables.
No factorial/bundle gain can be inferred by adding these percentages.

RREF wall improves in 6/8 new pairs and CPU in all eight. Across the first screen
and confirmation, that is 12/18 wall pairs and 18/18 CPU pairs, descriptive counts
only. Do not pool cross-placement/session raw times or present a universal speedup.
115 paired wall medians improved -0.44% then -1.49%; 238 -1.68% then -1.44%.
Their repeated completed gains support retaining the shortcut.
The two 258 RREF pairs are +1.60% and -0.84%, so no completion improvement on 258
has been demonstrated. The CPU decrease alone does not establish a wall gain.

## Previously capped variable control

Both fresh runs finish, preserving the full two-layout result:
r2 before 297.815s, variables 293.548s; r1 before 307.179s, variables 304.898s.
Both candidate runs finish within even the old 320s cap. The new 400s allowance
avoided a tight cutoff but did not itself produce faster execution.
Earlier 320s timeout is not reproduced. Its cause remains unproved; preserve the
failed run and do not retroactively call it complete or blame background load.
This supports keeping the earlier variable promotion, not a universal guarantee.

## Decision and next step

Recommend making the RREF ordering shortcut permanent, based on repeated exact
completed 115/238 gains, unchanged work, consistent CPU reductions and the retained
dense-order oracle. It is a small optimization, not a demonstrated 258 speedup.
The source already exists in isolated commit a79ecf7; no code rewrite is needed.
No promotion commit, new source edit, benchmark launch or push during analysis.
Results/handoff notes uncommitted. Previous variable/accounting promotions remain.

Stop broad repeats of this ordering change. Next proposed work is a diagnostic
split of canonical rational_rref pivot normalization versus row elimination and
nonzero-suffix preparation. Current code already skips zero factors and +1
multiplication. Measure negative-unit factors, zero destinations and coefficient
sizes before selecting another exact arithmetic shortcut. Rational operators
delegate to BigRational, so inspect its actual behavior before claiming redundant
work. This is a profiling proposal, not implemented or measured.
Keep instrumentation out of ordinary timing runs, scheduler extras paused, and
any next benchmark within the user's one-hour search+cleanup limit.

## Accepted promotion

User approved the recommendation on 2026-09-02. RREF reverse ordering is permanent
in existing isolated source commit a79ecf7. No code rewrite was needed. All measured
limitations remain, including mixed 258 completion times. Promotion documentation
is committed separately from the next diagnostic-only implementation. No push.
