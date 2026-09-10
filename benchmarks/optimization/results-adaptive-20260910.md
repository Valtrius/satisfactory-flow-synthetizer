# Adaptive results and scheduling stopping point

[Full HTML analysis and every paired sample](https://copyparty.jakez.eu/agent-files/JCal8u_vn.html?k=p6NNayy0JEnmlo0i).

Both global adaptive replacements were rejected against baseline 6852be9.
The static partition policy remains part of production. The subsequent
[hybrid comparison and promotion](results-hybrid-20260910.md) preserves it and
adds adaptive fallback only where its root-count condition declines to split.

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

Both variants lose on both 32-worker hard cases, including approximately 90â€“96
extra seconds on 97. Easy-case savings of 0.2â€“3.0 seconds and slightly earlier
witnesses do not compensate under the user's exact terminal-time priority.
Keep the 250 ms grace as an archived experiment; it has no established hard-case
advantage. The candidates were each paired with static, not directly with each
other. Do not rank them by unpaired raw medians or pool cross-session baselines.

The 8/16-worker case-258 gains motivated the completed hybrid experiment.
Its distinct source, measured tradeoff and final promotion decision are recorded
in [Hybrid promotion](results-hybrid-20260910.md). Broad scheduling sweeps are
paused. These global adaptive/grace variants remain rejected; no comparison here
establishes that the grace policy is better than immediate adaptive.

Case 10 All min N/L was outside this batch. Prior 600-second caps are incomplete
evidence, not a solver speed floor. Future backend work requires a separate
request and an exact, reproducible obligation. No new experiment is queued.
