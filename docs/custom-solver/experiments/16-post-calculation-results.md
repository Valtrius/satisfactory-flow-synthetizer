# 16 - Post-calculation results

Date: 2026-08-28. [Protocol](16-post-calculation-screen.md),
[diagnostics and next steps](16-diagnostics-and-next-steps.md).
All 38 jobs ran in 35.126 process minutes. No watchdog kills or solver failures.
The launcher reported failure because its final analyzer required a baseline-stage
reference. Corrected verification passes all records without rerunning searches.

## Verification correction

Run root: `target/parallelism-ladder/post-calculation-20260828/`.
The original `BENCHMARK-FAILED.txt`, logs, scripts and summaries remain unchanged.
Frozen prefix verification passes 26/26 and reproduces `summary.json` exactly.
Frozen whole verification reproduces six missing-baseline failures exactly.

The optimal comparison deliberately holds p1 fixed for preserved before/after
binaries. The analyzer accepted only baseline scheduling, or a completed stress
case, as its reference. These six completed optimal jobs belong to the reference
cohort. `scripts/analyze-parallelism.py` now also accepts a completed `before`
variant as the exact-result reference. Timing ratios still require matching stages
and settings. An after-only nonbaseline reference cohort still fails.

The corrected whole summary passes 12/12. Its statistics equal the original
summary's statistics; only the failures change. Schedule identities, frozen binary
hashes, source hashes, exact inputs/budgets and result/proof checks pass. Current
solver sources match the frozen combined sources after newline normalization.
All shared full solution objects agree across modes, variants and scheduling policies.

Evidence: `summary-rechecked.json`, `whole-results/summary-frozen-rechecked.json`,
`whole-results/summary-corrected.json`, `whole-verification-corrected.log`,
`analysis-rechecked.json`, `analyze-rechecked.py` and `verification-correction.json`.
The corrected analyzer copy/diff is retained beside the frozen original.
32 Python tooling tests pass, including the new reference-selection regression
and rejection tests. Log: `target/post-calculation-verifier-tests.log`.
This follow-up changes tooling and docs only. No Rust changes, new benchmark,
commit or push. Earlier Rust validation remains in the protocol.

## Repeated hard-36 optimal completion

Actual optimal API, p1, 32 workers, capacity 1200, N<=9, instrumentation off.
Three fresh processes per variant. Before is the preserved `1b558a6` solver;
after contains the three promoted calculation changes.

| Measure                     | Before        | Combined      |
| --------------------------- | ------------- | ------------- |
| Median completion, seconds  | 32.129        | 24.652        |
| Completion range, seconds   | 32.003-32.373 | 24.029-25.065 |
| Median process CPU, seconds | 383.500       | 358.297       |
| Largest sampled peak, MiB   | 99.7          | 100.9         |

Completion is 23.27% shorter, or 1.303x. All six runs return the same full preferred
solution at N=9/L=11 and the same proof. Each exhausts through N=8, with 19 link
groups, 41 profiles and 1,835 proof roots. Retained states and structural decisions
also agree. This confirms experiment 13's single hard-optimal observation and
supports keeping the calculation changes. It does not isolate their individual
contributions or show a universal memory reduction.

## Hard-36 full enumeration, p1 versus p14

Combined only, 32 workers, capacity 1200, N<=9, instrumentation off, two repeats
per policy and a 240 s cap. All four return incomplete cancellation.

| Measure                                    | p1              | p14             |
| ------------------------------------------ | --------------- | --------------- |
| Median first validated witness, seconds    | 24.485          | 24.616          |
| First-witness range, seconds               | 24.367-24.604   | 24.525-24.708   |
| Observed wall range, seconds               | 240.148-240.149 | 240.222-240.237 |
| Median process CPU, seconds                | 4,211.734       | 4,848.523       |
| Sampled peak range, MiB                    | 491.9-493.4     | 699.4-727.9     |
| Partial witnesses, both repeats            | 9               | 2               |
| Folded exhausted groups / profiles / roots | 21 / 50 / 2,437 | 20 / 45 / 2,029 |
| Earliest unfinished ordered group          | N=9/L=13        | N=9/L=12        |

Each policy reproduces its exact partial solution map. P14's two L=11 witnesses
are an exact subset of p1's nine. P1 has two at L=11, six at L=12 and one at L=13.
Its six L=12 full solution objects equal experiment 13's stored completed group.
Neither policy exhausts N=9 or completes enumeration.

P1 uses about 13.1% less process CPU than p14 at the same cap and less sampled
memory, with similar first-witness latency. It closes L=12 in both repeats.
These observations favor p1 for the next controlled test, not as a proven
full-enumeration speedup or a universal production default. P14 concurrently works
on later groups; its earliest unfinished group and visible result count do not
describe all work already performed. See the separate trace analysis.

## Decision

Keep the three calculation changes permanent. Do not promote p14 or change
scheduler defaults. Prioritize the optional constructor work identified after the
winning group, then a limited sharing/donation comparison on fixed hard work.
The existing difficult cases suffice. All 24 hard-10 prefixes capped, so none is
yet a completed control. Refine discovery before repeating a comparison matrix.
