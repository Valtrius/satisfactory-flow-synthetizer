# Experiment index

All timings are historical observations on this machine unless stated otherwise.
Read [current status](../status.md) before treating an old recommendation as current.
Record dates follow the analysis chronology, not necessarily every run's start time.

| ID                                            | Date          | Question                                                                                  | Outcome                                                                             |
| --------------------------------------------- | ------------- | ----------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------- |
| [01](01-parallelism-ladder.md)                | 2026-08-27    | Which parallelism stages help?                                                            | Large-worker wins; extra CPU/memory and small-worker regressions                    |
| [02](02-exact-kernel.md)                      | 2026-08-27/28 | Can exact calculations be cheaper without changing search?                                | 792 verified runs; kernel made permanent                                            |
| [03](03-hard-case-screen.md)                  | 2026-08-28    | Do those findings extend to difficult inputs?                                             | Slow witness/constructor phases; incomplete enumeration; N<=10 cutoff error         |
| [04](04-witness-constructor-diagnostics.md)   | 2026-08-28    | Which serial work and cancellation phase dominate?                                        | Full-witness refinement, repeated helper work; root teardown still unresolved       |
| [05](05-refinement-and-reuse.md)              | 2026-08-28    | Remove redundant refinement and avoid ineligible/repeated construction?                   | 6.54x isolated replay; first solver job killed; refinement later committed          |
| [06](06-cancellation-recovery.md)             | 2026-08-28    | Preserve failure evidence and locate the cancellation tail?                               | 9 valid results, 3 kills; cache destruction observed; allocator cause unproved      |
| [07](07-compact-keys-and-constructor.md)      | 2026-08-28    | Reduce key storage and exact subset arithmetic?                                           | 16 verified records, no kills; strong bundle gains; six incomplete solves           |
| [08](08-repeat-scheduling.md)                 | 2026-08-28    | Are gains repeatable; how do partitions and 16/32 workers compare?                        | 62 verified; p1 hard optimal 2.96x faster; 24 all/baseline median regresses 6.2%    |
| [09](09-serializer-and-find-all.md)           | 2026-08-28    | Does isolated encoding explain the regression; does p14 help hard all?                    | 42 verified; encoding favors all seven medians; p14 earlier witness, all unfinished |
| [10](10-constructor-promotion.md)             | 2026-08-28    | Promote the validated constructor without the deadline?                                   | Permanent, separate from compact keys; not pushed                                   |
| [11](11-compact-key-promotion.md)             | 2026-08-28    | Promote compact exact keys independently?                                                 | Permanent with isolated encoding evidence; not pushed                               |
| [12](12-hard-obligation-profiling.md)         | 2026-08-28    | What dominates the remaining hard N/L/profile work?                                       | 24 verified; 36 witness tail, 10 basis/labeling cost; no kills                      |
| [13](13-calculation-changes.md)               | 2026-08-28    | Can early exact-L checks, cached witness leaves and direct RREF bounds reduce hard costs? | 80 verified; isolated gains support three promotions; hard whole-all still capped   |
| [14](14-calculation-promotion.md)             | 2026-08-28    | Promote the three measured calculations independently?                                    | Permanent in three separate commits                                                 |
| [15](15-prefix-workloads.md)                  | 2026-08-28    | Can exact subtrees supply completed hard-10 references?                                   | Exact certificates verified; all 24 hard prefixes capped; deeper discovery needed   |
| [16](16-post-calculation-screen.md)           | 2026-08-28    | How do p1/p14 compare after cheaper calculations?                                         | 38 verified after analyzer fix; hard optimal 23.27% shorter; all still capped       |
| [17](17-p1-promotion-and-followups.md)        | 2026-08-29    | Can p1 become a default; do the constructor, sharing, donation and deeper prefixes help?  | 180 verified; p1 gains accepted despite hard-10 memory; other candidates need work  |
| [18](18-guarded-p1-promotion.md)              | 2026-08-29    | Promote p1 without the measured hard-10 memory failure?                                   | N<=9 guard validated, then rejected before commit                                   |
| [19](19-unconditional-p1-promotion.md)        | 2026-08-29    | Make p1 the production policy despite its hard-10 resource cost?                          | Unconditional p1 selected; memory issue explicitly deferred                         |
| [20](20-constructor-and-medium-cases.md)      | 2026-08-29    | Can post-winning constructor work be removed; what causes medium-case CPU troughs?        | 26 verified; guard permanent; troughs are uneven root-search tails                  |
| [21](21-adaptive-root-profiling.md)           | 2026-08-29    | Which calculations dominate the longest 238 adaptive root?                                | 6 verified; 80.6M witness leaves dominate; best/all do identical work               |
| [22](22-analytic-witness-ports.md)            | 2026-08-29    | Can exact symmetric-port labels replace their factorial witness enumeration?              | 6 verified; exact 559,872x leaf reduction; root 2.38-2.57x faster; permanent        |
| [23](23-whole-translation-and-dfs-profile.md) | 2026-08-29    | Does the port gain translate whole; which exact DFS identity dominates next?              | 27 verified; 238 improves 43-52%; legal/state identity dominate next                |
| [24](24-canonical-purpose-profile.md)         | 2026-08-29    | Which exact state/open-port/marked-child/SCC identity consumes canonicalization time?     | 4 verified; internal marked-child bypass is the next isolated test                  |
| [25](25-internal-dfs-marked-bypass.md)        | 2026-08-30    | Can dispatched DFS defer marked-child equivalence to propagated canonical state keys?     | 12 verified; four completed pairs improve 16.6-21.1%; repeat before promotion       |
| [26](26-internal-dfs-promotion-repeat.md)     | 2026-08-30    | Do repeated completed solves confirm the bypass; what owns hard-run memory?               | 16 verified; combined medians improve 17.5-22.9%; permanent; RAM is local caches    |
| [27](27-minimum-link-enumeration-mode.md)     | 2026-08-30    | Can users enumerate only the proven minimum-N/minimum-L layouts?                          | Implemented across all solvers, UI and benchmark runner; differential test passed   |
| [28](28-state-open-port-coordinate-reuse.md)  | 2026-08-30    | Can DFS reuse state labeling for its final open-port tie-break?                           | 14 verified; all completed pairs improve 18.2-49.4%; permanent                      |
| [29](29-deferred-state-canonicalization.md)   | 2026-08-31    | Can DFS avoid exact state keys until a cheap invariant bucket repeats?                    | 26 verified; completed medians improve 18.3-38.1%; permanent                        |
| [30](30-no-state-cache-ablation.md)           | 2026-08-31    | Does recursive DFS state caching improve completion at all?                               | Rejected; 12.3% slower on 115 and 3.00x slower on 238                               |
| [31](31-propagation-bounds.md)                | 2026-08-31    | Can propagation remove a second copy of every physical-flow bound check?                  | 38 verified; all completed medians improve 5.36-9.40%; promote                      |
| [32](32-weighted-sparse-quotient.md)          | 2026-08-31    | Can exact weighted representatives reduce sparse elimination work?                        | Rejected; 115 improves, but 36/238 regress and sparse passes cost 6.79% more        |
| [33](33-sparse-row-deduplication.md)          | 2026-08-31    | Do identical normalized equations add avoidable sparse elimination work?                  | Rejected; zero duplicates across 164M hard input-row instances                      |

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
   baseline regression. [Results](08-repeat-scheduling-results.md) motivate a p14
   hard find-all comparison and isolated serializer/constructor work next.
