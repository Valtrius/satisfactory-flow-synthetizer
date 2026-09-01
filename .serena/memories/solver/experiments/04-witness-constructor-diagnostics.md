# 04. Witness, constructor and cancellation diagnostics

Date: 2026-08-28. State: analyzed. Hypothesis: the long witness phase and open
root intervals explain slow completion and cancellation better than scheduler counts.

## Measurement changes

Added `profile_witness` to validate and normalize a saved optimal graph, replay
its public key without topology search, and reject partial keys on interruption.
Added full-witness branches/leaves, nested phase timers and a bounded optional
activity trace for constructors, plans, roots, groups and cancellation.
Added constructor subset cancellation checks every 256 masks; previously up to
2^20 masks could pass between checks. This fixed a gap, not the whole teardown issue.

Manifest `benchmarks/custom/diagnostic-screening.json`: three instrumented single
samples, 32 workers, max rate 1200. 10 optimal baseline/p1 at N<=11, 180 s each;
36 all/groups at N<=9, 600 s. Plus one full witness replay and one cancelled at
nominal 200 ms. Both replays verified; all three solver jobs returned incomplete
and passed the capped-run verifier. No trace records were dropped.

## Witness result

The N=9 replay took 43.267 s, reproduced the exact key, and visited 7,671,847
labeling calls and 1,327,104 leaves. Refinement took 35.452 s, 81.94% of replay;
leaf relabel/encoding took 6.922 s. Three validation calls totaled 0.468 ms.
The cancelled replay returned no key at 205.926 ms, about 0.057 ms after the actual
205.869 ms request. Nominal timeout is not the observed request time.

Source inspection corrected the earlier hypothesis: full-witness refinement only
orders exhaustive permutations. It does not prune them; the leaf key depends on
ranks. Removing that ordering work was the next test. The residual 7.816 s was
an estimate, not a measured optimized time. Partial-state Canonaut was unchanged.

## Constructor result

Both 10 jobs spent essentially all 180 s in the serial optional acyclic constructor.
No worker root or DFS decision started. Both reached N=11/L=18 after three link
groups and twelve profiles were exhausted. This supplied no scheduler ranking.

Normalized 250 = 151+99 requires source denominator 250. Splitters of arity two
and three cannot introduce factor five into an acyclic source coefficient.
Reuse this available necessary condition to skip the helper, not exact cyclic search.
More generally, require divisibility by `2^S2 * 3^S3`, using all source rates.

36 all made 44 helper calls for 23 profiles, totaling 309.944 s. Repeated calls
accounted for 134.842 s; the helper takes no L argument. Ineligible-profile work
was 56.570 s, overlapping repeats by 13.614 s. The union, 177.798 s, was observed
helper work, not a promised whole-solve saving. Cache completed outcomes within N;
never cache interruption as a finished miss or use any helper miss as UNSAT proof.

## Cancellation and scheduling result

36 first witness arrived at 417.602 s. Cancellation at 600.003 s returned at
659.402 s with the same two partial keys and 4.03 GiB peak process memory.
Three active witness canonicalizations returned within 0.043 ms of cancellation,
but the last of 27 open roots ended 59.399 s later. Those witness calls did not
cause the long tail; partial labeling, unwind or destruction remained alternatives.

Remaining groups started at 538.649 s. Until cancellation, roots were distributed
8/8/8/3 over L=12/13/14/15, nearly constant 27 open intervals. Whole-process CPU
averaged 2.58 core equivalents. Intervals include waits and teardown, so neither
27 roots nor low CPU alone proves five idle workers throughout the search.

## Decision and evidence

Prioritize constructor eligibility/reuse and ordering-only witness refinement.
Add search-return, cache-drop and long partial-labeling markers before choosing a
cancellation fix. A common pool cannot recover earlier serial phases. No defaults changed.

Local evidence: `target/parallelism-ladder/diagnostics-20260828/`, witness replay JSON,
`results/summary.json`, per-run activity, saved solutions and snapshots.
Follow-up: 05, refinement and reuse (`mem:solver/experiments/05-refinement-and-reuse`).
