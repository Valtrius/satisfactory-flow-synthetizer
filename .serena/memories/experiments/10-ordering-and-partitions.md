# Ordering and partition confirmation, 10 September 2026

Authoritative results: original campaign suites 1–16 plus recovery suites 17–26. `target/optimization-campaign-20260910-recovery/RECOVERY.json` maps the 1,208 runs; exclude 45 partial rows from interrupted suite 17. Combined evidence and hashes are in `target/optimization-campaign-20260910/analysis`. All 26 result verifiers and 36 diagnostic audits passed; 1,065 optimal outcomes, 143 capped. Full saved enumeration objects agree across 24 exact scopes.

Promoted descending first-output producer order in `cd703e9`. Case 10 One min N/L at 16 workers: candidate 6/6 complete, median 23.04 s; baseline 0/6 complete at 120 s. 8/12-worker screens also improve completion. Case 97 One min N/L at 16 workers adds about 0.10 s. Outside-in adds 8.74 s for the same observed case-10 benefit, so prefer reverse. All roots and proof/cancellation requirements remain.

Static Boolean partitions are the next promotion candidate: case 258 All min N/L 57.06 → 14.74 s, six pairs; case 97 452.69 → 218.02 s, two pairs, 813 matching layouts. Easy-case penalties around 0.96 s (24) and 1.91 s (36) are acceptable under the user's clarified priorities. Confirm the combined descending-order/partition policy before promotion. Case 10 All min N/L still caps at 600 s.

Adaptive splitting remains a follow-up for improved 8/16-worker behavior; two-repeat evidence and only 180 s caps for case 97. Sparse startup delay is secondary. Do not pool cross-session medians or count scaled258 as independent evidence.

Next prepared queue: `make-optimization-campaign.py --promotion`; baseline and pairs-boolean built from cd703e9. Five suites, 56 jobs, 28 pairs, workers 8/16/32. Six paired repeats each for 258 and 97 All min N/L plus scope guards. Worst scheduled session including verification reserves: 10,460 s (2 h 54 m 20 s); controller maximum 10,800 s. Qualification uses `validate-optimization-followup.py --promotion`. Preparation does not imply completion or promotion.

See `mem:solver/benchmarking` for current priorities and time/worker limits.
