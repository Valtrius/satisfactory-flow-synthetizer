# Custom solver optimization and parallelism

Start here for the optimization history and for continuing this work.
Updated 2026-09-01.

## Read in this order

1. [Current decisions and handoff](custom-solver/status.md): permanent changes,
   candidates, pending measurement and open questions.
2. [Experiment history](custom-solver/experiments/README.md): every major trial,
   its evidence, limitations and subsequent decision.
3. [Correctness contracts](custom-solver/contracts.md): proof, identity,
   cancellation and what the solver is allowed to know.
4. [Controls and code map](custom-solver/controls.md): flags, algorithm boundaries
   and source files for an agent joining the work.
5. [Benchmark guide](custom-solver/benchmarking.md): cases, reproducibility,
   interpretation and completion notifications.
6. [Documentation workflow](custom-solver/maintenance.md): update these records
   with each meaningful change, result, failure and decision.

## Current summary

- Exact MRV/RREF/inequality optimizations are permanent in `319191e`.
- Removal of ordering-only full-witness refinement is permanent in `3e804e8`.
  Its isolated witness replay improved 6.54x with identical keys and coverage.
- [Constructor improvements](custom-solver/experiments/10-constructor-promotion.md)
  are permanent in `099cc12`. [Compact exact keys](custom-solver/experiments/11-compact-key-promotion.md)
  are permanent in `2716fac`. Both are present in the local `origin/develop` tracking ref.
  The isolated encoding comparison favors
  all seven short-case medians, including 24 all/baseline, with large memory savings.
  This does not establish the cause of experiment 08's earlier bundle regression.
- Native scheduling flags still default to false. The shared production Custom
  adapter now selects adaptive partitions unconditionally. Other scheduler flags
  remain off.
- [The 62-job repeated comparison](custom-solver/experiments/08-repeat-scheduling.md)
  is verified: 56 optimal results, six capped incomplete, no watchdog kills.
  Adaptive partitions reduce hard 36 optimal from 92.9 to 31.4 seconds at 32 workers.
  See [the results and recommendations](custom-solver/experiments/08-repeat-scheduling-results.md).
- [Experiment 09 results](custom-solver/experiments/09-serializer-and-find-all-results.md):
  42 verified records, 34 optimal, eight incomplete, no kills. Compact encoding
  reduces 24 all/baseline median time 12.7%. P14/32 finds the first hard-36 witness
  3.01x sooner than groups/32; all hard enumeration runs remain unfinished at 240 s.
  The helper deadline stays removed.
- [Fixed hard-work results](custom-solver/experiments/12-hard-obligation-results.md):
  24 verified, 14 local exhaustions, ten incomplete, no kills; 14.028 process minutes.
  The 36 tail is in full-witness canonicalization; 10 keeps 32 workers active and
  spends heavily on exact bases and graph labeling. Local exhaustion is not a whole solve.
  [Next experiments](custom-solver/experiments/12-next-experiments.md) target those costs;
  no new scheduler default is justified.
- [Calculation candidates](custom-solver/experiments/13-calculation-changes.md) are
  permanent through [separate promotion](custom-solver/experiments/14-calculation-promotion.md). [80 verified jobs](custom-solver/experiments/13-calculation-results.md)
  support separate promotion: targeted exact-L proof improves 4.44-4.45x, witness
  replay 14.87x, and completed basis workloads 7.3-8.3%. [Whole results](custom-solver/experiments/13-whole-results.md)
  improve short-case completion medians 5.6-12.3%; hard 36 optimal improves 25.0%
  in one sample. Its exact N=9/L=12 group now completes, but full all remains capped.
  [Exact prefix tooling](custom-solver/experiments/15-prefix-workloads.md) is implemented.
  The [38-job follow-up results](custom-solver/experiments/16-post-calculation-results.md)
  pass after an analyzer reference-selection fix. Hard optimal is 23.27% shorter
  over three samples per variant. P1 closes L=12 with less CPU/memory than p14 at
  240 s, but neither completes all. Every hard-10 prefix capped.
  [Next steps](custom-solver/experiments/16-diagnostics-and-next-steps.md) identified
  optional constructor work, sharing/donation tests and deeper prefix discovery.
