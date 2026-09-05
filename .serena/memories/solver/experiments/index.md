# Experiment index

Do not bulk-read experiments. After picking an ID from the table below, read exactly
one matching memory under topic `solver/experiments` (name = file stem).

All timings are historical observations on this machine unless stated otherwise.
Read current status (`mem:solver/status`) before treating an old recommendation as current.
Record dates follow the analysis chronology, not necessarily every run's start time.

| ID                                                                 | Date          | Question                                                                                  | Outcome                                                                                                                                                                          |
| ------------------------------------------------------------------ | ------------- | ----------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 01 (`mem:solver/experiments/01-parallelism-ladder`)                | 2026-08-27    | Which parallelism stages help?                                                            | Large-worker wins; extra CPU/memory and small-worker regressions                                                                                                                 |
| 02 (`mem:solver/experiments/02-exact-kernel`)                      | 2026-08-27/28 | Can exact calculations be cheaper without changing search?                                | 792 verified runs; kernel made permanent                                                                                                                                         |
| 03 (`mem:solver/experiments/03-hard-case-screen`)                  | 2026-08-28    | Do those findings extend to difficult inputs?                                             | Slow witness/constructor phases; incomplete enumeration; N<=10 cutoff error                                                                                                      |
| 04 (`mem:solver/experiments/04-witness-constructor-diagnostics`)   | 2026-08-28    | Which serial work and cancellation phase dominate?                                        | Full-witness refinement, repeated helper work; root teardown still unresolved                                                                                                    |
| 05 (`mem:solver/experiments/05-refinement-and-reuse`)              | 2026-08-28    | Remove redundant refinement and avoid ineligible/repeated construction?                   | 6.54x isolated replay; first solver job killed; refinement later committed                                                                                                       |
| 06 (`mem:solver/experiments/06-cancellation-recovery`)             | 2026-08-28    | Preserve failure evidence and locate the cancellation tail?                               | 9 valid results, 3 kills; cache destruction observed; allocator cause unproved                                                                                                   |
| 07 (`mem:solver/experiments/07-compact-keys-and-constructor`)      | 2026-08-28    | Reduce key storage and exact subset arithmetic?                                           | 16 verified records, no kills; strong bundle gains; six incomplete solves                                                                                                        |
| 08 (`mem:solver/experiments/08-repeat-scheduling`)                 | 2026-08-28    | Are gains repeatable; how do partitions and 16/32 workers compare?                        | 62 verified; p1 hard optimal 2.96x faster; 24 all/baseline median regresses 6.2%                                                                                                 |
| 09 (`mem:solver/experiments/09-serializer-and-find-all`)           | 2026-08-28    | Does isolated encoding explain the regression; does p14 help hard all?                    | 42 verified; encoding favors all seven medians; p14 earlier witness, all unfinished                                                                                              |
| 10 (`mem:solver/experiments/10-constructor-promotion`)             | 2026-08-28    | Promote the validated constructor without the deadline?                                   | Permanent, separate from compact keys; not pushed                                                                                                                                |
| 11 (`mem:solver/experiments/11-compact-key-promotion`)             | 2026-08-28    | Promote compact exact keys independently?                                                 | Permanent with isolated encoding evidence; not pushed                                                                                                                            |
| 12 (`mem:solver/experiments/12-hard-obligation-profiling`)         | 2026-08-28    | What dominates the remaining hard N/L/profile work?                                       | 24 verified; 36 witness tail, 10 basis/labeling cost; no kills                                                                                                                   |
| 13 (`mem:solver/experiments/13-calculation-changes`)               | 2026-08-28    | Can early exact-L checks, cached witness leaves and direct RREF bounds reduce hard costs? | 80 verified; isolated gains support three promotions; hard whole-all still capped                                                                                                |
| 14 (`mem:solver/experiments/14-calculation-promotion`)             | 2026-08-28    | Promote the three measured calculations independently?                                    | Permanent in three separate commits                                                                                                                                              |
| 15 (`mem:solver/experiments/15-prefix-workloads`)                  | 2026-08-28    | Can exact subtrees supply completed hard-10 references?                                   | Exact certificates verified; all 24 hard prefixes capped; deeper discovery needed                                                                                                |
| 16 (`mem:solver/experiments/16-post-calculation-screen`)           | 2026-08-28    | How do p1/p14 compare after cheaper calculations?                                         | 38 verified after analyzer fix; hard optimal 23.27% shorter; all still capped                                                                                                    |
| 17 (`mem:solver/experiments/17-p1-promotion-and-followups`)        | 2026-08-29    | Can p1 become a default; do the constructor, sharing, donation and deeper prefixes help?  | 180 verified; p1 gains accepted despite hard-10 memory; other candidates need work                                                                                               |
| 18 (`mem:solver/experiments/18-guarded-p1-promotion`)              | 2026-08-29    | Promote p1 without the measured hard-10 memory failure?                                   | N<=9 guard validated, then rejected before commit                                                                                                                                |
| 19 (`mem:solver/experiments/19-unconditional-p1-promotion`)        | 2026-08-29    | Make p1 the production policy despite its hard-10 resource cost?                          | Unconditional p1 selected; memory issue explicitly deferred                                                                                                                      |
| 20 (`mem:solver/experiments/20-constructor-and-medium-cases`)      | 2026-08-29    | Can post-winning constructor work be removed; what causes medium-case CPU troughs?        | 26 verified; guard permanent; troughs are uneven root-search tails                                                                                                               |
| 21 (`mem:solver/experiments/21-adaptive-root-profiling`)           | 2026-08-29    | Which calculations dominate the longest 238 adaptive root?                                | 6 verified; 80.6M witness leaves dominate; best/all do identical work                                                                                                            |
| 22 (`mem:solver/experiments/22-analytic-witness-ports`)            | 2026-08-29    | Can exact symmetric-port labels replace their factorial witness enumeration?              | 6 verified; exact 559,872x leaf reduction; root 2.38-2.57x faster; permanent                                                                                                     |
| 23 (`mem:solver/experiments/23-whole-translation-and-dfs-profile`) | 2026-08-29    | Does the port gain translate whole; which exact DFS identity dominates next?              | 27 verified; 238 improves 43-52%; legal/state identity dominate next                                                                                                             |
| 24 (`mem:solver/experiments/24-canonical-purpose-profile`)         | 2026-08-29    | Which exact state/open-port/marked-child/SCC identity consumes canonicalization time?     | 4 verified; internal marked-child bypass is the next isolated test                                                                                                               |
| 25 (`mem:solver/experiments/25-internal-dfs-marked-bypass`)        | 2026-08-30    | Can dispatched DFS defer marked-child equivalence to propagated canonical state keys?     | 12 verified; four completed pairs improve 16.6-21.1%; repeat before promotion                                                                                                    |
| 26 (`mem:solver/experiments/26-internal-dfs-promotion-repeat`)     | 2026-08-30    | Do repeated completed solves confirm the bypass; what owns hard-run memory?               | 16 verified; combined medians improve 17.5-22.9%; permanent; RAM is local caches                                                                                                 |
| 27 (`mem:solver/experiments/27-minimum-link-enumeration-mode`)     | 2026-08-30    | Can users enumerate only the proven minimum-N/minimum-L layouts?                          | Implemented across all solvers, UI and benchmark runner; differential test passed                                                                                                |
| 28 (`mem:solver/experiments/28-state-open-port-coordinate-reuse`)  | 2026-08-30    | Can DFS reuse state labeling for its final open-port tie-break?                           | 14 verified; all completed pairs improve 18.2-49.4%; permanent                                                                                                                   |
| 29 (`mem:solver/experiments/29-deferred-state-canonicalization`)   | 2026-08-31    | Can DFS avoid exact state keys until a cheap invariant bucket repeats?                    | 26 verified; completed medians improve 18.3-38.1%; permanent                                                                                                                     |
| 30 (`mem:solver/experiments/30-no-state-cache-ablation`)           | 2026-08-31    | Does recursive DFS state caching improve completion at all?                               | Rejected; 12.3% slower on 115 and 3.00x slower on 238                                                                                                                            |
| 31 (`mem:solver/experiments/31-propagation-bounds`)                | 2026-08-31    | Can propagation remove a second copy of every physical-flow bound check?                  | 38 verified; all completed medians improve 5.36-9.40%; promote                                                                                                                   |
| 32 (`mem:solver/experiments/32-weighted-sparse-quotient`)          | 2026-08-31    | Can exact weighted representatives reduce sparse elimination work?                        | Rejected; 115 improves, but 36/238 regress and sparse passes cost 6.79% more                                                                                                     |
| 33 (`mem:solver/experiments/33-sparse-row-deduplication`)          | 2026-08-31    | Do identical normalized equations add avoidable sparse elimination work?                  | Rejected; zero duplicates across 164M hard input-row instances                                                                                                                   |
| 47 (`mem:solver/experiments/47-rref-arithmetic-profile`)           | 2026-09-02    | Which RREF arithmetic phase and operand patterns dominate?                                | 14 verified; elimination 66–69%; test zero-destination then factor -1 shortcuts                                                                                                  |
| 48 (`mem:solver/experiments/48-rref-elimination-shortcuts`)        | 2026-09-02    | Do zero-destination or negative-unit shortcuts improve completion independently?          | Negative-unit permanent after confirmation and integration checks; zero held                                                                                                     |
| 49 (`mem:solver/experiments/49-derived-key-results`)               | 2026-09-03    | Remove deterministic inequality-key payload?                                              | Permanent4d711b1; all17 substantive wall pairs improve,10.2–21.3% medians                                                                                                        |
| 50 (`mem:solver/experiments/50-checked-rref-results`)              | 2026-09-03    | Checked64 canonical RREF with BigInt fallback?                                            | Not promoted;115 -2.48%,238/258 flat,36 +2.80% wall/+8.49% CPU; source restored                                                                                                  |
| 51 (`mem:solver/experiments/51-reachability-results`)              | 2026-09-04    | Skip remaining-profile analysis, borrowed dead verdict, then pre-SCC check?               | Permanent skip+verdict; reject early. 32/32 115/238/258 wall pairs; 115 −3.08%, 238 −3.62%, 258 −1.52%. Unpushed.                                                                |
| 52 (`mem:solver/experiments/52-tail-scheduling-results`)           | 2026-09-04    | Schedule long remaining roots without disabling deferred canonicalization?                | Rejected. 115 all-mode lost layouts (49→43/44). Identity-passing 238/36 slower. Restored to experiment51.                                                                        |
| 53 (`mem:solver/experiments/53-post51-cost-profile`)               | 2026-09-04    | After 51, which buckets and root tails own wall?                                          | 12/12 verified. Prop 54–67% accounted; 258 Bareiss forward 21% accounted on leftover 16–31-row systems. Overlapping long roots, not one tail.                                    |
| 54 (`mem:solver/experiments/54-bareiss-forward`)                   | 2026-09-04    | Isolated Bareiss-forward bookkeeping vs production after 51?                              | Permanent in `d6902a4`. 66/66. 115 wall −10.85%/−10.58%, 258 −8.49%, 238 −2.89%/−2.39%, 36 −0.83% (bootstrap includes 0). Unpushed.                                              |
| 55 (`mem:solver/experiments/55-completed-state-sharing`)           | 2026-09-04    | Completed-state sharing that keeps deferred owners vs production p1 after 54?             | Rejected. 66/66. 115 identity 49=49. 258 wall −4.27%; 115 wall CI includes 0; WS +28–45%. Source restored to 54.                                                                 |
| 56 (`mem:solver/experiments/56-fraction-free-rref`)                | 2026-09-04    | Fraction-free canonical dense RREF vs production p1 after 54?                             | Rejected. 66/66. No wall win with CI excluding 0; 36 wall +2.98% / CPU +5.17%. Source restored.                                                                                  |
| 57 (`mem:solver/experiments/57-post54-cost-profile`)               | 2026-09-04    | After 54, which buckets own wall; does prep still justify templates?                      | 12/12 verified. Prep templates parked (unbucketed prep ~1% accounted). Prop 53–63%; state+SCC 30–39%; forward no longer 115/258 leader.                                          |
| 58 (`mem:solver/experiments/58-bounds-labeling-abc`)               | 2026-09-04    | Isolated dirty bound scan vs BaseColor/labeling cache, each vs production 54?             | 72/72. Dirty bounds permanent (115/238/258 wall −7 to −11%/−3.4%). Reject labeling (mixed; 238 CCD96 +1.67%). Unpushed.                                                          |
| 59 (`mem:solver/experiments/59-l-shortcut-opportunity`)            | 2026-09-04    | How often would exact-L under/over/complete shortcuts fire?                               | 4/4 completed. Under-L 28.6%/48.9%/0%/27.2% of decisions on 24/115/238/36. Over-L 0. Complete mismatch 6/79/0/0. `mem:solver/experiments/59-l-shortcut-opportunity`.             |
| 60 (`mem:solver/experiments/60-remaining-port-under-l`)            | 2026-09-05    | Isolated remaining-port under-L prune vs production 58?                                   | 30/30. Permanent in `2d6501c`. 115 all wall −39.4%/−39.7%; 36 −7.58%; 238 no-hit. Identity held. Unpushed.                                                                       |
| 61 (`mem:solver/experiments/61-scc-deferral`)                      | 2026-09-05    | Isolated SCC deferral vs production after 60?                                             | Rejected. 30/30. Identity held. 115/238 wall −5 to −9%; 36 wall +4.93% CI excludes 0. Source restored to `6bb0763`. Patch kept.                                                  |
| 62 (`mem:solver/experiments/62-l-bound-abc`)                       | 2026-09-05    | Earlier under-L placement and forced remaining-L lower bound vs production after 60?      | Rejected both. 60/60. Identity held. places 238 CCD32 wall +1.26% CI excludes 0; forced prune fires 1:1 with prop contradictions, 115 CCD96 wall +0.91%. Source stays `6bb0763`. |

