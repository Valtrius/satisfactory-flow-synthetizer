# 46. RREF ordering and regression-control results

Date: 2026-09-02. State: analyzed; whole screen failed its completion requirement.
Design/source identity: `mem:solver/experiments/46-rref-order`.
RREF candidate a79ecf7 remains active but unpromoted. No source edit in analysis.

## Completion and verification

Run: target/parallelism-ladder/rref-order-hour-20260902.
Started 17:02:38, finished 17:39:06 Europe/Paris, about 36m29s, within one hour.
All 40 jobs attempted: 39 optimal completions, one clean deadline cancellation.
No crash, cleanup watchdog kill or exact-output mismatch observed.
BENCHMARK-FAILED.txt is authoritative; do not relabel this screen successful.

The frozen analyzer independently reproduces exactly one failure: reference-cohort
v258_ccd32-minimum_links-variables-p1-w16-r1 hit its 320s cap. A reference-cohort
cap is a real failed completion requirement; --allow-incomplete must not hide it.
All 977 frozen artifact hashes and topology/affinity checks pass.
Original and rechecked summary SHA256:
7e36e0f85414ef9a437c719c83d6efb2519211595f973ffa103ee92917269715.

All 19 fully completed pairs preserve exact proofs and 15 structural counters.
The 39 completed runs match exact full solution objects, preferred keys, layout
sets and optima across aliases in all four problem/mode groups.
The capped run reports Incomplete(Cancelled), deadline_fired=true, no witness,
zero layouts, exhausted N through 8, and no false optimal result.
It returned about 0.148s after its search cap, well inside the 15s cleanup grace.

Evidence: original results/summary.json, separately written
results/summary-rechecked-20260902.json and results/audit-20260902.json.
Recheck log target/exp46-reverification.log exits 1 as expected.
Audit script target/exp46-analysis.py preserves the failure and omits capped ratios.
Original raw results, frozen files and failure marker are unchanged.

## RREF candidate versus permanent accounting

All 20 RREF comparison jobs completed. Capacity 1200, p1, hotspots off.
Negative percentages mean lower time. Values are medians of within-pair ratios,
not ratios of the separately computed variant medians.

| Workload    | Placement/workers | Pairs | Wall change | Exploratory 95% interval | CPU change | First valid |
| ----------- | ----------------- | ----: | ----------: | ------------------------ | ---------: | ----------: |
| 115 all     | CCD96 / 16        |     4 |      -0.44% | -6.42 to +4.63%          |     -0.59% |      -3.23% |
| 238 optimal | CCD32 / 16        |     4 |      -1.68% | -2.09 to +0.15%          |     -0.91% |      -1.68% |
| 36 optimal  | unrestricted / 32 |     2 |      -0.83% | -2.18 to +0.53%          |     -2.85% |      -0.83% |

Wall is lower in 6/10 pairs, CPU in all 10. All three paired wall medians favor
the shortcut, but every wall interval crosses zero. With two/four pairs these
bootstrap intervals are exploratory, not strong confirmation.
115 individual wall changes are +4.63%, +0.24%, -1.12%, -6.42%, despite each
pair using identical work. Its first-valid interval also crosses zero widely.
Do not present the -3.23% first-valid median as an established witness speedup.
No RREF/258 comparison was included. The 258 failure uses variables, not rref.

Recommendation: retain RREF as a promising experimental candidate and perform one
focused confirmation, including a completed 258 comparison if the time budget allows.
Do not make permanent yet on these noisy, small samples or on CPU savings alone.
Avoid introducing another independent calculation candidate before this check.

## Variable-discovery controls

These compare frozen before and variables binaries, without accounting or RREF.
Variable discovery remains permanent in 520b352; these are regression checks.

| Workload             | Placement         | Complete pairs |         Wall change |                    CPU change |
| -------------------- | ----------------- | -------------: | ------------------: | ----------------------------: |
| 115 all              | unrestricted / 32 |              2 |              -2.08% |                        -1.22% |
| 36 optimal           | CCD32 / 16        |              2 |              -1.32% |                        -1.43% |
| 258 minimum_links r2 | CCD32 / 16        |              1 |              -0.64% |                        -0.45% |
| 258 minimum_links r1 | CCD32 / 16        |              0 | no completion ratio | no comparable full-work ratio |

258 r2: reference 301.500s, variables 299.578s, identical two layouts at N9/L14.
258 r1: variables did not finish before the 320s deadline; reference completed
in 302.350s. Variables observed return time 320.148s includes cancellation cleanup,
not completion. Keep the adverse observation; do not average only the survivor.
The two-pair 258 cell is inconclusive and has no overall completion median/speedup.

There is still no demonstrated universal variable-discovery win on CCD32/258.
Do not reverse the broader promotion solely from this sample, but treat the timeout
as unresolved adverse evidence. Repeat this exact workload with a larger per-job
cap while reducing other jobs to preserve the user's one-hour total limit.
Placement was verified; boost, thermal and external-load causes were not measured.
Do not blame background load or normalize results by CPU speed without evidence.

## Accounting controls

These compare frozen variables and accounting, without the RREF shortcut.
Accounting remains permanent in 331b87a. Each cell has only two pairs.

| Workload   | Placement/workers | Wall change | CPU change | First valid |
| ---------- | ----------------- | ----------: | ---------: | ----------: |
| 115 all    | unrestricted / 32 |      +0.54% |     -0.18% |      -0.47% |
| 36 optimal | unrestricted / 32 |      -1.06% |     -1.36% |      -1.06% |

115 is slower in both completion pairs, +0.16% and +0.92%. This does not support
an unrestricted completion gain, even though first-valid improves in both.
36 has one faster and one slightly slower completion pair.
Do not pool these with the earlier 16-worker CCD results or claim uniform benefit.
Keep the previous promotion while retaining this minor adverse control for the next
useful screen. No accounting/unrestricted258 coverage yet.

## Decision and next step

No solver/default changes, new benchmark, commit or push during analysis.
RREF remains candidate a79ecf7. Prior promotions remain 520b352 and 331b87a,
documented by 6812173. Benchmark manifest commit f6f422c.
Scheduler extras stay paused; no production affinity or cyclicity gate.
A future focused confirmation must remain within one hour including cleanup.
Prioritize RREF 115/238 plus completed 258 work and the capped variable control,
rather than repeating the full matrix. Results/handoff notes are uncommitted.
