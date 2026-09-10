# Ordering and partition confirmation

Descending first-output order promoted in cd703e9. Case 10 One min N/L at 16 workers: candidate 6/6 complete, median 23.04 s; baseline 0/6 at the 120 s cap. Screens at 8/12 also improved completion. Case 97 One min N/L added about 0.10 s; outside-in added 8.74 s for the same case-10 result, so reverse was preferred. Preserve all roots and cancellation joins.

Authoritative corpus: original suites 1–16 plus recovery 17–26. RECOVERY.json selects 1,208 runs, excluding 45 partial rows from interrupted suite 17. All 26 verifiers and 36 diagnostic audits passed: 1,065 optimal, 143 capped, full objects across 24 exact scopes. Evidence IDs optimization-confirmation-recovery and optimization-confirmation-analysis; repository results-20260910.md.

Static Boolean screening: 258 All min N/L 57.06 -> 14.74 s (six pairs), 97 452.69 -> 218.02 s (two pairs). Those signals led to `mem:experiments/11-partition-promotion`. Case 10 All min N/L remained capped at 600 s; it has not been retimed with the final hybrid. Global adaptive and hybrid follow-ups are complete in records 12/13. No proposed queue here remains pending.