8. Isolated encoding favors compact rows and reduces their diagnostic cost 18.8x.
   The old bundle regression remains unexplained. P14 improves first-witness latency
   without completing hard enumeration. [09 results](09-serializer-and-find-all-results.md)
   recommend separate constructor/key promotion and fixed hard-proof diagnostics next.
9. [Fixed hard-work results](12-hard-obligation-results.md) isolate a serial witness
   tail in 36 and a busy worker pool in 10. [Next experiments](12-next-experiments.md)
   prioritize earlier exact-L rejection, cheaper witness leaves and exact basis work.
10. [Isolated calculation results](13-calculation-results.md) confirm those gains.
    [Whole results](13-whole-results.md) improve completed short solves and finish
    the fixed hard-36 group, but not full hard enumeration. Recompare p1/p14 with
    the cheaper calculations; seek a complete exact prefix for hard 10.
11. [Post-calculation results](16-post-calculation-results.md) repeat the hard-optimal
    gain. P1 closes L=12 with less CPU/memory than p14 at the cap; neither finishes
    all. [Diagnostics](16-diagnostics-and-next-steps.md) identify optional constructor
    work after the winning group, a p1 DFS tail and fixed p14 worker allocations.
    Test the helper first, then isolate sharing/donation. All hard prefixes capped.