## Current optimization work

Experiment62 remaining-port L-bound A/B/C rejected both candidates after 60/60 at `target/parallelism-ladder/l-bound-abc-20260905`. Identity held. places: no completed-wall win; 238 CCD32 +1.26% CI excludes 0 (no-hit cell). forced: prune fires 1:1 with later propagation contradictions; states/decisions unchanged; 115 CCD96 wall +0.91% / CPU +0.34% CIs exclude 0. Production stays `6bb0763`. Patches kept. Results recorded in `4adddc2`. `mem:solver/experiments/62-l-bound-abc`. `mem:solver/active`.
Experiment61 isolated SCC deferral rejected after 30/30 at `target/parallelism-ladder/scc-deferral-ab-20260905`. Identity held. 115/238 wall −5.16 to −8.85%; 36 wall +4.93% with CI excluding 0. Solver-core restored to `6bb0763`. Candidate remains `benchmarks/custom/variants/scc-deferral.patch`. `mem:solver/experiments/61-scc-deferral`.
Experiment60 remaining-port under-L prune is permanent in `2d6501c`, unpushed. 30/30 at `target/parallelism-ladder/remaining-port-under-l-ab-20260904`. 115 all wall −39.4% CCD32 / −39.7% CCD96 (CIs exclude 0); 36 −7.58%; 238 no-hit control. Identity held. Do not add over-L-before-prepare or complete-L-before-solve. `mem:solver/experiments/60-remaining-port-under-l`. `mem:solver/active`.
Experiment59 count-only exact-L shortcut diagnostic analyzed. 4/4 completed at `target/parallelism-ladder/l-shortcut-opportunity-screen-20260904`. Under-L remaining-port hits 28.6–48.9% of decisions on 24 all / 115 all / 36 optimal, 0% on 238 optimal. Over-L before prepare never fired. Complete-L mismatch 6 and 79 on the two all-mode jobs. Analyzer baseline-name failure only. `mem:solver/experiments/59-l-shortcut-opportunity`.
Experiment58 isolated dirty-bounds and color-labeling A/B/C analyzed. 72/72 at `target/parallelism-ladder/bounds-labeling-abc-screen-20260904`. Dirty bounds permanent in `90747a4`, unpushed; labeling rejected. Experiments 61 and 62 later rejected SCC deferral, earlier under-L placement, and forced remaining-L. Next remaining: post-58/60 hotspot-on cost-mix. `mem:solver/experiments/58-bounds-labeling-abc`. `mem:solver/active`.
Experiment57 hotspot-on cost-mix profile of production 54 analyzed. 12/12 verified at `target/parallelism-ladder/post54-cost-profile-20260904`. Prep-template reuse parked. Original 49 apply-all list exhausted. `mem:solver/experiments/57-post54-cost-profile`.
49 is permanent and committed4d711b1. 50 not promoted. Experiment51 skip+borrowed-verdict is permanent in `7799c11`, unpushed. Experiment52 rejected. Experiment53 is pre-54. Experiment54 Bareiss-forward is permanent in `d6902a4`, unpushed. Experiments 55 and 56 rejected; solver source matches 54. Park templates, sharing, donation, checked64, fraction-free RREF, weighted quotients, duplicate-row removal, rollback-aware Bareiss, early reachability, labeling, SCC deferral, earlier under-L placement, and forced remaining-L. Negative-unit48 remains permanent, zero-destination held.

