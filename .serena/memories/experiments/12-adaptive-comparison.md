# Adaptive comparison against promoted partitions

Baseline 6852be9: descending first-output order and static Boolean All min N/L partitions. Both global adaptive replacements are rejected for production. Preserve the baseline; see `mem:experiments/11-partition-promotion`.

Completed 2026-09-10: 56/56 exact terminal results, 28 matched pairs, ten suites, 51m27s; no caps or verifier failures. All 1,485 frozen hashes rechecked, all ten verifiers rerun, full canonical layout keys and saved solution objects equal for all five All min N/L scopes. Fourteen release qualification probes include two deliberate cancellations. Timings exclude these probes.

Case 258, immediate adaptive: paired median changes -24.74% at 8 workers (59.87 -> 45.09 s), -50.89% at 16 (60.91 -> 29.88 s), +38.07% at 32 (14.16 -> 19.55 s). Grace250: -18.78%, -50.92%, +36.02% respectively, against its own matched baselines. Case 97 at 32: immediate +44.56% (215.43 -> 311.45 s); grace +45.87% (201.98 -> 293.86 s). All four case-97 adverse pairs add about 90-100 seconds. Every two-pair group agrees in direction. Easier 24/36/65 gains are at most about three seconds and cannot offset the hard regression. Earlier witnesses do not justify slower exact termination.

Evidence: benchmarks/optimization/results-adaptive-20260910.md; target/optimization-adaptive-20260910/{frozen,analysis}; preparation target/optimization-adaptive-prep-20260910. Shared HTML https://copyparty.jakez.eu/agent-files/JCal8u_vn.html?k=p6NNayy0JEnmlo0i. Do not pool baselines or infer grace beats immediate from different pairs.

User approved one final hybrid trial, then a pause: preserve static splitting wherever it activates, adapt otherwise. See `mem:experiments/13-hybrid-final`. Broad scheduling sweeps should end. Case 10 All min N/L remains a separate SMT-proof target; current combined policy has not been freshly timed on that scope. Prefer substantial exact hard-case savings, tolerate reasonable easy penalties up to about ten seconds, performance workers >=8, session <=3 hours.
