# First proved optimum

Decision: keep; commit 16713a0. The user permits any optimal tie. One min N/L returns the first validated witness at proved minimum N/L. Stop and join sibling roots without counting them exhausted. All min N/L and All min N still exhaust their requested scopes. Optional root diagnostics and any_optimum comparison preserve all other exact checks.

Evidence: target/astra-first-optimum-screen-20260908; benchmarks/first-optimum.json and first-optimum-binaries.json. 94 records verified, 90 completed. Prior Astra and Custom each timed out twice on case 10 at 900 seconds. Candidate case 10 medians: 112.719 seconds in the prior-Astra cohort, 117.117 in the Custom cohort, N=11, L=18.

Prior / new paired medians in seconds:

| Scope           | Prior / new     |
| --------------- | --------------- |
| 36 One min N/L  | 9.598 / 7.641   |
| 258 One min N/L | 50.327 / 6.807  |
| 238 One min N/L | 1.811 / 1.227   |
| 36 All min N    | 20.445 / 20.639 |
| 115 All min N   | 11.009 / 11.025 |
| 238 All min N   | 1.904 / 1.958   |
| 258 All min N/L | 50.454 / 50.531 |

Full sets and objects matched. Diagnostics: zero within-root duplicates for 36, 238, 258 and 10; one among 50 for 115. Case 258 summed check time 201.961–204.751 seconds versus root wall time 202.137–204.918; longest root 50–51 seconds, UNSAT without models. Checks dominate; concurrent times overlap.
Snapshot: target/astra-before-cardinality-20260908. Runner SHA256: d344ead5543dbe41db799233c355d9a9a867486a047c42f33f81773ddb9b2b26.
