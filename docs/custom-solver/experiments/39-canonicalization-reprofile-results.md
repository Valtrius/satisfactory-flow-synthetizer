# 39. Canonicalization reprofile results

Date: 2026-09-01. State: completed and verified.
Related: [experiment design](39-canonicalization-reprofile.md).

## Verification

The frozen analyzer verifies all four records. The 115 and 238 controls complete
optimally and match available exact references. Hard 36 and hard 10 cancel cleanly at
their declared 60-second caps. Hard 36 retains eight validated layouts; hard 10 retains
no witness. No job fails or reaches the cleanup watchdog.

Run: `target/parallelism-ladder/canonicalization-reprofile-20260901`.
`profile_case.exe` SHA-256:
`6618B0B55768D9E275A82BA0446EA7DE7F2799F21CD26CE390899C07AC022CA1`.
Verified `summary.json` SHA-256:
`4ED2748992B0A70473C59D3A2C540E83DBE07B680F7C52E964C7C2C1F286095E`.

## Purpose distribution

Times are aggregate worker seconds. Open and marked percentages use total non-SCC
graph-canonicalization time.

| Workload          | State graph | Open port | Marked link | Open + marked share | SCC canonicalization |
| ----------------- | ----------: | --------: | ----------: | ------------------: | -------------------: |
| 115 all, complete |   163.350 s |   0.010 s |     0.037 s |              0.028% |             51.123 s |
| 238 all, complete |    55.799 s |   0.001 s |     0.004 s |              0.010% |             20.154 s |
| hard 36 all, cap  |   568.252 s |   0.062 s |     0.155 s |              0.038% |             29.069 s |
| hard 10 opt, cap  |   572.664 s |   0.001 s |     0.003 s |              0.001% |            300.113 s |

State keys account for 99.96-100.00% of graph-canonicalization time and calls.
Experiment 24 had put open-port plus marked-child keys at 94.5-96.2% of legal-decision
time. The marked-child bypass, state-coordinate reuse, and deferred state-key changes
have removed that target from current production work.

## Subphases and top-level share

| Workload          |  Equality | Inequality |  Labeling | Equality + inequality / canonicalization | Canonicalization / accounted | Propagation / accounted |
| ----------------- | --------: | ---------: | --------: | ---------------------------------------: | ---------------------------: | ----------------------: |
| 115 all, complete |  60.293 s |   79.741 s |  27.253 s |                                    64.8% |                        38.9% |                   53.2% |
| 238 all, complete |  20.093 s |   29.908 s |  10.739 s |                                    65.4% |                        51.1% |                   41.7% |
| hard 36 all, cap  | 157.470 s |  246.459 s |  82.969 s |                                    67.2% |                        50.4% |                   41.4% |
| hard 10 opt, cap  | 205.436 s |  312.469 s | 125.141 s |                                    58.9% |                        59.6% |                   34.1% |

Canonicalization remains the larger combined bucket on 238 and both hard cases.
Propagation leads on 115. Exact equality RREF and physical-bound inequality encoding,
not graph labeling, now consume most canonicalization time. Inequality work is the
largest individual canonical subphase in every case.

These timers overlap according to the documented recorder hierarchy and contain
hotspot atomic overhead. The capped rows establish cost shares, not completion speed.

## Decision

Stop pursuing open-port and marked-child canonical keys. Do not optimize graph labeling
next. Add a diagnostic-only split inside `primitive_equality_basis` and
`primitive_inequality_basis`:

- equality port/index construction, dense row assembly, and rational RREF;
- inequality pivot indexing, row construction, positive-scale normalization, and
  sort/deduplication.

Use that split to choose between sparse/integer canonical rows and a smaller local
normalization change. Any candidate must preserve state and SCC key bytes across the
existing exhaustive relabeling, rational, and Reference oracles. Scheduling stays
paused.
