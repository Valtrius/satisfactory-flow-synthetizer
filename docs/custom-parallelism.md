# Custom solver optimization and parallelism

Start here for the optimization history and for continuing this work.
Updated 2026-08-28.

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
  are permanent in `2716fac`. Both remain local, not pushed. The isolated encoding comparison favors
  all seven short-case medians, including 24 all/baseline, with large memory savings.
  This does not establish the cause of experiment 08's earlier bundle regression.
- All experimental scheduling flags still default to false. Their benefits vary
  with solve mode, problem, worker count and memory cost.
- [The 62-job repeated comparison](custom-solver/experiments/08-repeat-scheduling.md)
  is verified: 56 optimal results, six capped incomplete, no watchdog kills.
  Adaptive partitions reduce hard 36 optimal from 92.9 to 31.4 seconds at 32 workers.
  See [the results and recommendations](custom-solver/experiments/08-repeat-scheduling-results.md).
- [Experiment 09 results](custom-solver/experiments/09-serializer-and-find-all-results.md):
  42 verified records, 34 optimal, eight incomplete, no kills. Compact encoding
  reduces 24 all/baseline median time 12.7%. P14/32 finds the first hard-36 witness
  3.01x sooner than groups/32; all hard enumeration runs remain unfinished at 240 s.
  The helper deadline stays removed.
- [Fixed hard-work profiling](custom-solver/experiments/12-hard-obligation-profiling.md)
  isolates exact N/L/profile work through the production search. The 24-job plan
  has 18.7 minutes of search caps, both collection modes and completion notification.
  Results are pending; local profile exhaustion is not a whole-solve proof.

The goal is shorter time to a proven optimum and complete minimum-node enumeration.
First-witness latency is a separate measure. More occupied workers, a partial
layout count, or reaching a time cap does not prove faster completion.

Raw evidence is under local, ignored `target/parallelism-ladder/`. Essential
results and conclusions live in the linked Markdown records so this history is
useful without the chat or the raw archives.
