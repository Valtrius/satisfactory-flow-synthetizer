# 45. Cache accounting results

Date: 2026-09-02. State: permanent after user approval.
Design, correctness oracle and binary identity: `mem:solver/experiments/45-cache-accounting`.
Shared verification/topology: `mem:solver/experiments/44-topology-variable-confirmation-results`.

## Evidence

48 completed jobs, 24 adjacent pairs, no failures or caps. Reference = variables;
candidate = accounting, the same solver plus allocation-free BigInt payload-length
accounting. Exact outputs, proofs and all 15 compared structural counters agree.
The 90,310 previous oracle comparisons cover correctness of the size calculation.
No new solver code or tests in this analysis-only turn.

Run: target/parallelism-ladder/topology-final-20260902.
Original and independent frozen recheck summary SHA256:
d92a22be3d10147bd98180b0b7f56d79cc647a77c2b1525b3ad11ff392af3837.
Detailed pairs and audit: results/summary.json and results/audit-20260902.json.

## Paired results

All capacity 1200, p1, hotspots off, 16 workers on one verified CCD.
Negative means faster/lower; intervals are exploratory bootstrap 95% ranges.
Five pairs for 115 and 238; only two for each 258 cell.

| Workload          | CCD   | Pairs | Wall change | Wall interval   | First valid | CPU change |
| ----------------- | ----- | ----: | ----------: | --------------- | ----------: | ---------: |
| 115 all           | 96MiB |     5 |      -0.74% | -1.69 to -0.47% |      -0.18% |     -1.12% |
| 115 all           | 32MiB |     5 |      -1.12% | -1.50 to -0.76% |      -1.39% |     -1.15% |
| 238 optimal       | 96MiB |     5 |      -2.10% | -2.24 to +0.34% |      -2.10% |     -1.06% |
| 238 optimal       | 32MiB |     5 |      -1.88% | -5.45 to +0.22% |      -1.88% |     -1.17% |
| 258 minimum_links | 96MiB |     2 |      -0.14% | -0.35 to +0.06% |      -0.14% |     -0.59% |
| 258 minimum_links | 32MiB |     2 |      -0.68% | -0.80 to -0.56% |      -0.68% |     -0.37% |

Wall faster in 21/24 pairs and all six cell medians; CPU lower in every pair.
First valid improves in 19/24 pairs and all six medians. Counts are descriptive.
115 completion improves in all ten pairs; 238 in eight of ten.
115/CCD96 first-valid interval crosses zero, so no blanket first-witness claim.
The two-pair 258 observations are supportive, not independent promotion evidence.

## Recommendation

Retain as a small permanent improvement, based primarily on repeated completed
115/238 comparisons plus exact correctness. This is the clearer candidate of the two.
Do not promise 2% on every workload. Unrestricted w32, 36 and 10 accounting
comparisons were not included, and combined gains cannot be obtained by adding
the two candidates' percentages.

Use a few unrestricted accounting regression pairs in the next useful screen,
including 258 and 36 optimal, without another full confirmation matrix.
Memory savings are not a promotion gate; this removes needless allocation work.
It does not solve the uneven search tail or change cache content/scheduling.

Candidate is already active and committed separately in 331b87a.
Promotion is recommended, not recorded as applied by this analysis.
No new source edit, commit or push. Result/handoff notes remain uncommitted.

## Accepted retention

The user approved applying recommendations after analysis. This change is now
permanent in the existing isolated source commit 331b87a; no source rewrite was
needed. All measured limitations above remain. Promotion documentation is being
committed separately from the next RREF candidate. No push authorized.
