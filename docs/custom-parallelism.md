# Custom solver optimization and parallelism

Start here for the optimization history and for continuing this work.
Updated 2026-08-28.

## Read in this order

1. [Current decisions and handoff](custom-solver/status.md): permanent changes,
   uncommitted candidates, pending measurement and open questions.
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
- Compact exact keys and constructor improvements remain uncommitted candidates.
  The last reviewed screen had ten optimal results, six capped incomplete results
  and no watchdog kills. Hard 36 optimal took about 100 seconds.
- All experimental scheduling flags still default to false. Their benefits vary
  with solve mode, problem, worker count and memory cost.
- [The 62-job repeated comparison](custom-solver/experiments/08-repeat-scheduling.md)
  is launched and awaiting analysis. No findings from that run are claimed here.

The goal is shorter time to a proven optimum and complete minimum-node enumeration.
First-witness latency is a separate measure. More occupied workers, a partial
layout count, or reaching a time cap does not prove faster completion.

Raw evidence is under local, ignored `target/parallelism-ladder/`. Essential
results and conclusions live in the linked Markdown records so this history is
useful without the chat or the raw archives.