## How the diagnosis changed

1. Early parallelism results favored larger frontiers and overlapping groups.
2. The full suite exposed overhead and worker-count regressions, so defaults stayed off.
3. Hard cases showed low aggregate CPU use, but this did not identify idle workers.
4. Replay isolated expensive full-witness refinement; the cyclic diagnostic was
   blocked in an optional acyclic helper, not in the worker scheduler.
5. Removing those costs exposed heavy exact-search storage and cancellation tails.
6. Live evidence identified cache destruction in some tails; compact storage and
   integer subsets improved the bundle without proving an allocator diagnosis.
7. Repeats confirm a strong hard-optimal partitioning gain and expose a small-case
   baseline regression. Results (`mem:solver/experiments/08-repeat-scheduling-results`) motivate a p14
   hard find-all comparison and isolated serializer/constructor work next.
8. Isolated encoding favors compact rows and reduces their diagnostic cost 18.8x.
   The old bundle regression remains unexplained. P14 improves first-witness latency
   without completing hard enumeration. 09 results (`mem:solver/experiments/09-serializer-and-find-all-results`)
   recommend separate constructor/key promotion and fixed hard-proof diagnostics next.
9. Fixed hard-work results (`mem:solver/experiments/12-hard-obligation-results`) isolate a serial witness
   tail in 36 and a busy worker pool in 10. Next experiments (`mem:solver/experiments/12-next-experiments`)
   prioritize earlier exact-L rejection, cheaper witness leaves and exact basis work.
