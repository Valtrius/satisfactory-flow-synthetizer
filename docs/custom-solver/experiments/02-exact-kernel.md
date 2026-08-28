# 02. Cheaper exact calculations and both solve modes

Date: 2026-08-27/28. State: accepted and permanent, commit `319191e`, published.
Hypothesis: avoid unnecessary canonicalization and zero/unit arithmetic while
retaining exactly the same selected decisions, normalized systems and search coverage.

## Changes

- Canonical MRV filters inexpensive ranking fields first, computing canonical
  port keys only for tied finalists. Orbit choice and raw-reference tie-break stay fixed.
- Exact RREF borrows nonzero pivot suffixes; zero, unit-pivot and unit-factor work
  is skipped. Inequality reduction and normalization avoid zero-coefficient work.
- Differential tests retain eager/dense reference calculations, including fractions,
  contradictions and large integers. No approximate arithmetic or extra shared state.
- Benchmarks call actual `solve_with_observer` and `enumerate_with_observer` paths.
  Added tiny-case overhead checks, p14/p124, preserved before binaries and exact
  mode-specific verification. Kernel improvements have no production opt-out flag.

## Preliminary diagnostics

One instrumented sample per case/mode/variant, one worker, baseline scheduling:

| Case | Mode    | Before s | After s |
| ---- | ------- | -------: | ------: |
| 65   | all     |  165.650 |  78.444 |
| 24   | all     |  175.988 |  72.111 |
| 65   | optimal |    5.618 |   2.545 |
| 24   | optimal |   10.563 |   4.882 |

These eight samples motivated the full rerun. They are not interchangeable with
the uninstrumented medians below. Complete-state evaluation still cost about 20 s
in the 65 serial diagnostic; that broad bucket required decomposition before another fix.

## Full rerun

792 fresh-process records, all verified: 396 all and 396 optimal, three samples per
configuration, three cases, workers 1/4/16/32, nine after-change stages and preserved
before baseline/full-stack. Randomized order, hotspots off, 3.61 measured process hours.

One-worker baseline medians isolate the kernel:

| Mode    | Case | Before s | After s | Speedup |
| ------- | ---- | -------: | ------: | ------: |
| all     | 65   |   141.16 |   78.17 |   1.81x |
| all     | 24   |   155.88 |   71.05 |   2.19x |
| optimal | 65   |     5.87 |    2.48 |   2.36x |
| optimal | 24   |    11.27 |    4.83 |   2.33x |

Baseline search-state and decision counts stayed identical. Serial process CPU
fell about 45-57%, supporting cheaper computation rather than less search coverage.

Selected after-change medians at 32 workers, seconds:

| Mode    | Stage    |    65 |    24 |
| ------- | -------- | ----: | ----: |
| all     | baseline | 31.70 | 33.33 |
| all     | groups   | 12.41 | 13.32 |
| all     | p14      | 15.19 | 10.10 |
| all     | p1234    | 15.53 |  9.63 |
| optimal | baseline |  1.35 |  2.75 |
| optimal | p1       |  0.49 |  1.88 |
| optimal | p12      |  0.40 |  1.88 |
| optimal | p1234    |  0.44 |  1.88 |

Groups used 98/101 CPU-seconds versus p14's 219/118, with 401/432 MiB peak memory.
p14 used 285/221 MiB, versus full-stack 724 MiB on 24, but was slower on 65.
Sharing saved about 96 ms on 65 optimal and did not improve 24 optimal.

## What did not generalize

At four workers, 65 all/groups regressed from 32.66 to 48.38 s. At 32 workers,
tiny all/full-stack took 26.65 ms versus baseline 7.01 ms; tiny optimal took
9.68 versus 1.53 ms. Donation and sharing were not universal improvements.
Several timing ranges overlapped. These two hard shapes, one tiny shape and one
machine did not justify automatic defaults or a universal worker threshold.

## Decision, validation and evidence

Keep kernel changes permanent. Keep scheduler flags opt-in, use groups and p1 as
comparison candidates, leave donation off. Prefer elapsed completion within the
resource budget; do not choose a slower option solely because it has lower overhead.

All mode-specific keys, preferred witnesses, objectives and independent validation
matched. Historical validation: 287 Rust tests passed, five ignored, strict Clippy,
88 frontend tests, frontend checks, formatting and four analyzer tests passed.

Local evidence: `target/parallelism-ladder/kernel-full-suite/`, particularly
`results.csv`, `summary.json`, `summary-rechecked.json`, `schedule.json` and hashes;
`kernel-before-bin/`, `kernel-after-bin/`, corresponding source snapshots, and
`kernel-diagnostics-before/after/`. The old tiny capacity was 5, later changed to 1200.
Follow-up: [03, harder inputs](03-hard-case-screen.md).
