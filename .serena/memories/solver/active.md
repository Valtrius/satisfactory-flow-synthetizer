# solver/active

Nothing running. Experiment 47 finished and analyzed on 2026-09-02.
Run: `target/parallelism-ladder/rref-arithmetic-20260902`.
14 records verify in 18m54: 10 optimal controls, 4 intended stress caps.
192 frozen hashes, exact completed outputs/proofs and 15 structural counters match.
Results: `mem:solver/experiments/47-rref-arithmetic-profile-results`.

## Recommended next, not yet implemented or approved

Test a zero-destination elimination shortcut first, then a separate factor -1
shortcut. Observed prevalence 78–80% and 26–30% of updates; categories overlap.
Current rational-library source lacks these explicit shortcuts.
Elimination is 66–69% of measured RREF; no projected whole-solve gain.
Keep exact oracle/full-output checks and hotspot-OFF A/B timing, fixed affinity,
balanced order, all three solve scopes, and total new run budget <=1h with cleanup.
Do not start another run or infer promotion from the diagnostic alone.

Existing committed source/docs 1006e6f, manifest 14d3f73, permanent RREF promotion
docs c106d4c. Result and handoff notes are uncommitted; no new commit/push.
No production arithmetic change in this analysis. Scheduler extras stay paused.
