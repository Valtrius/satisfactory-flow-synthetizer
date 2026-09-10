# Hybrid promotion: accepted performance tradeoff

[Full HTML analysis with every matched pair](https://copyparty.jakez.eu/agent-files/lKbE7J4h2.html?k=fiN9fiLplV2nT-Fs).

Promoted after explicit user approval on 10 September 2026. Keep descending
first-output order and static Boolean partitions, and add the exact tested
adaptive fallback for All min N/L. The user accepts the measured case-97
uncertainty in exchange for the confirmed case-258 improvement at 16 workers.
The timeout remains adverse evidence; it is not established to be a fluke.
Scheduling experiments are paused after integration.

The linked HTML preserves the initial conservative recommendation before this
approval. The measurements are unchanged; this record states the final decision.

Seven suites finished in 1 h 29 m 40 s: 48 timing runs, 24 matched pairs,
47 complete and one capped hybrid run. Baseline completed 24/24; hybrid 23/24.
Fresh verification checked all 1,041 frozen hashes, reran seven result verifiers,
compared full canonical keys and saved solution objects across six enumeration
scopes, and reaudited all 14 separate qualification probes (12 complete, two
intentional cancellations). There were no verification failures. At analysis time, the nine production solver source files matched the frozen
static baseline. Promotion imports the tested hybrid source instead.

Times below are complete-run medians; percentages are median matched-pair
changes. No candidate median or speedup ratio is calculated across a capped run.

| Case / scope       | Workers | Pairs | Baseline median |    Hybrid median / outcome |       Paired change |
| ------------------ | ------: | ----: | --------------: | -------------------------: | ------------------: |
| 258 Â· All min N/L |       8 |     2 |         38.97 s |                    39.27 s |              +0.75% |
| 258 Â· All min N/L |      16 |     6 |         38.52 s |                    22.83 s |             -40.81% |
| 258 Â· All min N/L |      32 |     2 |         11.47 s |                    11.83 s |              +3.13% |
| 97 Â· All min N/L  |       8 |     2 |        579.37 s | 478.91 s / capped at 600 s | Unmeasured: one cap |
| 97 Â· All min N/L  |      16 |     2 |        436.28 s |                   456.63 s |              +5.03% |
| 97 Â· All min N/L  |      32 |     2 |        170.47 s |                   169.85 s |              -0.37% |
| 24 Â· All min N/L  |      32 |     2 |          2.01 s |                     2.07 s |              +2.85% |
| 36 Â· All min N/L  |      32 |     2 |          6.64 s |                     6.49 s |              -2.27% |
| 65 Â· All min N/L  |      32 |     2 |          0.49 s |                     0.48 s |              -1.64% |
| 36 Â· All min N    |      32 |     2 |          5.74 s |                     5.73 s |              -0.01% |

Case 258 at 16 workers wins all six pairs, saving 15.09â€“15.84 seconds
(39.5â€“41.1%). Its paired median saving is 15.72 seconds. This is credible
evidence of a specific improvement, even though it does not settle the overall
production recommendation. The eight-worker improvement from the preceding
global adaptive campaign did not reproduce in this hybrid comparison.

Case 97 at 16 workers is mixed: 408.06 -> 451.38 s (+43.32 s) in one pair,
464.49 -> 461.88 s (-2.62 s) in the other. The descriptive paired median is
+5.03%, but two pairs cannot establish a stable regression percentage.

Case 97 at eight workers is also mixed. One pair improves 593.53 -> 478.91 s
(-114.62 s). The other baseline completes in 565.21 s while hybrid reaches the
600-second cap. Its eventual completion time is unknown and would exceed that
baseline by more than 34.79 seconds. Do not average the cap with the completed
candidate or discard either pair.

The capped run found all 813 canonical layouts and full saved witnesses seen in
completed runs. It remained in the N=9/L=15 search group and correctly reported
incomplete enumeration. Finding the known set does not discharge the solver's
proof obligations. There is no observed wrong solution or false optimality
claim. Per-root timing traces were disabled, so this batch does not identify the
specific slow root or explain the variance.

The 32-worker hard comparisons are effectively preserved: +0.36 s on 258,
-0.63 s on 97. Easy guard changes are tiny (largest individual penalty 0.065 s)
and would be acceptable under the user's tolerance. They do not drive this
decision; the unresolved hard-case tradeoff does. No performance conclusion
comes from fewer than eight workers or parent-process-only CPU counters.

Baseline times differ materially from preceding campaigns; compare only matched
pairs within this batch. The source baseline is the same, but the cause of the
between-session timing shift is unmeasured. Never credit it to the candidate.

The hybrid source and qualified contracts were preserved in `2dccf50` and the
frozen preparation. Integration copies all nine solver source files exactly,
keeps the proof/cancellation contracts, and makes their tracing test independent
of ambient diagnostic settings. No case-specific dispatch, new worker threshold
or additional performance policy is introduced.

The earlier recommendation to park this candidate was superseded by the user's
explicit acceptance of the tradeoff. The evidence does not prove a universal
speedup, nor a stable case-97 regression. Retain both conclusions when discussing
the promotion. No additional scheduling experiment is queued.

Evidence: target/optimization-hybrid-20260910/frozen and /analysis.
Recheck saved artifacts with:
`python target/optimization-hybrid-20260910/analysis/analyze_hybrid.py`.
The analysis launched no new timing solves. Promotion runs integration and
release checks, not another benchmark campaign.

## Integration verification

All nine integrated solver sources match the qualified hybrid, allowing only
line-ending normalization. Workspace validation passed 140 Rust tests, strict
Clippy on all targets, 100 frontend tests, Svelte checks with no errors or
warnings, and 33 maintained Python harness tests. The diagnostic contract also
passed separately without ambient tracing configuration.

The desktop release build, installer and portable ZIP passed packaging checks:
exact solving through the production API with external backend lookup disabled,
missing-backend failure controls, and application/backend/notices hashes. These
are local verification packages with version 0.2.0, which is already tagged;
select the next version before publishing. Receipts are in
`target/release-preparation-20260910/VALIDATION.json`. No timing batch was rerun.