12. [The focused promotion screen](17-p1-promotion-and-followups.md) evaluates p1
    against current baseline across modes and worker counts. It separately measures
    the post-winning constructor guard, p1/p12/p123 fixed work and deeper prefixes.
    P1 fails the predeclared hard-10 memory gate but improves every completed N<=9
    workload at 16/32 workers. Constructor and p123 advance work but are not promoted.
13. The [guarded p1 proposal](18-guarded-p1-promotion.md) passed validation, then the
    user rejected memory as a promotion gate before commit.
14. [Unconditional p1 promotion](19-unconditional-p1-promotion.md) applies adaptive
    partitions for every N in the shared production Custom adapter. Sharing, donation
    and remaining groups stay off. The hard-10 memory cost remains documented.
15. [The constructor and medium-case results](20-results.md) preserve all exact
    outputs across 26 jobs. Timing is mixed, but the guard removes redundant
    post-winning work and becomes permanent. Process samples show that the medium-case
    CPU troughs are uneven root-search tails, sometimes with one root left running.
16. [Adaptive root profiling](21-adaptive-root-profiling.md) freezes one production
    p1 root plan and replays its 82.731-second tail independently. [Six verified
    results](21-results.md) isolate 80.6 million witness leaves and show that best
    and all perform identical canonicalization work.
17. [Analytic witness ports](22-analytic-witness-ports.md) retain terminal/node
    permutations but derive the exact minimum symmetric ports. [Six verified
    results](22-results.md) preserve every exact result, reduce witness leaves
    559,872x and improve the isolated root 2.38-2.57x. The change is permanent.
18. [Whole translation and DFS profiling](23-whole-translation-and-dfs-profile.md)
    compares the preserved pre-port solver with current production on 115, 238 and
    hard 36, then profiles hard 10 and remaining whole DFS/canonicalization costs.
    [Results](23-results.md) verify 27 jobs. The 238 whole solve improves 43-52%,
    115 all improves 10%, and hard-36 optimal is unchanged. Legal-decision and state
    canonicalization dominate the remaining measured work. Split their exact key
    purposes before changing DFS identity construction.
19. [Canonicalization purpose profiling](24-canonical-purpose-profile.md) splits
    graph calls into state, open-port, marked-child and other keys, and counts SCC
    keys. Four verified jobs attribute 94.5-96.2% of legal-decision time to open and
    marked keys. Marked keys remove fewer than 0.3% of measured candidates but also
    stabilize root obligations. Test bypass only inside dispatched DFS roots.
20. [Internal DFS marked-child bypass](25-internal-dfs-marked-bypass.md) retains
    keyed root/frontier identities while testing raw decisions inside dispatched
    roots. Its four completed screening pairs improve 16.6-21.1% with identical
    exact outputs. Run two more samples per variant before production promotion.
21. [The promotion repeat](26-internal-dfs-promotion-repeat.md) verifies all 16 new
    jobs. Combined three-sample medians improve 17.5-22.9% with identical exact
    outputs, so the internal bypass becomes permanent. Existing telemetry attributes
    hard-run RAM mainly to the sum of worker-local state and SCC caches. Measure those
    two categories separately before optimizing memory itself.
22. [Minimum-link enumeration](27-minimum-link-enumeration-mode.md) adds a third
    exact solve scope across Custom, Z3 and Reference. It exhausts only the first
    satisfiable L group at minimum N and is available to the benchmark runner as
    `minimum_links`.
23. [State-coordinate reuse](28-state-open-port-coordinate-reuse.md) removes the
    recursive DFS open-port individualization pass while retaining keyed root and
    frontier planning. All six completed pairs preserve exact outputs and improve
    18.2-49.4%, so the change is permanent. Hard 10 advances 45% more states within
    the common cap while sampled working set rises 12.8%.
