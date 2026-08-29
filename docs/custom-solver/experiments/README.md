# Experiment index

All timings are historical observations on this machine unless stated otherwise.
Read [current status](../status.md) before treating an old recommendation as current.
Record dates follow the analysis chronology, not necessarily every run's start time.

| ID                                          | Date          | Question                                                                                  | Outcome                                                                             |
| ------------------------------------------- | ------------- | ----------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------- |
| [01](01-parallelism-ladder.md)              | 2026-08-27    | Which parallelism stages help?                                                            | Large-worker wins; extra CPU/memory and small-worker regressions                    |
| [02](02-exact-kernel.md)                    | 2026-08-27/28 | Can exact calculations be cheaper without changing search?                                | 792 verified runs; kernel made permanent                                            |
| [03](03-hard-case-screen.md)                | 2026-08-28    | Do those findings extend to difficult inputs?                                             | Slow witness/constructor phases; incomplete enumeration; N<=10 cutoff error         |
| [04](04-witness-constructor-diagnostics.md) | 2026-08-28    | Which serial work and cancellation phase dominate?                                        | Full-witness refinement, repeated helper work; root teardown still unresolved       |
| [05](05-refinement-and-reuse.md)            | 2026-08-28    | Remove redundant refinement and avoid ineligible/repeated construction?                   | 6.54x isolated replay; first solver job killed; refinement later committed          |
| [06](06-cancellation-recovery.md)           | 2026-08-28    | Preserve failure evidence and locate the cancellation tail?                               | 9 valid results, 3 kills; cache destruction observed; allocator cause unproved      |
| [07](07-compact-keys-and-constructor.md)    | 2026-08-28    | Reduce key storage and exact subset arithmetic?                                           | 16 verified records, no kills; strong bundle gains; six incomplete solves           |
| [08](08-repeat-scheduling.md)               | 2026-08-28    | Are gains repeatable; how do partitions and 16/32 workers compare?                        | 62 verified; p1 hard optimal 2.96x faster; 24 all/baseline median regresses 6.2%    |
| [09](09-serializer-and-find-all.md)         | 2026-08-28    | Does isolated encoding explain the regression; does p14 help hard all?                    | 42 verified; encoding favors all seven medians; p14 earlier witness, all unfinished |
| [10](10-constructor-promotion.md)           | 2026-08-28    | Promote the validated constructor without the deadline?                                   | Permanent, separate from compact keys; not pushed                                   |
| [11](11-compact-key-promotion.md)           | 2026-08-28    | Promote compact exact keys independently?                                                 | Permanent with isolated encoding evidence; not pushed                               |
| [12](12-hard-obligation-profiling.md)       | 2026-08-28    | What dominates the remaining hard N/L/profile work?                                       | 24 verified; 36 witness tail, 10 basis/labeling cost; no kills                      |
| [13](13-calculation-changes.md)             | 2026-08-28    | Can early exact-L checks, cached witness leaves and direct RREF bounds reduce hard costs? | 80 verified; isolated gains support three promotions; hard whole-all still capped   |
| [14](14-calculation-promotion.md)           | 2026-08-28    | Promote the three measured calculations independently?                                    | Permanent in three separate commits                                                 |
| [15](15-prefix-workloads.md)                | 2026-08-28    | Can exact subtrees supply completed hard-10 references?                                   | Exact certificates verified; all 24 hard prefixes capped; deeper discovery needed   |
| [16](16-post-calculation-screen.md)         | 2026-08-28    | How do p1/p14 compare after cheaper calculations?                                         | 38 verified after analyzer fix; hard optimal 23.27% shorter; all still capped       |
| [17](17-p1-promotion-and-followups.md)      | 2026-08-29    | Can p1 become a default; do the constructor, sharing, donation and deeper prefixes help?  | 180 verified; p1 gains accepted despite hard-10 memory; other candidates need work  |
| [18](18-guarded-p1-promotion.md)            | 2026-08-29    | Promote p1 without the measured hard-10 memory failure?                                   | N<=9 guard validated, then rejected before commit                                   |
| [19](19-unconditional-p1-promotion.md)      | 2026-08-29    | Make p1 the production policy despite its hard-10 resource cost?                          | Unconditional p1 selected; memory issue explicitly deferred                         |
| [20](20-constructor-and-medium-cases.md)    | 2026-08-29    | Can post-winning constructor work be removed; what causes medium-case CPU troughs?        | 26 verified; guard permanent; troughs are uneven root-search tails                  |
| [21](21-adaptive-root-profiling.md)         | 2026-08-29    | Which calculations dominate the longest 238 adaptive root?                                | 6 verified; 80.6M witness leaves dominate; best/all do identical work               |
| [22](22-analytic-witness-ports.md)          | 2026-08-29    | Can exact symmetric-port labels replace their factorial witness enumeration?              | 6 verified; exact 559,872x leaf reduction; root 2.38-2.57x faster; permanent        |

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