10. Isolated calculation results (`mem:solver/experiments/13-calculation-results`) confirm those gains.
    Whole results (`mem:solver/experiments/13-whole-results`) improve completed short solves and finish
    the fixed hard-36 group, but not full hard enumeration. Recompare p1/p14 with
    the cheaper calculations; seek a complete exact prefix for hard 10.
11. Post-calculation results (`mem:solver/experiments/16-post-calculation-results`) repeat the hard-optimal
    gain. P1 closes L=12 with less CPU/memory than p14 at the cap; neither finishes
    all. Diagnostics (`mem:solver/experiments/16-diagnostics-and-next-steps`) identify optional constructor
    work after the winning group, a p1 DFS tail and fixed p14 worker allocations.
    Test the helper first, then isolate sharing/donation. All hard prefixes capped.
12. The focused promotion screen (`mem:solver/experiments/17-p1-promotion-and-followups`) evaluates p1
    against current baseline across modes and worker counts. It separately measures
    the post-winning constructor guard, p1/p12/p123 fixed work and deeper prefixes.
    P1 fails the predeclared hard-10 memory gate but improves every completed N<=9
    workload at 16/32 workers. Constructor and p123 advance work but are not promoted.
13. The guarded p1 proposal (`mem:solver/experiments/18-guarded-p1-promotion`) passed validation, then the
    user rejected memory as a promotion gate before commit.