24. [Deferred state canonicalization](29-deferred-state-canonicalization.md) stores
    the first state under a cheap invariant and promotes a repeated bucket to exact
    canonical keys. All 26 records verify; six completed medians improve 18.3-38.1%,
    so promotion is recommended.
25. [The no-state-cache ablation](30-no-state-cache-ablation.md) confirms that local
    memoization pays for itself. No cache is 12.3% slower on 115 minimum L and 3.00x
    slower on 238 optimal, where structural decisions rise 4.33x. The fail-fast gate
    rejects the candidate before the 36 control or larger suite.
26. [Propagation-bound profiling](31-propagation-bounds.md) attributes substantial
    hard-run time to a second evaluation of the same positivity and capacity
    predicates. All 38 jobs verify; six completed medians improve 5.36-9.40%, while
    the capped hard run processes 3.62% more decisions. Promotion is recommended.
27. [The weighted sparse quotient](32-weighted-sparse-quotient.md) substitutes only
    exact facts already established by propagation before Bareiss elimination. All 38
    records verify, but the mixed whole-solve result and 6.79% higher sparse cost per
    hard pass reject the unconditional candidate. The source is restored.
28. [Sparse row deduplication](33-sparse-row-deduplication.md) isolates the quotient's
    cheapest operation after the existing sort. All 26 records verify, but the hard
    candidate removes zero rows across 164 million input-row instances. Restore source
    and stop pursuing duplicate equations.
29. [Sparse phase and matrix-shape profiling](34-sparse-phase-profile.md) measures
    preparation, Bareiss forward elimination, back reduction, and deduction extraction.
    It also buckets analysis time by matrix shape and distinguishes initial analysis
    from value and ratio reanalysis. [Results](34-sparse-phase-profile-results.md)
    put 55.2-84.0% of sparse time in preparation and reject a rollback-aware Bareiss
    basis as the next change.
30. [Sparse preparation profiling](35-sparse-preparation-profile.md) splits the
    dominant preparation phase into variable collection, substitution and
    normalization, tautology filtering, sorting, and working-row conversion. The
    [six verified results](35-sparse-preparation-profile-results.md) put 89.99-91.42%
    of preparation in substitution and normalization. Test exact integer evaluation
    of fully known rows before rewriting mixed-row arithmetic.
31. [Fully known row substitution](36-fully-known-row-substitution.md) evaluates
    equations whose coefficients are all known with one integer denominator LCM.
    [All 14 records verify](36-fully-known-row-substitution-results.md). Completed
    medians improve 9.73-16.41% with identical exact work, and hard 10 processes
    6.56% more states within the cap. The change is permanent.
32. [No-known row substitution](37-no-known-row-substitution.md) returns an already
    primitive row directly when none of its coefficients has a known value. Fully
    known and mixed behavior remain unchanged. [All 14 records verify](37-no-known-row-substitution-results.md).
    Completed medians improve 3.42-5.37%; the single capped hard sample processes
    2.40% less work. The exact clone path is permanent.
33. [Mixed-row integer substitution](38-mixed-row-integer-substitution.md) replaces
    repeated rational normalization in the remaining substitution case with one
    denominator LCM and integer residual. [All 14 records verify](38-mixed-row-integer-substitution-results.md).
    Completed algebra medians improve 2.43-3.05% with identical exact work. Whole-solve
    wall medians move -0.60% and +3.16%. The integer path is permanent.
34. [Canonicalization reprofile](39-canonicalization-reprofile.md) reuses the retained
    purpose and subphase counters on the original two completed and two capped hard
    workloads. [All four records verify](39-canonicalization-reprofile-results.md).
    State keys now own 99.96-100.00% of graph-purpose time. Equality and inequality
    encoding consume 58.9-67.2% of combined state/SCC canonicalization. Split those
    calculations next; labeling and legal-decision keys are no longer the first target.
35. [Equality and inequality subphase profile](40-equality-inequality-subphase-profile.md)
    splits canonical port indexing, equation construction, rational RREF, bound-row
    construction, primitive normalization, and sort/deduplication only when hotspot
    recording is active. Full validation and a capped 115 reconciliation smoke pass;
    its four-job fixed-work screen is ready to launch.

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

Use [the template](template.md) for the next distinct experiment. Do not erase
failed records when replacing a runner or promoting a successful optimization.
