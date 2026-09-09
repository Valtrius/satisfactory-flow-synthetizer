# Independent sparse/Boolean portfolio

Decision: retain the current default; commit 02ae07b.
Two complete searches share the total worker budget: 16 each at 32 workers. Sparse gets the odd extra; one worker uses Boolean only. Each owns an independent ledger. The first complete proof wins after both branches are joined. User cancellation stays incomplete; failure of one branch cannot prove the other's work. Copy one proof owner, retain the first equal incumbent, and deduplicate the requested scope. See `mem:solver/contracts`.

Evidence: target/astra-portfolio-screen-20260908; benchmarks/portfolio.json and portfolio-binaries.json. 130 records verified, 128 completed. Only two Boolean case-10 controls remained incomplete at 300 seconds. All portfolio jobs completed. Ten root-diagnostic proof-owner audits matched terminal exhaustion.

Separate paired cohorts; medians in seconds:

| Scope           | Sparse / portfolio | Boolean / portfolio     |
| --------------- | ------------------ | ----------------------- |
| 10 One min N/L  | 20.133 / 24.221    | timeout at 300 / 24.522 |
| 36 One min N/L  | 8.427 / 3.186      | 2.603 / 3.124           |
| 36 All min N    | 18.153 / 5.241     | 4.216 / 5.170           |
| 115 All min N   | 15.745 / 6.333     | 5.872 / 6.180           |
| 238 All min N   | 2.166 / 0.700      | 0.627 / 0.693           |
| 258 One min N/L | 35.108 / 2.493     | 1.995 / 2.555           |
| 258 All min N/L | 64.331 / 59.582    | 49.474 / 58.779         |

Overhead against the best standalone formulation: about 23% on 36 All min N and 19% on 258 All min N/L; tiny cases moved from roughly 0.03 to 0.04 seconds. This improves hard-case coverage, with some speed tradeoffs. Full exact sets, objects and objectives passed.
Snapshot: target/astra-before-output-pairs-20260908. Runner SHA256: d820f13c0453316c0cd278d91342a06161c5c3d011ac8cb6b32fa85e1897744b.