- [Experiment 17 results](custom-solver/experiments/17-results.md) verify 180 jobs.
  P1 wins completed N<=9 work at 16/32 workers but exceeds baseline memory 2.84-6.32x
  on hard N=11. The memory gate was explicitly waived in favor of completion time.
  Constructor and p123 remain candidates; depth-10/pick-4 is a completed prefix control.
- The [N<=9 guard](custom-solver/experiments/18-guarded-p1-promotion.md) was validated
  then rejected before commit. [Unconditional p1](custom-solver/experiments/19-unconditional-p1-promotion.md)
  is the production policy for both modes. Sharing, donation and remaining groups stay off.
- [Experiment 20](custom-solver/experiments/20-results.md) verifies the post-winning
  constructor guard across 26 jobs and makes it permanent. The mixed timing screen
  does not establish a general speedup; it preserves exact results and removes
  redundant existence work. Medium-case CPU troughs track uneven root-search tails,
  sometimes leaving one root running alone. Scheduler experiments remain paused.
- [Experiment 21 results](custom-solver/experiments/21-results.md) verify the frozen
  longest 238 root. Best/all perform identical work; 80.6 million full-witness
  leaves consume about 34 seconds. [Experiment 22 results](custom-solver/experiments/22-results.md)
  preserve exact results while reducing witness leaves 559,872x and isolated root
  time 58-61%. The calculation change is permanent. [Experiment 23](custom-solver/experiments/23-results.md)
  verifies whole translation: 238 improves 43-52% and 115 all improves 10%, with
  identical exact results and search coverage. Hard-36 optimal is unchanged and
  all remains capped. Legal-decision and state canonicalization dominate next;
  their open-port/marked-child purposes need a diagnostic split. Scheduling stays
  paused. [Experiment 24](custom-solver/experiments/24-canonical-purpose-profile.md)
  verifies the diagnostic split across four jobs. Open-port and marked-child keys
  account for 94.5-96.2% of legal-decision time. Marked keys remove fewer than 0.3%
  of measured candidates, so an internal-DFS-only bypass is the next isolated test;
  root partition identities remain canonical. [Experiment 25](custom-solver/experiments/25-internal-dfs-marked-bypass.md)
  verifies 12 screening records. Four completed pairs preserve exact outputs and
  improve 16.6-21.1%. [Experiment 26](custom-solver/experiments/26-internal-dfs-promotion-repeat.md)
  verifies 16 repeat records. Combined three-sample medians improve 17.5-22.9%, so
  the internal DFS bypass is permanent. Existing telemetry attributes hard-run RAM
  mainly to concurrent worker-local exact caches. Scheduling remains paused.
- [Experiment 27](custom-solver/experiments/27-minimum-link-enumeration-mode.md)
  adds a third exact solve scope across Custom, Z3, Reference, Tauri, the UI, and
  benchmark tooling. [Experiment 28](custom-solver/experiments/28-state-open-port-coordinate-reuse.md)
  reuses the state labeling for recursive DFS open-port selection. All six completed
  A/B pairs preserve exact outputs and improve 18.2-49.4%, so the change is permanent.
  The capped hard-10 diagnostic advances more work with 12.8% higher sampled memory.
- [Experiment 29](custom-solver/experiments/29-deferred-state-canonicalization.md)
  tests deferred state labeling inside recursive DFS. Cheap fingerprints only decide
  when to compute exact canonical keys; they never authorize pruning or reuse. All 26
  records verify, and six completed medians improve 18.3-38.1%. The change is
  permanent. [Experiment 30](custom-solver/experiments/30-no-state-cache-ablation.md)
  directly tests whether local memoization still pays for itself. No cache is 12.3%
  slower on 115 minimum L and 3.00x slower on 238 optimal, so complete removal is
  rejected. Scheduling stays paused.
