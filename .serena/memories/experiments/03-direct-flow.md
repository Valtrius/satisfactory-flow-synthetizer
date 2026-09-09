# Direct single-input flow equalities

Decision: keep. Exactly one incoming edge, with each selected edge implying source flow equals required flow, replaces the conditional sum for outputs, splitters and single discards. Mergers and multiple discards retain sums. No routing is removed. Cyclic 5 = 2 + 2 + 1 at capacity 6 validates feedback above external supply.

Evidence: `direct-flow-screen` in benchmarks/evidence.json; benchmarks/direct-flow.json and direct-flow-binaries.json. All 52 records verified. Of 26 pairs, 24 completed with equal full canonical objects, sets, proof, objectives and scope; two case-10 pairs remained incomplete at 180 seconds.

Paired medians in seconds:

| Scope           | Independent control / candidate | Partition control / candidate |
| --------------- | ------------------------------- | ----------------------------- |
| 36 All min N    | 62.521 / 20.737                 | 52.919 / 20.434               |
| 115 All min N   | 15.881 / 10.869                 | 23.454 / 10.761               |
| 238 All min N   | 5.247 / 1.871                   | 31.123 / 1.917                |
| 258 All min N/L | 203.874 / 51.739                | —                             |

Layout counts: 36 = 12, 115 = 49, 238 = 1, 258 = 2 at N=9, L=14. Completed independent control 258 independently confirmed the full set. This resolved the partition regression on 238.

Correction: incomplete case 10 retained validated N=11, L=18 incumbents at 114.086 / 113.114 seconds. Zero enumeration count in One min N/L did not mean no incumbent.
Runner SHA256: 52ef07b33475f7e0d006a17f3084c2d199ff6c6324d140bc2d976a3a871cdcdc.
