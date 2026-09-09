# Sparse link count

Decision: retain as a portfolio branch; unsuitable as the sole default.
Let P be operator input ports, I external input belts, X input-to-operator belts, D input-to-terminal belts and L operator-to-operator belts. Exact port equations P=L+X and I=X+D imply L=P-I+D. Assert D=L+I-P using sparse terminal edges. This holds with cycles, surplus, parallel belts and multiple inputs.

Evidence: target/astra-cardinality-screen-20260908; benchmarks/cardinality.json and cardinality-binaries.json. The shared screen has 132 verified records, 128 completed; four timeouts belong to Boolean case 10.

First-optimum / sparse paired medians in seconds:

| Scope           | First optimum / sparse |
| --------------- | ---------------------- |
| 10 One min N/L  | 121.187 / 22.731       |
| 36 One min N/L  | 8.249 / 8.645          |
| 36 All min N    | 22.819 / 19.432        |
| 115 All min N   | 11.551 / 16.580        |
| 238 All min N   | 2.007 / 2.281          |
| 258 One min N/L | 7.110 / 38.921         |
| 258 All min N/L | 50.268 / 68.810        |

Full exact result comparisons passed. Retain the regression evidence.
Snapshot: target/astra-sparse-links-candidate-20260908. Runner SHA256: bb7bb2da257aafbb456d09f0e353cfc07f4150a7e9a1b3ed79be520393a3b0ff.
