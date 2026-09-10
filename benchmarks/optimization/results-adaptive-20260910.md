# Adaptive results and scheduling stopping point

[Full HTML analysis and every paired sample](https://copyparty.jakez.eu/agent-files/JCal8u_vn.html?k=p6NNayy0JEnmlo0i).

Keep the production policy from 6852be9: descending first-output order and static
Boolean All min N/L partitions. Neither global adaptive replacement should be
promoted. Broad scheduling experiments are approaching diminishing returns.

All ten suites completed in 51 m 27 s: 56 timing runs / 28 pairs, no caps and no
verification failures. Fresh analysis verified 1,485 frozen hashes, reran ten
result verifiers, compared full solution objects across five exact enumeration
scopes and reaudited 14 qualification probes (12 complete, two intentional
cancellations). Evidence is in target/optimization-adaptive-20260910/frozen and
/analysis. The baseline source still matches current production.

Each row below has two paired repeats, All min N/L. Before/after times are separate
terminal medians; percentages are median matched-pair changes. Positive is slower.

| Case | Workers | Candidate         | Paired static baseline | Candidate median | Paired change | Paired delta |
| ---- | ------: | ----------------- | ---------------------: | ---------------: | ------------: | -----------: |
| 258  |       8 | Adaptive          |               59.871 s |         45.092 s |       -24.74% |    -14.779 s |
| 258  |      16 | Adaptive          |               60.906 s |         29.882 s |       -50.89% |    -31.023 s |
| 258  |      32 | Adaptive          |               14.165 s |         19.555 s |       +38.07% |     +5.390 s |
| 24   |      32 | Adaptive          |                2.136 s |          1.699 s |       -20.20% |     -0.437 s |
| 36   |      32 | Adaptive          |                6.892 s |          3.870 s |       -43.83% |     -3.022 s |
| 65   |      32 | Adaptive          |                0.522 s |          0.319 s |       -38.86% |     -0.203 s |
| 258  |       8 | Adaptive + 250 ms |               60.977 s |         49.498 s |       -18.78% |    -11.479 s |
| 258  |      16 | Adaptive + 250 ms |               61.686 s |         30.265 s |       -50.92% |    -31.421 s |
| 258  |      32 | Adaptive + 250 ms |               15.303 s |         20.810 s |       +36.02% |     +5.508 s |
| 24   |      32 | Adaptive + 250 ms |                2.090 s |          1.358 s |       -35.04% |     -0.733 s |
| 36   |      32 | Adaptive + 250 ms |                6.600 s |          3.577 s |       -45.81% |     -3.023 s |
| 65   |      32 | Adaptive + 250 ms |                0.545 s |          0.272 s |       -49.72% |     -0.273 s |
| 97   |      32 | Adaptive          |              215.434 s |        311.450 s |       +44.56% |    +96.016 s |
| 97   |      32 | Adaptive + 250 ms |              201.985 s |        293.855 s |       +45.87% |    +91.871 s |

Both variants lose on both 32-worker hard cases, including approximately 90–96
extra seconds on 97. Easy-case savings of 0.2–3.0 seconds and slightly earlier
witnesses do not compensate under the user's exact terminal-time priority.
Keep the 250 ms grace as an archived experiment; it has no established hard-case
advantage. The candidates were each paired with static, not directly with each
other. Do not rank them by unpaired raw medians or pool cross-session baselines.

There is one remaining targeted opportunity: adaptive saves about 15 seconds at
8 workers and 31 seconds at 16 workers on 258. A hybrid could retain static
splitting where its existing activation condition applies and use immediate
adaptive refinement otherwise. This policy is unimplemented and unmeasured.
If pursued, allow one focused comparison against current production, preserving
32-worker hard performance and confirming 8/16-worker gains. Include case 97 at
the affected worker budgets, which were absent here. Keep the session within
three hours, with no performance runs below eight workers. Failure should end
the scheduling experiment; success should lead to integration and then closure.
Current committed improvements need not wait for this additional work.

The broader solver has an unresolved backend bottleneck, not a demonstrated speed
floor. Previous case-10 All min N/L comparisons capped both baseline and static
at 600 seconds in two pairs. Its static diagnostic trace had 16 unfinished Boolean
searches, including one spending 599.87 seconds in SMT checks out of 599.96 seconds
of root wall time. This is previous evidence; this batch did not retest that scope.
Export/replay identical obligations and test a concrete proof-preserving encoding
change, complete decomposition or incremental reuse before another whole-solver
campaign. None of these directions is yet a measured improvement. Root check
times overlap and must not be called total CPU time.

Production code is unchanged by this review. No new benchmarks were launched.
