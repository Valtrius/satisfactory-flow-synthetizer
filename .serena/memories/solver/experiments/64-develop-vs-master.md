# 64. Develop vs master mixed-type screen

Date: 2026-09-05. State: analyzed. No solver change.
Related: production on develop after experiments 49–63 vs published `master`.

## Question and hypothesis

How much faster is current `develop` (`18b60f6`) than `master` (`f89b6d9`) on a sample of each case type and solve mode? Hypothesis: completed develop walls are shorter on medium/hard cells; tiny/easy may be noisy. Exact public results must match. `cyclic10` is a stress cap.

## Change and comparison

No source change. Isolated release `profile_case`:

- before = `master` `f89b6d9`, SHA256 `1042C2B3044D490B2D9C1E507AA0C3DF1ECD010540C18EA596D4AA67D18A883F`
- after = `develop` `18b60f6`, SHA256 `41A32FCBA771D283D7249E2AB3BD0F0EC7D63F3A0CA3C5D0F90E7248BD3D2F3E`

Screen `target/parallelism-ladder/develop-vs-master-20260905`. Finished 2026-09-05T17:18:13+02:00. 24/24 verified, failures []. Affinity `ffff` before resume on 24/24. Hotspot off, p1, CCD96, 16 workers.

## Results

22 completed Optimal, 2 intended cyclic10 caps. Analyzer failures []. HTML `develop-vs-master.html` in the run directory.

Paired develop/master wall (official summary): 115 all n=2 −57.58% [−57.76, −57.41]; 65 all n=1 −55.00%; 24 all −44.27%; 65 opt −40.33%; 24 opt −31.24%; 238 opt n=2 −29.97% [−31.16, −28.78]; 36 opt −25.24%; 258 min-links −13.31%. Tiny 11ms vs 14ms, CPU both 15.6ms: no percent claim. Cyclic10 both cancelled at 60s: no completion speedup.

CPU: 115 −64.66%; 65 all −59.50%; 36 −44.09%; 258 −20.63%; 238 −31.51%.

Layouts held: tiny 2 N=2 L=1; 24 1/6 N=7 L=8; 65 1/13 N=6 L=8; 115 49 N=7 L=10; 238 1 N=8 L=13; 258 2 N=9 L=14; 36 1 N=9 L=11.

Decisions: 238 and 258 identical (627,595 and 7,303,465). 115 3,989,576 → 2,087,005. 36 1,525,496 → 1,167,883. Cyclic10 cap 2,467,202 → 2,877,228 (+16.6%) with the same proof ledger (N through 10, 13 profiles, 13 roots); peak WS 2.25GiB → 2.60GiB.

## Correctness and limitations

Exact public identity held on completed pairs. Survey, mostly n=1. CCD96 only. Tiny timings below timer/sample resolution. Caps are not completion evidence. Bundle of develop commits, not an isolated patch.

## Decision and next step

Informational. Develop is faster on every completed cell in this screen. No production change from this experiment. Solver-core unchanged.