- [Experiment 31](custom-solver/experiments/31-propagation-bounds.md) splits the
  remaining propagation cost and removes a duplicate positivity/capacity pass. All
  38 jobs verify; six completed medians improve 5.36-9.40%, so promotion is
  recommended and permanent in `9f63f01`.
- [Experiment 32](custom-solver/experiments/32-weighted-sparse-quotient.md) rewrites
  sparse equations through already-proved weighted representatives before exact
  elimination. All 38 records verify, but 36/238 regress and hard sparse time per pass
  rises 6.79%. The unconditional candidate is rejected and absent from source.
  Scheduling stays paused.
- [Experiment 33](custom-solver/experiments/33-sparse-row-deduplication.md) isolates
  adjacent deduplication after the sparse system's existing row sort. All 26 records
  verify, but the hard candidate removes zero rows across 164 million input-row
  instances. The candidate is rejected and absent from source.
- [Experiment 34](custom-solver/experiments/34-sparse-phase-profile.md) adds a
  diagnostic-only split of sparse preparation, elimination, back reduction, and
  deduction work. It buckets total time by matrix shape and records why analyses are
  repeated. [All six records verify](custom-solver/experiments/34-sparse-phase-profile-results.md).
  Preparation consumes 55.2-84.0% of sparse time and active matrices are usually
  small, so preparation replaces an incremental Bareiss basis as the next target.
  [Experiment 35](custom-solver/experiments/35-sparse-preparation-profile.md) now
  splits that preparation work into five diagnostic components. Its
  [six verified records](custom-solver/experiments/35-sparse-preparation-profile-results.md)
  put 89.99-91.42% of preparation in substitution and normalization. Test a
  fully-known integer evaluation path before changing mixed-row arithmetic.
  [Experiment 36](custom-solver/experiments/36-fully-known-row-substitution.md)
  implements that isolated candidate. [All 14 records verify](custom-solver/experiments/36-fully-known-row-substitution-results.md):
  completed medians improve 9.73-16.41% with identical exact work, so the change is
  permanent in `532aaa1`. [Experiment 37](custom-solver/experiments/37-no-known-row-substitution.md)
  isolates the no-known-variable clone path. [All 14 records verify](custom-solver/experiments/37-no-known-row-substitution-results.md):
  completed medians improve 3.42-5.37%, so the exact identity path is permanent in
  `109667c`. [Experiment 38](custom-solver/experiments/38-mixed-row-integer-substitution.md)
  implements the remaining mixed-row integer path. [All 14 records verify](custom-solver/experiments/38-mixed-row-integer-substitution-results.md):
  completed algebra medians improve 2.43-3.05% with identical exact work. Wall medians
  move -0.60% on 115 and +3.16% on 238. The integer path is permanent.
  [Experiment 39](custom-solver/experiments/39-canonicalization-reprofile.md) reuses
  the retained purpose and subphase timers on fixed completed and capped hard work to
  select the next calculation target from the current production distribution.
  [All four records verify](custom-solver/experiments/39-canonicalization-reprofile-results.md):
  state keys own 99.96-100.00% of graph-purpose time, and equality plus inequality
  encoding consume 58.9-67.2% of state/SCC canonicalization. Split those calculations
  next; labeling is no longer the first target.
  [Experiment 40](custom-solver/experiments/40-equality-inequality-subphase-profile.md)
  implements that hotspot-only split. Full validation and a capped reconciliation
  smoke pass; its fixed four-job screen is ready to launch. Scheduling stays paused.

The goal is shorter time to a proven optimum, complete minimum-link enumeration,
and complete minimum-node enumeration. Benchmark modes are `optimal` for one layout,
`minimum_links` for every layout at minimum N and minimum L, and `all` for every L
at minimum N.
First-witness latency is a separate measure. More occupied workers, a partial
layout count, or reaching a time cap does not prove faster completion.

Raw evidence is under local, ignored `target/parallelism-ladder/`. Essential
results and conclusions live in the linked Markdown records so this history is
useful without the chat or the raw archives.
