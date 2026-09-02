# 48. Negative-unit confirmation results

Date: 2026-09-02. Completed and independently verified.
Protocol and source identities: `mem:solver/experiments/48-rref-negative-unit-confirmation`.
Initial results: `mem:solver/experiments/48-rref-elimination-shortcuts-results`.

## Verification

Run `target/parallelism-ladder/rref-negative-unit-confirmation-20260902` finished
22:05:15.996 Europe/Paris, about 36m02 after launch. Within the one-hour budget.
All 32 reference jobs completed optimally. No cap, kill, invalid result or failure.
Frozen analyzer rerun passes; original/rechecked summary SHA256:
`2d724ba0ec39a0c3d5d9d0fa946edf269345b684b5614af15b47f6184084d5e5`.

Independent audit verifies all 191 run artifact hashes, exact frozen schedule,
16 adjacent matched pairs and two AB/two BA pairs in each workload.
Every job has hotspots OFF and verified affinity before resume. Affinity application
0.0073174 to 0.0360451s. Full solutions, keys, preferred witnesses, complete outcomes
and proofs match. All 15 checked structural counters match within pairs.

Results unchanged: 115 all 49 layouts at N7/L10; 238 optimal one at N8/L13;
258 minimum_links two at N9/L14; 36 optimal one at N9/L11.
At analysis time, main canonical.rs/solver.rs production prefixes still equaled frozen before.

Secondary cross-screen audit checks all initial frozen hashes too, verifies identical
before/minus executable hashes and settings, and preserves screen identity.
All 48 negative-unit comparison records, 24 pairs across both screens, have matching
exact outputs/proofs and structural counters for each problem/mode.

## Primary results: four new pairs per workload

Percentages are median matched candidate/reference changes. Negative is faster.
No pooling across workloads or CPU placements.

| Workload / placement                 | Completion | First witness | Process CPU | Wall pairs improved |
| ------------------------------------ | ---------: | ------------: | ----------: | ------------------: |
| 115 all, CCD96, 16 workers           |     -1.59% |        -4.44% |      -1.60% |                 4/4 |
| 238 optimal, CCD32, 16 workers       |     -2.05% |        -2.05% |      -2.07% |                 3/4 |
| 258 minimum_links, CCD96, 16 workers |     -1.96% |        -1.96% |      -1.49% |                 4/4 |
| 36 optimal, unrestricted, 32 workers |     +0.39% |        +0.40% |      -2.50% |                 1/4 |

Individual paired completion changes, in saved audit order:

- 115: -0.99%, -0.02%, -2.82%, -2.18%.
- 238: -1.66%, +1.13%, -2.45%, -4.03%.
- 258: -3.73%, -1.77%, -2.15%, -0.93%.
- 36: -0.30%, +4.70%, +0.19%, +0.59%.

258 reference times 202.765, 198.965, 196.077, 196.899s;
matched candidates 195.209, 195.440, 191.862, 195.076s.
All four long completed comparisons improve by 1.82 to 7.56s.
36 adverse pair: 13.411 -> 14.042s, +4.70%, while process CPU falls 3.32%.
Do not discard it or attribute it to placement/background activity without evidence.
238 has one +1.13% wall pair and one +0.30% CPU pair. These are not the same pair.

New CPU improves 15/16 pairs, completion 12/16. 115 first witness improves 4/4
new pairs, but its earlier +2.11% sample remains part of the evidence.
Sampled peak-memory paired medians: 115 -3.22%, 238 -2.78%, 258 +0.76%, 36 -0.44%.
Coarse samples do not prove an allocation effect; memory is not a promotion gate.

## Secondary results: six pairs per workload across both screens

Only within-case, same-affinity, same-binary pair ratios are combined.
New confirmation above remains primary.

| Workload          | Initial wall median | Confirmation wall median | Six-pair wall median | Six-pair wall improvements |
| ----------------- | ------------------: | -----------------------: | -------------------: | -------------------------: |
| 115 all           |              -1.16% |                   -1.59% |               -1.16% |                        6/6 |
| 238 optimal       |              -0.65% |                   -2.05% |               -1.42% |                        5/6 |
| 258 minimum_links |      effectively 0% |                   -1.96% |               -1.35% |                        5/6 |
| 36 optimal        |              -1.14% |                   +0.39% |               -0.05% |                        3/6 |

Across all cells, 19/24 wall pairs and 23/24 CPU pairs improve. These counts are
descriptive, not a pooled effect size or independence/significance claim.
Six-pair CPU medians in table order: -1.52%, -2.42%, -1.40%, -1.69%.
115 first-witness median -3.59%, improved 5/6; other modes track completion.

Exploratory exact empirical percentile-bootstrap intervals for each six-pair wall
median: 115 [-2.50%, -0.50%], 238 [-3.24%, +0.50%],
258 [-2.94%, effectively 0%], 36 [-1.14%, +2.65%].
These small-sample intervals are not a universal guarantee. In particular, do not
claim demonstrated hard36 completion improvement or a strong 238 significance result.

## Recommendation

Recommend permanent, unconditional retention of the isolated negative-unit shortcut.
Repeated completed 115 gains, the four new long 258 gains, favorable 238 direction
and CPU reductions support this small exact arithmetic change. This is a practical
promotion recommendation, not a claim that every workload is faster.
Hard36 is unresolved/effectively unchanged across screens; preserve its +4.70% pair.
Retain arbitrary-size arithmetic, exact canonical ordering and diagnostic parity.
No case-name, cyclicity, N, affinity or memory condition in the production change.

Promote only the arithmetic hunks in canonical.rs and its diagnostic twin.
The standalone patch also contains tests already in main; do not apply it blindly.
Re-run relevant exact tests/Clippy after integration and include this evidence in
the separate conventional commit. No promotion or source modification in this
analysis turn.

Keep zero-destination held. Its initial mixed 115/258 evidence, including +10.62%
on one 258 pair, is unchanged. Do not combine it implicitly with the recommended
candidate. Any later zero test must identify whether it measures the isolated
shortcut or its incremental effect on the promoted baseline.
Scheduler extras remain paused. Any further benchmark stays <=1h including cleanup.

## Evidence and commit state

New evidence under the run's results/: `summary-rechecked-20260902.json`,
`audit-20260902.json`, `combined-negative-audit-20260902.json`.
Helpers/logs: `target/exp48-negative-confirmation-analysis.py`,
`target/exp48-negative-confirmation-analysis.log`,
`target/exp48-negative-confirmation-reverification.log`,
`target/exp48-negative-combined-analysis.py`,
`target/exp48-negative-combined-analysis.log`.

The combined helper's first draft floored percentile indices, truncating the upper
bound for its two-pair initial cells. Corrected to inverse empirical CDF using
ceil(p*N)-1 before reporting. Original draft JSON/log retained as rank-floor-v1.
No medians, timings, outcomes or raw/frozen evidence changed.

Preparation/initial results/confirmation manifest committed 3b32c51; candidates
and shared tests fadb316. Analysis-time unrelated HEAD edb4f0b and other user commits
preserved. Neither arithmetic shortcut is in production yet. Result/handoff notes
uncommitted; no new commit, push, build, test suite or run during this analysis.
Nothing in flight.

## Subsequent promotion

User approved permanent integration after this analysis. Source and diagnostic
arithmetic now match the tested candidate; fresh release tests and strict Clippy
pass. The analysis-time source state above is historical.
`mem:solver/experiments/48-negative-unit-promotion` records integration and commit.
