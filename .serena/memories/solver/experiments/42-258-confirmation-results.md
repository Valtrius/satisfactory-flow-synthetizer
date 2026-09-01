# 42. 258 confirmation results

Date: 2026-09-02. State: analyzed; no promotion.
Design: `mem:solver/experiments/42-258-confirmation`.
Earlier factorial: `mem:solver/experiments/42-complete-propagation-variable-set-results`.

## Verification

Follow-up run: `target/parallelism-ladder/registered-variable-258-confirmation-20260901`.
All ten jobs completed and verified, no caps or kills. 41.714 process minutes.
Frozen reanalysis reproduces summary SHA-256:
`89a73d666a70628b9d7107dc1531ae34f9022e41766a0e4aba82a2425ba6dea2`.

Combined frozen analysis verifies all 46 records across both screens. The independent
audit checks all 14 timing/diagnostic 258 results against the same full outcome,
preferred witness, two solution objects and key set at N=9/L=14. All 15 structural
counters agree. Four binary identities per screen and 560 source-file hashes pass.
The two diagnostics have identical 31 non-timing hotspot counters; each closes all
670 activity spans with zero drops or open spans.

## Three-sample timing result

Hotspots off only. Preserve every sample, including the slow reference.
Seconds; CPU is whole-process CPU time.

| Variant   | Wall samples, repeats 1/2/3 |  Median | Change vs reference | CPU median |
| --------- | --------------------------- | ------: | ------------------: | ---------: |
| Reference | 198.178 / 268.175 / 195.515 | 198.178 |            baseline |   2312.078 |
| Integer   | 177.753 / 253.530 / 252.431 | 252.431 |       27.38% slower |   2493.234 |
| Variables | 242.696 / 269.210 / 227.897 | 242.696 |       22.46% slower |   2531.953 |
| Both      | 190.015 / 260.134 / 251.305 | 251.305 |       26.81% slower |   2543.906 |

CPU medians rise 7.84% / 9.51% / 10.03%. First emitted witnesses still arrive almost
at completion. These whole-solve samples do not support promotion. Large overlap
and the 195.5-268.2s reference range mean the magnitude and cause of regression are
not isolated. Do not erase slow results or claim all variation is measurement noise.

The existing aggregate canonical medians are reference 735.357s, integer 679.640s,
variables 721.226s, both 679.721s. Algebra medians are 838.697 / 839.803 / 821.742 /
821.014s. These nested/aggregate elapsed timers cannot be subtracted from CPU to
identify the remainder.

## The tail is searching, not cleanup

Both diagnostics identify roots 7 and 23 in profile S2=5/S3=1/M2=0/M3=3 at N9/L14.
Both roots start within about 0.02s. Only those two remain during the last
108.48s of reference and 107.37s of variables.

| Root | Reference search | Variables search | Reference finish | Variables finish |
| ---- | ---------------: | ---------------: | ---------------: | ---------------: |
| 23   |         256.693s |         265.663s |           0.564s |           0.557s |
| 7    |         255.125s |         261.970s |           0.453s |           0.456s |

Across all roots, finish spans total 2.842/2.767 aggregate seconds. Witness
canonicalization totals only 0.011/0.010s. Root search owns the long critical path;
cache destruction and witness processing do not explain this measured tail.

## The local optimization does work

Same 7,964,026 sparse calls, 70,428,505 variable instances and 258,043,667 input-row
instances in both diagnostics:

| Cost                       | Reference | Variables | Reduction |
| -------------------------- | --------: | --------: | --------: |
| Variable collection        |   27.384s |    4.846s |    82.30% |
| Sparse preparation         |  132.776s |  111.377s |    16.12% |
| Sparse analysis            |  503.172s |  479.517s |     4.70% |
| Sparse forward elimination |  271.450s |  270.198s |     0.46% |

These are instrumented aggregate elapsed times, not whole completion gains. The
candidate removes its intended repeated work, but the completion decision remains
unresolved. In these whole diagnostics forward elimination is now about 54-56.3%
of sparse analysis; root-specific profiling is needed before selecting a new kernel.

## Decision and next work

Do not promote either candidate from these timings. Keep source unchanged while
isolating root 23, with root 7 as a control. Do not run another broad whole suite.

The user authorized extra work during an AFK period. Experiment 43 now prepares
all-four-variant root replays with benchmark-only CPU affinity controls and separate
diagnostics: `mem:solver/experiments/43-root258-affinity`.
CPU placement/clock/cache effects are hypotheses, not established causes.
The machine is an AMD Ryzen 9 7950X3D, 16 cores/32 logical processors. No assumption
about which logical CPU has which cache topology is used.

Reconstruct the original p1/32 target-128 plan before replay, freeze its 76 keys and
selected ordinal. Execute one root serially; do not replan with workers=1. Local
root exhaustion does not prove the whole problem. Production scheduler stays paused.

## Evidence and commit state

`analyze-confirmation.py` and `analysis/verified-report.json` in the follow-up root
preserve the audit and detailed root spans; `pooled-summary-rechecked.json` holds
the 46-record verification. Raw artifacts are ignored/local.
Candidate commits remain cd46fae/520b352; prior preparation 4933048. This analysis
does not change solver code or promote anything. Results not yet committed.
