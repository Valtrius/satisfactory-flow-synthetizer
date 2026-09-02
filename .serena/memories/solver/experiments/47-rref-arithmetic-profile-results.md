# 47. RREF arithmetic profile results

Date: 2026-09-02. State: verified and analyzed. No optimization promoted.
Setup: `mem:solver/experiments/47-rref-arithmetic-profile`.

## Verification

Run: `target/parallelism-ladder/rref-arithmetic-20260902`.
Started 18:49:37, finished 19:08:31.369 Europe/Paris, about 18m54, under one hour.
All 14 records verify: 10 optimal reference controls and 4 intended stress caps.
No crashes, watchdog kills or verification failures. 192 frozen artifact hashes pass.
Frozen analyzer re-run succeeds; original and rechecked summaries are byte-identical.
SHA256: `d485753d683fa53a70ff1140bf30600321316ef5b5f37cdd65ad3b0977143835`.

All completed before/profile results agree on full saved solutions, canonical keys,
preferred witness, full outcome/proof and 15 structural counters. Repeated diagnostic
operand counts agree exactly on 115 and 238.

- 115 all: 49 layouts, N7/L10.
- 238 optimal: 1 layout, N8/L13.
- 258 minimum_links: 2 layouts, N9/L14.

Hard36 all stops at N9/L13 with the same 8 partial saved solutions/keys in both
variants. Hard10 optimal stops at N11/L18 with no witness in either.
Both remain incomplete. Their encountered work and timings are not fixed-work
completion comparisons. The 36 process wall exceeded reported solver wall by
8.59/8.71s; those extra seconds are included in the overall elapsed run, not hidden.
The gap's cause was not profiled here.

## Measured RREF subphases

Percent of the outer RREF elapsed-time accumulator, median within each case.
Both binaries record hotspots; these are diagnostic phases, not production CPU
shares or speedup estimates. Timers include local instrumentation.

| Case / scope / placement  | Profile runs | Normalization | Suffix | Elimination | Finalization |
| ------------------------- | -----------: | ------------: | -----: | ----------: | -----------: |
| 115 all, CCD96            |            2 |        18.31% |  4.11% |      67.97% |        2.87% |
| 238 optimal, CCD32        |            2 |        17.83% |  4.13% |      67.34% |        3.41% |
| 258 minimum_links, CCD96  |            1 |        18.07% |  3.67% |      69.01% |        2.92% |
| 36 all, CCD96, capped     |            1 |        17.00% |  4.52% |      66.17% |        4.41% |
| 10 optimal, CCD32, capped |            1 |        17.68% |  3.85% |      68.84% |        3.37% |

Unbucketed outer RREF work is 6.27–7.89%, including filtering/pivot search,
clock/bookkeeping/atomic merging. RREF itself is 78.93–83.31% of equality encoding.
Do not sum nested RREF and equality timers.

## Operand patterns

Percent of all elimination coefficient updates. Categories overlap.

| Case      | Updates per profile run | Zero destination | Factor+1 | Factor-1 | Pivot entry+1 | Integer pivot entry | Maximum numerator/denominator bits |
| --------- | ----------------------: | ---------------: | -------: | -------: | ------------: | ------------------: | ---------------------------------- |
| 115       |              79,641,802 |           79.46% |   39.51% |   29.48% |         2.91% |              72.59% | 11/6                               |
| 238       |              14,555,868 |           78.55% |   43.45% |   28.65% |         2.53% |              79.17% | 13/6                               |
| 258       |             251,735,042 |           78.35% |   35.19% |   30.05% |         4.67% |              67.67% | 13/7                               |
| 36 capped |             205,325,450 |           79.90% |   42.90% |   26.17% |         2.80% |              77.69% | 12/7                               |
| 10 capped |             193,411,380 |           78.13% |   39.05% |   30.07% |         2.72% |              62.41% | 16/7                               |

Maxima cover observed operands and stored results only, not internal temporaries or
a general upper bound. These samples do not justify replacing exact arbitrary-size
rationals with fixed-width arithmetic without overflow fallback.

## Timing context, not a speedup comparison

Before/profile median solver wall seconds:
115 all 51.272/52.259; 238 optimal 8.617/8.631; 258 minimum_links 193.663/193.958.
This is diagnostic instrumentation on both sides, not a candidate optimization.
One 258 run and two 115/238 runs per side, randomized but no balanced paired effect
estimate. Do not infer an optimization gain or precise instrumentation cost.
The existing RREF ordering remains permanent based on experiment 46, not this screen.

## Source check and recommendations

Checked production `canonical.rs::rational_rref` and `solver-api/src/rational.rs`.
The latter delegates operators to installed num-rational 0.4.2.
Local `num-rational-0.4.2/src/lib.rs` lines 701–715 multiply ratios using general
cross-GCD arithmetic. Lines 761–790 implement subtraction with equal-denominator
or LCM paths, without a zero-left shortcut. Lines 879–900 negate via new_raw.
These are source observations, not timing attribution to individual GCD calls.

1. Test zero-destination elimination first. Replace 0-factor*pivot with the negated
   product, and 0-pivot with direct negation for the existing +1-factor path.
   It removes general subtraction from 78–80% of observed updates while preserving
   exact arithmetic and output order.
2. Test factor -1 separately. Classify once per eliminated row and replace
   destination-(-1)*pivot with destination+pivot. It can remove a general
   multiplication from 26–30% of updates. Overlap with zero destinations is unknown;
   do not add the prevalence figures or assume combined gains.
3. Keep the existing +1 and zero-factor skips. Defer suffix representation changes,
   an integer RREF rewrite and pivot-normalization work until the smaller tests.
   No cyclicity, benchmark identity or memory gate; no scheduler change.
4. Require exact dense-oracle and full-output/proof checks, then fresh hotspot-OFF
   A/B runs with fixed affinity and balanced order. Keep optimal/all/minimum_links
   controls and capped hard diagnostics distinct. New runs must remain <=1h
   including cleanup. Measure each shortcut independently before combining.

No production shortcut implemented in this analysis. Nothing running.
Existing source 1006e6f, manifest 14d3f73, prior promotion docs c106d4c.
Results/handoff memory updates are uncommitted; no commit or push this turn.

## Local evidence

- `target/exp47-analysis.py`, `target/exp47-analysis.log`.
- `results/audit-20260902.json` inside the run, with raw counts/phase shares,
  complete/stress identity checks, statuses, proofs and per-run resource records.
- `target/exp47-reverification.log`; original `results/summary.json` unchanged.
- Source and binaries remain frozen in the run and variant directories.