14. Unconditional p1 promotion (`mem:solver/experiments/19-unconditional-p1-promotion`) applies adaptive
    partitions for every N in the shared production Custom adapter. Sharing, donation
    and remaining groups stay off. The hard-10 memory cost remains documented.
15. The constructor and medium-case results (`mem:solver/experiments/20-results`) preserve all exact
    outputs across 26 jobs. Timing is mixed, but the guard removes redundant
    post-winning work and becomes permanent. Process samples show that the medium-case
    CPU troughs are uneven root-search tails, sometimes with one root left running.
16. Adaptive root profiling (`mem:solver/experiments/21-adaptive-root-profiling`) freezes one production
    p1 root plan and replays its 82.731-second tail independently. Six verified
    results (`mem:solver/experiments/21-results`) isolate 80.6 million witness leaves and show that best
    and all perform identical canonicalization work.
17. Analytic witness ports (`mem:solver/experiments/22-analytic-witness-ports`) retain terminal/node
    permutations but derive the exact minimum symmetric ports. Six verified
    results (`mem:solver/experiments/22-results`) preserve every exact result, reduce witness leaves
    559,872x and improve the isolated root 2.38-2.57x. The change is permanent.
18. Whole translation and DFS profiling (`mem:solver/experiments/23-whole-translation-and-dfs-profile`)
    compares the preserved pre-port solver with current production on 115, 238 and
    hard 36, then profiles hard 10 and remaining whole DFS/canonicalization costs.
    Results (`mem:solver/experiments/23-results`) verify 27 jobs. The 238 whole solve improves 43-52%,
    115 all improves 10%, and hard-36 optimal is unchanged. Legal-decision and state
    canonicalization dominate the remaining measured work. Split their exact key
    purposes before changing DFS identity construction.
