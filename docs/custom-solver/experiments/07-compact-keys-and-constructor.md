# 07. Compact exact keys and integer constructor

Date: 2026-08-28. State: analyzed; promising uncommitted candidates, repeats pending.
Hypothesis: fewer retained key bytes and cheaper exact subset calculations reduce
memory, destruction time and serial construction without changing solver semantics.

## Implementation

- State/SCC keys use exact-sized boxed bytes. Semantic rows carry a versioned
  sparse payload: variable/row counts, nonzero indices and exact binary rationals.
  Zero is implicit; units use short tags; arbitrary-size integers remain exact.
  Equality compares full bytes. Public witness/marked keys and exact algebra stay fixed.
- Constructor subset sums scale leaves and target to one integer denominator,
  update only changed mask bits, allocate leaf selections only for matching sums,
  and test the full remaining set for the final output. Numeric mask order is retained.
- Each optional helper attempt has a cooperative five-second budget. Expiry is
  deferred separately from completed misses within N; exact search remains the fallback.
- A separate five-second heartbeat uses a preopened file and fixed buffer, without
  the trace mutex/JSON allocation. Both diagnostic threads still join before return.
  Timers are labeled summed elapsed time, not CPU.

Regression coverage reconstructs legacy coefficient streams from compact rows,
including wide indices and large signed rationals; compares ordered subset matches
with direct rational sums; and distinguishes deferred, completed and cancelled helper work.

## Screen and results

Manifest `benchmarks/custom/compact-screening.json`: 12 timing jobs with hotspots
off and four shorter diagnostic jobs with hotspots on, all 32 workers/rate 1200.
All 16 verified: ten optimal, six cooperative capped incomplete, zero watchdog kills.
All ten optima matched earlier full saved solutions. Partial 36 sets retained the
same two known keys, without proving enumeration. Diagnostic traces dropped no records.

Single-sample seconds. Recovery was instrumented; current timing was not:

| Case/mode  | Stage    | Recovery | Current timing |
| ---------- | -------- | -------: | -------------: |
| 24 optimal | baseline |    1.916 |          1.551 |
| 24 optimal | p1       |    1.073 |          0.812 |
| 24 all     | baseline |   21.379 |         22.441 |
| 24 all     | groups   |   11.946 |         10.072 |
| 65 optimal | baseline |    1.354 |          1.169 |
| 65 optimal | p1       |    0.445 |          0.420 |
| 65 all     | baseline |   24.406 |         20.564 |
| 65 all     | groups   |   11.158 |         10.803 |
| 36 optimal | baseline |  244.974 |         99.841 |

The instrumented 36 optimal run took 94.122 s and 6.516 s total construction across
eight attempts, versus recovery's 143.962 s, about 22x less constructor time.
Zero budget expiries occurred. The faster instrumented sample shows variability,
not that profiling improves speed. The 24 all baseline's observed 5% increase
requires matched repeated timing before calling it a regression.

Previously killed jobs now returned explicitly incomplete:

| Job                 | Cap s | Return s | First witness s | Previous/current peak GiB |
| ------------------- | ----: | -------: | --------------: | ------------------------: |
| 36 all groups       |   600 |  600.220 |          97.005 |               8.81 / 1.60 |
| 10 optimal baseline |   180 |  180.101 |            None |               6.68 / 0.60 |
| 10 optimal p1       |   180 |  183.335 |            None |              11.55 / 4.24 |

Neither 10 job found a witness. P1 averaged about 29.4 CPU-core equivalents versus
baseline's 3.0, but this did not demonstrate shorter completion. 36 all remained
at two partial layouts. The shorter 180 s diagnostic already visited 22.9 million witness leaves.

Short diagnostic cancellation-to-return delays: 10 baseline 0.036 s, 10 p1 0.771 s,
36 all/groups 0.201 s. Their largest completed state-cache drops were respectively
0.016/0.488/0.198 s. These shorter runs do not substitute for long-run measurements;
nominal cap overshoot is not the exact request-to-return latency.

## Conclusion, limitations and next step

Strong evidence for the bundle's memory/cancellation improvement and faster hard
optimal completion. Do not attribute the whole gain to one change. The five-second
policy's usefulness is unmeasured because it never fired. No allocator, cache
eviction, cleanup-ownership or scheduling-default change was made.

Repeat shorter comparisons with both binaries uninstrumented; test p1 on the
faster 36 path and compare 16/32 workers. Keep candidates separate from the already
committed witness refinement. Future witness pruning must preserve the exact minimum.

Local evidence: `target/parallelism-ladder/compact-20260828/`, `results/summary.json`,
`results/summary-strong-verification.json`, `compact-analysis.json`, full results, hashes,
and `solver-source/`. The old report's push-blocked note was superseded by user publication.
Follow-up: [08, repeated timing and worker counts](08-repeat-scheduling.md).
