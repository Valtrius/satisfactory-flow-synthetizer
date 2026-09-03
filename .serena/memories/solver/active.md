# Idle checkpoint after experiment50

2026-09-03: No benchmark running or queued. User requested permanent commit of49 and analysis of50; both handled. Production49 commit4d711b1899ebca8cefe1b87cfb060596257d1049, no push.

Experiment50 run target/parallelism-ladder/checked-rref-screen-20260903 finished09:20:58 Paris. All56 records and200 frozen hashes verified;52 completed,4 expected caps. Rechecked summary byte-identical. Exact public results/proofs and15 structural counters match. Candidate not promoted:115 wall-2.48%,238/258 approximately-0.2%,36 +2.80% with+8.49% CPU. Source restored to committed49; identity matches frozen50-before.

Candidate preserved as benchmarks/custom/variants/checked-rref.patch, with checked-rref-screen.json and mixed-huge.json. Patch applies cleanly to4d711b1. Detached sf50-checked and sf50-checked-build and all frozen evidence remain. Full details in `mem:solver/experiments/50-checked-rref-results`, implementation/fixture failures in `mem:solver/experiments/50-checked-rref`.

Research artifacts and current handoff are recorded in the separate docs(custom): preserve checked-rref benchmark results commit; no rejected arithmetic enters production. Remaining original approved recommendations include pruning-order instrumentation and deferred-cache-compatible tail scheduling. No new implementation or benchmark started during this checkpoint.