19. Canonicalization purpose profiling (`mem:solver/experiments/24-canonical-purpose-profile`) splits
    graph calls into state, open-port, marked-child and other keys, and counts SCC
    keys. Four verified jobs attribute 94.5-96.2% of legal-decision time to open and
    marked keys. Marked keys remove fewer than 0.3% of measured candidates but also
    stabilize root obligations. Test bypass only inside dispatched DFS roots.
20. Internal DFS marked-child bypass (`mem:solver/experiments/25-internal-dfs-marked-bypass`) retains
    keyed root/frontier identities while testing raw decisions inside dispatched
    roots. Its four completed screening pairs improve 16.6-21.1% with identical
    exact outputs. Run two more samples per variant before production promotion.
21. The promotion repeat (`mem:solver/experiments/26-internal-dfs-promotion-repeat`) verifies all 16 new
    jobs. Combined three-sample medians improve 17.5-22.9% with identical exact
    outputs, so the internal bypass becomes permanent. Existing telemetry attributes
    hard-run RAM mainly to the sum of worker-local state and SCC caches. Measure those
    two categories separately before optimizing memory itself.
22. Minimum-link enumeration (`mem:solver/experiments/27-minimum-link-enumeration-mode`) adds a third
    exact solve scope across Custom, Z3 and Reference. It exhausts only the first
    satisfiable L group at minimum N and is available to the benchmark runner as
    `minimum_links`.
23. State-coordinate reuse (`mem:solver/experiments/28-state-open-port-coordinate-reuse`) removes the
    recursive DFS open-port individualization pass while retaining keyed root and
    frontier planning. All six completed pairs preserve exact outputs and improve
    18.2-49.4%, so the change is permanent. Hard 10 advances 45% more states within
    the common cap while sampled working set rises 12.8%.
24. Deferred state canonicalization (`mem:solver/experiments/29-deferred-state-canonicalization`) stores
    the first state under a cheap invariant and promotes a repeated bucket to exact
    canonical keys. All 26 records verify; six completed medians improve 18.3-38.1%,
    so promotion is recommended.
