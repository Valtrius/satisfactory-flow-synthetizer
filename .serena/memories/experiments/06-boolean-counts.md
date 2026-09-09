# Boolean exact counts

Decision: retain as a portfolio branch; do not promote alone on current hard-case evidence.
Encode exact row, column and direct-terminal counts with bidirectional prefix thresholds q(i,j)=q(i-1,j) OR (q(i-1,j-1) AND edge(i)). Require k and forbid k+1. Complement literals above n/2; handle zero, full and impossible counts. Exhaustive cvc5 truth tables through six literals and Reference full witness tests passed.

Evidence: `cardinality-screen` in benchmarks/evidence.json, the same 132-record screen as sparse, but a distinct paired cohort.

Sparse / Boolean medians in seconds:

| Scope           | Sparse / Boolean           |
| --------------- | -------------------------- |
| 10 One min N/L  | 20.655 / incomplete at 300 |
| 36 One min N/L  | 8.652 / 2.659              |
| 36 All min N    | 18.185 / 4.276             |
| 115 All min N   | 16.547 / 5.936             |
| 238 All min N   | 2.193 / 0.633              |
| 258 One min N/L | 40.682 / 2.043             |
| 258 All min N/L | 61.699 / 49.198            |

Four case-10 timeouts: two main and two diagnostic. Diagnostic case 10 produced no models. Full canonical objects and objectives matched completed controls.
Snapshot: `before-portfolio` in benchmarks/evidence.json. Runner SHA256: f56f09dc4aba3f1f85739fbb5016706b6556d573b83d5d61fd26923562bf6023.
