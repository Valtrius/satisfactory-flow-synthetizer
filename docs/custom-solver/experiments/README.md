# Experiment index

All timings are historical observations on this machine unless stated otherwise.
Read [current status](../status.md) before treating an old recommendation as current.
Record dates follow the analysis chronology, not necessarily every run's start time.

| ID                                          | Date          | Question                                                                | Outcome                                                                             |
| ------------------------------------------- | ------------- | ----------------------------------------------------------------------- | ----------------------------------------------------------------------------------- |
| [01](01-parallelism-ladder.md)              | 2026-08-27    | Which parallelism stages help?                                          | Large-worker wins; extra CPU/memory and small-worker regressions                    |
| [02](02-exact-kernel.md)                    | 2026-08-27/28 | Can exact calculations be cheaper without changing search?              | 792 verified runs; kernel made permanent                                            |
| [03](03-hard-case-screen.md)                | 2026-08-28    | Do those findings extend to difficult inputs?                           | Slow witness/constructor phases; incomplete enumeration; N<=10 cutoff error         |
| [04](04-witness-constructor-diagnostics.md) | 2026-08-28    | Which serial work and cancellation phase dominate?                      | Full-witness refinement, repeated helper work; root teardown still unresolved       |
| [05](05-refinement-and-reuse.md)            | 2026-08-28    | Remove redundant refinement and avoid ineligible/repeated construction? | 6.54x isolated replay; first solver job killed; refinement later committed          |
| [06](06-cancellation-recovery.md)           | 2026-08-28    | Preserve failure evidence and locate the cancellation tail?             | 9 valid results, 3 kills; cache destruction observed; allocator cause unproved      |
| [07](07-compact-keys-and-constructor.md)    | 2026-08-28    | Reduce key storage and exact subset arithmetic?                         | 16 verified records, no kills; strong bundle gains; six incomplete solves           |
| [08](08-repeat-scheduling.md)               | 2026-08-28    | Are gains repeatable; how do partitions and 16/32 workers compare?      | 62 verified; p1 hard optimal 2.96x faster; 24 all/baseline median regresses 6.2%    |
| [09](09-serializer-and-find-all.md)         | 2026-08-28    | Does isolated encoding explain the regression; does p14 help hard all?  | 42 verified; encoding favors all seven medians; p14 earlier witness, all unfinished |
| [10](10-constructor-promotion.md)           | 2026-08-28    | Promote the validated constructor without the deadline?                 | Permanent, separate from compact keys; not pushed                                   |
| [11](11-compact-key-promotion.md)           | 2026-08-28    | Promote compact exact keys independently?                               | Permanent with isolated encoding evidence; not pushed                               |

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