25. The no-state-cache ablation (`mem:solver/experiments/30-no-state-cache-ablation`) confirms that local
    memoization pays for itself. No cache is 12.3% slower on 115 minimum L and 3.00x
    slower on 238 optimal, where structural decisions rise 4.33x. The fail-fast gate
    rejects the candidate before the 36 control or larger suite.
26. Propagation-bound profiling (`mem:solver/experiments/31-propagation-bounds`) attributes substantial
    hard-run time to a second evaluation of the same positivity and capacity
    predicates. All 38 jobs verify; six completed medians improve 5.36-9.40%, while
    the capped hard run processes 3.62% more decisions. Promotion is recommended.
27. The weighted sparse quotient (`mem:solver/experiments/32-weighted-sparse-quotient`) substitutes only
    exact facts already established by propagation before Bareiss elimination. All 38
    records verify, but the mixed whole-solve result and 6.79% higher sparse cost per
    hard pass reject the unconditional candidate. The source is restored.
28. Sparse row deduplication (`mem:solver/experiments/33-sparse-row-deduplication`) isolates the quotient's
    cheapest operation after the existing sort. All 26 records verify, but the hard
    candidate removes zero rows across 164 million input-row instances. Restore source
    and stop pursuing duplicate equations.
29. Sparse phase and matrix-shape profiling (`mem:solver/experiments/34-sparse-phase-profile`) measures
    preparation, Bareiss forward elimination, back reduction, and deduction extraction.
    It also buckets analysis time by matrix shape and distinguishes initial analysis
    from value and ratio reanalysis. Results (`mem:solver/experiments/34-sparse-phase-profile-results`)
    put 55.2-84.0% of sparse time in preparation and reject a rollback-aware Bareiss
    basis as the next change.
30. Sparse preparation profiling (`mem:solver/experiments/35-sparse-preparation-profile`) splits the
    dominant preparation phase into variable collection, substitution and
    normalization, tautology filtering, sorting, and working-row conversion. The
    six verified results (`mem:solver/experiments/35-sparse-preparation-profile-results`) put 89.99-91.42%
    of preparation in substitution and normalization. Test exact integer evaluation
    of fully known rows before rewriting mixed-row arithmetic.
31. Fully known row substitution (`mem:solver/experiments/36-fully-known-row-substitution`) evaluates
    equations whose coefficients are all known with one integer denominator LCM.
    All 14 records verify (`mem:solver/experiments/36-fully-known-row-substitution-results`). Completed
    medians improve 9.73-16.41% with identical exact work, and hard 10 processes
    6.56% more states within the cap. The change is permanent.
32. No-known row substitution (`mem:solver/experiments/37-no-known-row-substitution`) returns an already
    primitive row directly when none of its coefficients has a known value. Fully
    known and mixed behavior remain unchanged. All 14 records verify (`mem:solver/experiments/37-no-known-row-substitution-results`).
    Completed medians improve 3.42-5.37%; the single capped hard sample processes
    2.40% less work. The exact clone path is permanent.
33. Mixed-row integer substitution (`mem:solver/experiments/38-mixed-row-integer-substitution`) replaces
    repeated rational normalization in the remaining substitution case with one
    denominator LCM and integer residual. All 14 records verify (`mem:solver/experiments/38-mixed-row-integer-substitution-results`).
    Completed algebra medians improve 2.43-3.05% with identical exact work. Whole-solve
    wall medians move -0.60% and +3.16%. The integer path is permanent.
34. Canonicalization reprofile (`mem:solver/experiments/39-canonicalization-reprofile`) reuses the retained
    purpose and subphase counters on the original two completed and two capped hard
    workloads. All four records verify (`mem:solver/experiments/39-canonicalization-reprofile-results`).
    State keys now own 99.96-100.00% of graph-purpose time. Equality and inequality
    encoding consume 58.9-67.2% of combined state/SCC canonicalization. Split those
    calculations next; labeling and legal-decision keys are no longer the first target.
