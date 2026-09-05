# 63. Overflow next equal-L group

Date: 2026-09-05. State: analyzed, rejected. Source restored. Related: exp16 occupancy tail, exp20 root tails, exp52 tail-help reject, p14 remaining groups.

## Question and hypothesis

Idle workers during a group's last long roots can search the next same-N L group. If the current group is UNSAT, that work is required. If it SAT and later L is not required (`optimal` / `minimum_links`), cancel overflow. Overflow uses at most `idle - 1` slots (`idle = workers - current_busy`) so one remaining worker stays free.

Expected: 115 all benefits (many L groups, overflow kept). 258 min-links and 36 optimal can benefit on UNSAT L groups, then cancel on the winning group. 238 optimal should be ~flat (winning-group long tail; overflow cancelled).

## Change and comparison

New `ParallelismOptions.overflow_next_link_group` (default false). Isolated screen used the same solver source. Stage stayed `p1`. After applied `benchmarks/custom/variants/overflow-p1-stage.patch` so `p1` also set the flag. Before kept `p1` partitions-only.

Mechanism: current roots preferred; overflow of next same-N L; cap `idle - 1`; cancel overflow on current SAT unless `all`; isolated caches; no next-N.

Candidate preserved as `benchmarks/custom/variants/overflow-next-link-group.patch` against `7cb9340`. Screen isolation overlay: `overflow-p1-stage.patch`. Solver-core restored; production Custom is partitions-only.

## Screen

- Manifest: `benchmarks/custom/overflow-next-l-screen.json`
- Output: `target/parallelism-ladder/overflow-next-l-ab-20260905`
- Finished 2026-09-05T15:34:25+02:00. Analyzer failures []. 20/20 completed. Affinity `ffff` before resume on every job. Hotspot off. CCD96, 16 workers.
- Before SHA256 `C899BF1DC684A18349AA63D226200162B1CA1203A8CF35EB26E978C78826E2E4`
- After SHA256 `D40D00B1D14F13B2956EE0D73167D9BAB1823336AC0A0AC83BFA335F4434C9C7`
- git HEAD at freeze `7cb93407b2d4b6e3078d2dadae416694b9606b4d` plus uncommitted overflow source.

## Results

Public layout keys, preferred witnesses, and saved solutions match on all 10 pairs. 115 all: 49 layouts, N=7 L=10. 258 min-links: 2 layouts, N=9 L=14. 36 optimal: 1 layout, N=9 L=11. 238 optimal: 1 layout, N=8 L=13. Analyzer does not require identical proof objects.

Paired candidate/reference (median change, bootstrap 95%):

| Cell | Wall | CPU | First valid | Notes |
| 115 all CCD96 | −6.55% [−6.61, −6.14] | −0.28% [−0.36, −0.06] | +367% [+360, +368] | Same structural counters (2,087,005 decisions). First valid 2.58–2.64s → 12.08–12.15s. Peak WS +4.8 to +10.0%. |
| 258 min-links CCD96 | −0.02% [−0.67, −0.00] | −0.19% [−0.40, −0.12] | −0.02% | Structural counters identical (7,303,465 decisions). Wall flat. |
| 36 opt CCD96 | +20.03% [+19.86, +20.11] | +31.89% [+29.98, +31.89] | +20.03% | Extra search imported: decisions 1,167,883 → 1,775,585; proof linkGroups 19→20, profiles 41→44, roots 1491→1633. Peak WS +6.2 to +13.6%. |
| 238 opt CCD96 | +3.23% (n=1) | +16.65% (n=1) | +3.23% | Structural counters identical (627,595 decisions). Overflow aborted, not imported; extra CPU only. |

115/258/238 r1 structural counters match except times. 36 is the cell that imported completed later-L overflow into the proof ledger.

## Correctness and limitations

Exact public identity held. 36 proof accounting is not identical: overflow completed extra later-L work before current SAT and imported it, which `optimal` does not need. Three pairs (not five) on 115/258/36; 238 is a single pair. First-valid is not a completion metric but is a large all-mode regression.

## Decision and next step

Rejected. Solver-core restored. Candidate remains `benchmarks/custom/variants/overflow-next-link-group.patch`. Do not treat the 115 all completion win as a production reason: first witness is 4.7× later and 36 wall/CPU CIs exclude 0. Do not revive donation, a coarse occupancy queue, or next-N speculation.
