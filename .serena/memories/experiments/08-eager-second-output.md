# Rejected eager second-output partitions

Decision: rejected; production restored to the verified portfolio. Archive commit a9c114d.
The candidate refined underfilled live-root groups with at least two outputs and multiple workers. It replaced each first-output parent with all second-output producer choices, scheduling rounds across parents. Children were disjoint and exhaustive, and the ledger counted children. Correctness passed, but extra and harder checks hurt completion of hard cases.

Evidence: `output-pairs-screen` in benchmarks/evidence.json; benchmarks/output-pairs.json, output-pairs-binaries.json, output-pairs-results.json and output-pairs-rejected.patch. 100 records verified, 95 completed. Five candidate case-10 timeouts at 300 seconds: three timing and two diagnostic. Across 15 scopes: two faster, twelve slower, one lost completion.

Three paired repeats; medians in seconds:

| Scope           | Portfolio / eager       |
| --------------- | ----------------------- |
| 10 One min N/L  | 25.179 / timeout at 300 |
| 258 All min N/L | 39.902 / 16.876         |
| 238 One min N/L | 0.534 / 0.304           |
| 36 All min N    | 5.317 / 8.114           |
| 36 One min N/L  | 3.087 / 5.467           |
| 115 All min N   | 6.327 / 7.389           |
| 238 All min N   | 0.703 / 1.377           |
| 258 One min N/L | 2.593 / 8.598           |

Case 258 improved 57.7% within this screen; its control measured about 59 seconds in the portfolio screen, so avoid ratios across screens. Case 36 proof-owner roots increased from 484 to 1052, checks from 39 to 65, summed root wall time from about 54 to 105 seconds. Case 258's longest root was around 10 seconds versus 40–60 for the portfolio control; some case-10 children checked for 300 seconds without models.

Next hypothesis: delayed, bounded child partitions while preserving parent progress. This remains untested.
Rejected runner SHA256: 4d072a2dbe0ca979345f74a58f8ef43492a76289e34e15edac6ecc9bc8290d2f. The archive patch retains its exact source paths for reproducibility.