35. Equality and inequality subphase profile (`mem:solver/experiments/40-equality-inequality-subphase-profile`)
    splits canonical port indexing, equation construction, rational RREF, bound-row
    construction, primitive normalization, and sort/deduplication. All four records
    verify (`mem:solver/experiments/40-equality-inequality-subphase-profile-results`). Inequality sort/dedup
    owns 52.1-54.3% of bound encoding; test integral rows through encoding next.
36. Primitive integer inequality rows (`mem:solver/experiments/41-primitive-integer-inequality-rows`)
    retains normalized bound coefficients as integers through sorting, deduplication,
    and sparse encoding. All 16 records verify (`mem:solver/experiments/41-primitive-integer-inequality-rows-results`),
    but completed whole medians are mixed. Keep the candidate and repeat controls.
37. Complete propagation variables (`mem:solver/experiments/42-complete-propagation-variable-set`) skips
    repeated sparse-row variable discovery when propagation supplies its authoritative
    sorted registered-port set. A 36-job four-variant screen separates this candidate
    from integer inequality rows and adds a longer 258 minimum-link control.
    Results (`mem:solver/experiments/42-complete-propagation-variable-set-results`)
    verify all 36 completions but expose a 22.46% adverse variable-only 258 sample.
    Retain candidates pending focused repeats; no scheduler change.

38. Focused 258 repeats (`mem:solver/experiments/42-258-confirmation-results`) verify
    all ten new jobs. Search spans identify the two long roots, not cleanup.
    An 82.30% local variable-collection reduction does not establish whole speedup.
39. Isolated roots with CPU affinity (`mem:solver/experiments/43-root258-affinity`)
    retain the original ordered root plan and compare four variants within each
    fixed logical CPU. All 28 completions verify. Results (`mem:solver/experiments/43-root258-affinity-results`)
    favor variables modestly, reject integer rows on these roots, and establish
    substantial placement sensitivity. No production change yet.

40. Topology-controlled confirmation (`mem:solver/experiments/44-topology-variable-confirmation`)
    separates CCD placement from candidate effects. All 198 runs verify; variables
    provide small but nonuniform wall gains. Results:
    `mem:solver/experiments/44-topology-variable-confirmation-results`.
41. Allocation-free cache size accounting (`mem:solver/experiments/45-cache-accounting`)
    replaces temporary byte-vector allocation with an exact length calculation.
    Oracle/correctness checks pass; 21/24 completed wall pairs and all CPU pairs
    improve. Retention recommended; `mem:solver/experiments/45-cache-accounting-results`.

## Evidence conventions

Run directories named below are under repository-local `target/parallelism-ladder/`.
They contain ignored raw data, not portable documentation. The Markdown records
retain essential results; request the archives if another checkout lacks them.
Do not recreate historical timings from current binaries and call them the same run.

Auxiliary evidence includes `stage0/` and `script-smoke/`, early harness checks;
`baseline-bin/`, preserved early binaries; `*-plan-check-*`, manifest validation;
and `kernel-before-bin/`, `kernel-after-bin/`, `kernel-before-source/`,
`kernel-after-source/`, the kernel snapshots.
The smoke run was capped/incomplete and supplies no completion-speed evidence.
The 42 early performance runs are the 10 pilot, 20 repeat, 8 scaling and 4 ablation
records in experiment 01. The eight initial kernel diagnostics are in experiment 02.

Use the template (`mem:solver/experiments/template`) for the next distinct experiment. Do not erase
failed records when replacing a runner or promoting a successful optimization.

2026-09-03 checkpoint:49 permanently committed4d711b1;50 not promoted and removed from production after verified mixed results. See the current optimization section and `mem:solver/experiments/50-checked-rref-results`.

2026-09-05 experiment62 remaining-port L-bound A/B/C rejected both; source stays `6bb0763`. Experiment61 isolated SCC deferral rejected. Experiment60 remaining-port under-L prune remains permanent in `2d6501c`, unpushed. Experiment59 count-only diagnostic: under-L only is material; the forced remaining-L prune fires but does not shrink search. Experiment58 dirty bounds remain in `90747a4`, unpushed; labeling rejected. `mem:solver/experiments/62-l-bound-abc`.
