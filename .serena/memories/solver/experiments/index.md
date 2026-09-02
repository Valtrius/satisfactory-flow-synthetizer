# Experiment index

Do not bulk-read experiments. After picking an ID from the table below, read exactly
one matching memory under topic `solver/experiments` (name = file stem).

All timings are historical observations on this machine unless stated otherwise.
Read current status (`mem:solver/status`) before treating an old recommendation as current.
Record dates follow the analysis chronology, not necessarily every run's start time.

| ID                                                                 | Date          | Question                                                                                  | Outcome                                                                             |
| ------------------------------------------------------------------ | ------------- | ----------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------- |
| 01 (`mem:solver/experiments/01-parallelism-ladder`)                | 2026-08-27    | Which parallelism stages help?                                                            | Large-worker wins; extra CPU/memory and small-worker regressions                    |
| 02 (`mem:solver/experiments/02-exact-kernel`)                      | 2026-08-27/28 | Can exact calculations be cheaper without changing search?                                | 792 verified runs; kernel made permanent                                            |
| 03 (`mem:solver/experiments/03-hard-case-screen`)                  | 2026-08-28    | Do those findings extend to difficult inputs?                                             | Slow witness/constructor phases; incomplete enumeration; N<=10 cutoff error         |
| 04 (`mem:solver/experiments/04-witness-constructor-diagnostics`)   | 2026-08-28    | Which serial work and cancellation phase dominate?                                        | Full-witness refinement, repeated helper work; root teardown still unresolved       |
| 05 (`mem:solver/experiments/05-refinement-and-reuse`)              | 2026-08-28    | Remove redundant refinement and avoid ineligible/repeated construction?                   | 6.54x isolated replay; first solver job killed; refinement later committed          |
| 06 (`mem:solver/experiments/06-cancellation-recovery`)             | 2026-08-28    | Preserve failure evidence and locate the cancellation tail?                               | 9 valid results, 3 kills; cache destruction observed; allocator cause unproved      |
| 07 (`mem:solver/experiments/07-compact-keys-and-constructor`)      | 2026-08-28    | Reduce key storage and exact subset arithmetic?                                           | 16 verified records, no kills; strong bundle gains; six incomplete solves           |
| 08 (`mem:solver/experiments/08-repeat-scheduling`)                 | 2026-08-28    | Are gains repeatable; how do partitions and 16/32 workers compare?                        | 62 verified; p1 hard optimal 2.96x faster; 24 all/baseline median regresses 6.2%    |
| 09 (`mem:solver/experiments/09-serializer-and-find-all`)           | 2026-08-28    | Does isolated encoding explain the regression; does p14 help hard all?                    | 42 verified; encoding favors all seven medians; p14 earlier witness, all unfinished |
| 10 (`mem:solver/experiments/10-constructor-promotion`)             | 2026-08-28    | Promote the validated constructor without the deadline?                                   | Permanent, separate from compact keys; not pushed                                   |
| 11 (`mem:solver/experiments/11-compact-key-promotion`)             | 2026-08-28    | Promote compact exact keys independently?                                                 | Permanent with isolated encoding evidence; not pushed                               |
| 12 (`mem:solver/experiments/12-hard-obligation-profiling`)         | 2026-08-28    | What dominates the remaining hard N/L/profile work?                                       | 24 verified; 36 witness tail, 10 basis/labeling cost; no kills                      |
| 13 (`mem:solver/experiments/13-calculation-changes`)               | 2026-08-28    | Can early exact-L checks, cached witness leaves and direct RREF bounds reduce hard costs? | 80 verified; isolated gains support three promotions; hard whole-all still capped   |
| 14 (`mem:solver/experiments/14-calculation-promotion`)             | 2026-08-28    | Promote the three measured calculations independently?                                    | Permanent in three separate commits                                                 |
| 15 (`mem:solver/experiments/15-prefix-workloads`)                  | 2026-08-28    | Can exact subtrees supply completed hard-10 references?                                   | Exact certificates verified; all 24 hard prefixes capped; deeper discovery needed   |
| 16 (`mem:solver/experiments/16-post-calculation-screen`)           | 2026-08-28    | How do p1/p14 compare after cheaper calculations?                                         | 38 verified after analyzer fix; hard optimal 23.27% shorter; all still capped       |
| 17 (`mem:solver/experiments/17-p1-promotion-and-followups`)        | 2026-08-29    | Can p1 become a default; do the constructor, sharing, donation and deeper prefixes help?  | 180 verified; p1 gains accepted despite hard-10 memory; other candidates need work  |
| 18 (`mem:solver/experiments/18-guarded-p1-promotion`)              | 2026-08-29    | Promote p1 without the measured hard-10 memory failure?                                   | N<=9 guard validated, then rejected before commit                                   |
| 19 (`mem:solver/experiments/19-unconditional-p1-promotion`)        | 2026-08-29    | Make p1 the production policy despite its hard-10 resource cost?                          | Unconditional p1 selected; memory issue explicitly deferred                         |
| 20 (`mem:solver/experiments/20-constructor-and-medium-cases`)      | 2026-08-29    | Can post-winning constructor work be removed; what causes medium-case CPU troughs?        | 26 verified; guard permanent; troughs are uneven root-search tails                  |
| 21 (`mem:solver/experiments/21-adaptive-root-profiling`)           | 2026-08-29    | Which calculations dominate the longest 238 adaptive root?                                | 6 verified; 80.6M witness leaves dominate; best/all do identical work               |
| 22 (`mem:solver/experiments/22-analytic-witness-ports`)            | 2026-08-29    | Can exact symmetric-port labels replace their factorial witness enumeration?              | 6 verified; exact 559,872x leaf reduction; root 2.38-2.57x faster; permanent        |
| 23 (`mem:solver/experiments/23-whole-translation-and-dfs-profile`) | 2026-08-29    | Does the port gain translate whole; which exact DFS identity dominates next?              | 27 verified; 238 improves 43-52%; legal/state identity dominate next                |
| 24 (`mem:solver/experiments/24-canonical-purpose-profile`)         | 2026-08-29    | Which exact state/open-port/marked-child/SCC identity consumes canonicalization time?     | 4 verified; internal marked-child bypass is the next isolated test                  |
| 25 (`mem:solver/experiments/25-internal-dfs-marked-bypass`)        | 2026-08-30    | Can dispatched DFS defer marked-child equivalence to propagated canonical state keys?     | 12 verified; four completed pairs improve 16.6-21.1%; repeat before promotion       |
| 26 (`mem:solver/experiments/26-internal-dfs-promotion-repeat`)     | 2026-08-30    | Do repeated completed solves confirm the bypass; what owns hard-run memory?               | 16 verified; combined medians improve 17.5-22.9%; permanent; RAM is local caches    |
| 27 (`mem:solver/experiments/27-minimum-link-enumeration-mode`)     | 2026-08-30    | Can users enumerate only the proven minimum-N/minimum-L layouts?                          | Implemented across all solvers, UI and benchmark runner; differential test passed   |
| 28 (`mem:solver/experiments/28-state-open-port-coordinate-reuse`)  | 2026-08-30    | Can DFS reuse state labeling for its final open-port tie-break?                           | 14 verified; all completed pairs improve 18.2-49.4%; permanent                      |
| 29 (`mem:solver/experiments/29-deferred-state-canonicalization`)   | 2026-08-31    | Can DFS avoid exact state keys until a cheap invariant bucket repeats?                    | 26 verified; completed medians improve 18.3-38.1%; permanent                        |
| 30 (`mem:solver/experiments/30-no-state-cache-ablation`)           | 2026-08-31    | Does recursive DFS state caching improve completion at all?                               | Rejected; 12.3% slower on 115 and 3.00x slower on 238                               |
| 31 (`mem:solver/experiments/31-propagation-bounds`)                | 2026-08-31    | Can propagation remove a second copy of every physical-flow bound check?                  | 38 verified; all completed medians improve 5.36-9.40%; promote                      |
| 32 (`mem:solver/experiments/32-weighted-sparse-quotient`)          | 2026-08-31    | Can exact weighted representatives reduce sparse elimination work?                        | Rejected; 115 improves, but 36/238 regress and sparse passes cost 6.79% more        |
| 33 (`mem:solver/experiments/33-sparse-row-deduplication`)          | 2026-08-31    | Do identical normalized equations add avoidable sparse elimination work?                  | Rejected; zero duplicates across 164M hard input-row instances                      |

## Active follow-up

46 (`mem:solver/experiments/46-rref-order`) is implemented and validated in
a79ecf7, active but not promoted. It replaces only the final canonical RREF
sort/dedup with reversal of ordered unique pivot rows.
40-job plan: 20 new-candidate jobs, 12 adverse variable controls, eight
unrestricted accounting controls. 52 minutes search+cleanup, within user limit
of one hour. Exact AB/BA balance. Full tests, Clippy, release and smoke pass.
Nothing launched yet; `mem:solver/active`.

Variables/accounting are permanent in 520b352/331b87a, promotion notes in 6812173.
Prior results remain in
`mem:solver/experiments/44-topology-variable-confirmation-results` and
`mem:solver/experiments/45-cache-accounting-results`. No scheduler change or push.

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
