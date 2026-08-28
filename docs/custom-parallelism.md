# Custom kernel and scheduling

## Permanent kernel decision, 28 August 2026

The lazy canonical MRV selection and exact sparse arithmetic optimizations are
permanent production behavior for both solve modes. They have no opt-out flag.
The eager and dense versions exist only in differential tests. The measured
solver implementation is unchanged by this decision.

The scheduling ladder remains available for comparison through native options
and the benchmark runner. No experimental scheduling flag is enabled by default.
Keeping this comparison code does not select it for the application.

### Next scheduling decision

The primary objective is lower elapsed time to a proven optimum and to complete
minimum-node enumeration, especially on difficult inputs. Measure first validated
witness latency separately. More CPU or memory can be justified by lower elapsed
time within the user's resource budget; lower overhead alone is not a win.

Before choosing defaults, add representative hard inputs and freeze a new
post-kernel baseline. Keep the current small cases as regression and overhead
checks. They cannot predict scheduling behavior on long searches.

The next proposed scheduler experiment is a single worker pool consuming coarse
root tasks from eligible profiles and L-groups. Find-optimal initially distributes
the current group's work. Find-all can distribute remaining groups after the
first satisfiable group, using the existing eligibility rule. Workers that finish
one group can then help another without a fixed per-group worker allocation.
Bound queued task state and measure planning time, replay cost, task duration,
idle worker time and the final slow task. If a few indivisible roots dominate,
test bounded refinement of those roots as a separate follow-up. A common queue
alone cannot shorten an already-running indivisible root.

Keep exact group-scoped memoization, deterministic witness selection, witness
publication, cancellation and exhaustion accounting. Do not infer proof from
queue emptiness or an in-flight
task, weaken optimality, or drop enumeration results. Initially leave fine DFS
sibling donation off and compare completed-state sharing separately. The present
results do not establish a universal sharing policy or refinement threshold.

This is a proposed next experiment, not an implemented or measured improvement.
Compare it with baseline, partitions, groups, partitions + groups and the best
measured sharing configuration for each mode. First use a small screening matrix,
then repeat the finalists with randomized order and the same worker budgets.
Choose on completion times, variability and timeout counts; use CPU, memory and
instrumentation to explain the result and identify resource limits.

### Hard-case intake

Collect roughly 6–10 representative cases, including slow single-solution search,
long enumeration, feedback/cyclic constructions, duplicate terminal rates and
capacity-constrained inputs. Prefer some cases taking tens of seconds to several
minutes with the new kernel, plus a few known timeout cases. These are selection
targets, not eligibility requirements. Uniformly scaling rates is not a useful
way to create harder cases because normalization can produce the same problem.

For each case, record exact input and output rates, maximum link rate, node bound
if any, worker count, mode, observed elapsed time/status and any known witness.
Use a file-based case manifest in the next runner revision rather than adding a
Rust example for every input. Keep input data and expected proof status together.

Keep two benchmark cohorts. Completed reference cases must still match optimum
and complete enumeration identities. Stress cases may time out; record them as
censored observations, validate any incumbent, retain the last proven bound and
never call a partial layout set exhaustive. Add explicit stress-result handling
to the runner/analyzer rather than weakening the existing completed-run gate.
Include nontrivial exhausted UNSAT obligations where available, distinguishing
finite node-bound exhaustion from global UNSAT.

## Complete rerun, 28 August 2026

All **792 fresh-process runs** passed verification: 396 find-all and 396
find-optimal. Each configuration has three samples. The suite covered three
cases, 1/4/16/32 workers, all nine scheduling configurations after the kernel
change, and baseline/full-stack configurations from preserved before-change
executables. The random schedule mixed variants and modes. Optional hotspot
recording was off. Total measured process time was 3.61 hours.

The baseline scheduler isolates the calculation changes. At **one worker**:

| Mode         | Case                   |   Before |   After | Speedup |
| ------------ | ---------------------- | -------: | ------: | ------: |
| Find all     | 65 → 40 + 25           | 141.16 s | 78.17 s |   1.81× |
| Find all     | 24 → 7 + 6 + 5 + 4 + 2 | 155.88 s | 71.05 s |   2.19× |
| Find optimal | 65 → 40 + 25           |   5.87 s |  2.48 s |   2.36× |
| Find optimal | 24 → 7 + 6 + 5 + 4 + 2 |  11.27 s |  4.83 s |   2.33× |

The baseline search-state and decision counts are unchanged. Serial process CPU
fell by about 45–57%, supporting a calculation-cost improvement rather than a
change in search coverage. These medians supersede the preliminary diagnostic
timings, which had hotspot recording enabled and only one sample per variant.

At **32 workers**, selected after-change configurations produced these medians:

| Mode         | Configuration        |   40+25 | 7+6+5+4+2 |
| ------------ | -------------------- | ------: | --------: |
| Find all     | Baseline             | 31.70 s |   33.33 s |
| Find all     | Groups only          | 12.41 s |   13.32 s |
| Find all     | Partitions + groups  | 15.19 s |   10.10 s |
| Find all     | Full stack           | 15.53 s |    9.63 s |
| Find optimal | Baseline             |  1.35 s |    2.75 s |
| Find optimal | Partitions only      |  0.49 s |    1.88 s |
| Find optimal | Partitions + sharing |  0.40 s |    1.88 s |
| Find optimal | Full stack           |  0.44 s |    1.88 s |

### Recommendations from this run

- Keep the kernel optimizations for both modes. They reduce CPU cost with no new
  shared state or changed mathematical contract.
- For find-all on a 16/32-worker budget, start with **groups only** if CPU cost
  matters. At 32 workers it uses 98/101 CPU-seconds for these two cases, versus
  219/118 for partitions + groups. Its peak working sets are 401/432 MiB.
- **Partitions + groups** is a useful memory-conscious alternative: 285/221 MiB
  at 32 workers. It gets within 0.47 s of the full stack on the second case while
  avoiding the full stack's 724 MiB peak. Its first case is slower and uses much
  more CPU than groups only, so it is not a universal winner.
- For find-optimal, prefer **partitions only** as the simpler latency option.
  Completed-state sharing is worth retaining as an optional addition: it saves
  about 96 ms on 40+25 at 32 workers, but does not improve the second case.
  Remaining-group concurrency is inactive in this mode.
- Leave donation disabled by default. Its small win on one find-all case does
  not justify enabling the full stack globally. On the tiny case at 32 workers,
  full-stack find-all takes 26.65 ms versus baseline's 7.01 ms; find-optimal takes
  9.68 ms versus 1.53 ms.
- Keep the baseline scheduler for small budgets until coarse worker allocation
  is improved. At four workers, groups-only 40+25 regresses from 32.66 to 48.38 s.
  The current fixed per-group allocations do not rebalance when a group finishes.
  A shared pool of coarse root tasks is the next scheduling experiment, not a
  demonstrated fix. It must preserve per-group exhaustion and witness accounting.

No parallelism defaults were changed. The kernel improvements are active in the
ordinary solver; these scheduler choices remain opt-in. Before wider defaults,
add more hard shapes, finite unsatisfiable cases, and another machine. Three
samples of two hard problems and one tiny problem cannot establish a universal
worker threshold. Several find-all timing ranges overlap.

Next calculation targets are the remaining state/decision canonicalization,
propagation, and complete-witness evaluation. In the separate serial diagnostic,
40+25 still spends about 20 s evaluating complete states. Split that bucket
before choosing the next optimization. Do not add overlapping timers together.

Raw data, schedules, executable hashes, per-run exact keys and verified summaries
are in `target/parallelism-ladder/kernel-full-suite/`. Before/after source snapshots
and binaries are preserved in sibling directories. Solver source still matches
the measured after-change snapshot. Validation passed with 287 Rust tests,
5 ignored, strict workspace Clippy, 88 frontend tests, frontend checks, formatting
and four benchmark-analyzer regression tests. Very short runs can have zero
recorded process CPU due to timer resolution; that does not mean zero CPU work.

## Kernel optimization and both solve modes

The follow-up experiment keeps every parallelism flag opt-in, and changes the
shared calculation kernel for both `solve_with_observer` and
`enumerate_with_observer`:

- Canonical MRV selection first filters on the inexpensive ranking fields, then
  computes canonical port keys only for the tied finalists. The exact selected
  orbit and final raw-reference tie-break remain unchanged.
- Exact RREF borrows a pivot row's nonzero suffix instead of cloning the row and
  multiplying every zero entry. Unit pivots avoid division, and unit factors
  avoid multiplication.
- Inequality reduction skips zero coefficients, and positive-scale normalization
  computes denominator/GCD work only for nonzero entries. Arithmetic stays exact.

Differential tests compare selection with the original eager implementation, and
the normalized equality/inequality rows with the original dense arithmetic,
including fractions, contradictions and very large integers. Existing relabeling,
rollback and independent-reference tests remain active. Scheduler tests exercise
both solve modes, cancellation, and the two added combinations `p14` and `p124`.

The profiling examples accept positional arguments:

```text
<timeout-seconds> <workers> <max-nodes> <engine> <stage> <output-json> <all|optimal> <on|off>
```

The final argument controls optional hotspot recording. The benchmark script
defaults to `off`; separate diagnostic runs use `on`. `optimal` calls the actual
single-result API, without running full enumeration or stopping enumeration at
its first event. JSON includes the terminal witness, mode, independently validated
result identities, first validated-witness time, and canonicalization sub-buckets.
Sub-buckets overlap the existing search buckets and must not be summed with them.

The runner defaults to both modes, all nine stage combinations, 1/4/16/32 workers,
and three repetitions. It can interleave preserved executables from before the
kernel change with the current build, using a recorded random seed. Existing
output directories are rejected to prevent accidental mixing of runs.

```powershell
cargo build --release -p solver-core --example profile_40_25 --example profile_7_6_5_4_2 --example profile_tiny
./scripts/parallelism-ladder.ps1 -Cases profile_40_25,profile_7_6_5_4_2,profile_tiny -ReferenceBinaryDirectory target/parallelism-ladder/kernel-before-bin
python scripts/test_analyze_parallelism.py
```

The optional `profile_tiny` case uses 2+3 → 1+4 with a two-node bound, exposing
fixed scheduling costs. For very short processes a sampled working-set peak of
zero means no live memory sample was obtained, not zero memory consumption.

Verification compares full result sets within each mode and checks that the
single optimum belongs to the full enumeration with the same objective. It does
not require the optimal-mode witness to equal enumeration's preferred witness.
Results from before and after the kernel change are grouped separately, and the
analyzer rejects incomplete schedules, incomplete solves, result differences,
and mixed hotspot settings within one summary group.

## Earlier ladder results, 27 August 2026

On the Ryzen 9 7950X3D with 32 logical workers, three fresh-process runs per
stage produced these median solver wall times before the kernel optimizations.
These are historical measurements on baseline revision `fda1a5d`, not a portable
speed guarantee or the current implementation's timings.

| Stage      | 65 → 40 + 25 | 24 → 7 + 6 + 5 + 4 + 2 |
| ---------- | -----------: | ---------------------: |
| `baseline` |      67.58 s |                83.29 s |
| `p1`       |      31.34 s |                27.45 s |
| `p12`      |      26.16 s |                32.11 s |
| `p123`     |      28.78 s |                33.12 s |
| `p1234`    |      20.76 s |                16.10 s |

The full stack improved median wall time by 3.26× and 5.17×, but used about
1.53× and 1.15× the baseline CPU time. Its maximum sampled peak working sets
were 676 MiB and 730 MiB, versus 214 MiB and 223 MiB for baseline. Deeper
partitions alone peaked at 168 MiB and 175 MiB.

Single-run checks at four workers regressed with the full stack: 70.61 s
versus 67.56 s for 40+25, and 80.70 s versus 62.11 s for the second case.
Single-worker baseline/full-stack timings were 164.98/161.25 s and
177.42/167.29 s. Isolated shared-cache runs at 32 workers took 65.04/79.57 s;
isolated parallel-group runs took 32.67/31.85 s. These isolated and scaling
checks have one sample each.

All 42 measured runs completed with `Optimal`, the exact baseline key sets
of 13 and 6 layouts, and the same preferred witnesses. Raw measurements,
binary hashes, and exact result keys are under `target/parallelism-ladder/`.
The repeated baseline for 40+25 ranged from 52.44 to 68.88 s despite identical
work counters, so use the recorded ranges as well as medians.

The main gains came from deeper frontiers and overlapping remaining groups.
The cache and donation stages were not consistently faster. Production
defaults remain unchanged, including for low worker counts.

## Controls

All `ParallelismOptions` flags default to `false`. The shared application adapter
continues to use those defaults. These experiments are selectable through native
`solver_core::SolveOptions` and the profiling examples, not through new UI controls.

| Stage      | Enabled controls                                                   |
| ---------- | ------------------------------------------------------------------ |
| `baseline` | None                                                               |
| `p1`       | Adaptive static partitions                                         |
| `p12`      | Adaptive partitions and completed-state sharing                    |
| `p123`     | Adaptive partitions, completed-state sharing, and sibling donation |
| `p1234`    | All four controls, including remaining L-groups                    |
| `shared`   | Completed-state sharing alone                                      |
| `groups`   | Parallel remaining L-groups alone                                  |
| `p14`      | Adaptive partitions and parallel remaining L-groups                |
| `p124`     | Adaptive partitions, completed-state sharing and parallel groups   |

`work_stealing` requires `shared_state_cache`. An invalid combination returns an
options error. `parallel_remaining_groups` applies only to enumeration, after
the first satisfiable L-group at the minimum N has completed.

## Correctness contracts

The adaptive planner replaces a prefix with its legal continuation frontier.
Planning, replay, and DFS share propagation, SCC, and reachability checks.
Complete prefixes remain searchable leaves; rejected children require a direct
proof. Interrupted planning returns incomplete. Refinement stops at depth four,
the target of four roots per requested worker per profile, or its 250 ms budget.
The budget is checked between expensive operations, not a hard deadline.
Frontier IDs are assigned after canonical sorting. Timing can change the final
frontier, so IDs are local to the registered plan.

The shared cache contains **completed** states only. It never treats an in-flight
owner as a proof and never waits for another cache owner. Concurrent duplicates
may still compute the same state. Keys are scoped by the exact target L as well
as the full canonical state. A group-owned witness registry publishes validated
witnesses before terminal SAT entries. Borrowers recover a concrete best witness;
group enumeration collects every registered witness, including those found by
donated tasks.

The donation scheduler uses a bounded shared deque. A DFS frame can donate
siblings but joins every donated result before caching its own terminal status.
Joining workers execute queued work, including with one worker. Each child has
one completion slot. Incomplete and failed children prevent parent exhaustion.
Unwinding donated-task failures become failures rather than leaving an unresolved
join. The workspace release profile still uses `panic = "abort"`, so a release
panic terminates the process instead of returning a Rust error.

Static root leaves remain the proof-ledger boundary. Dynamic tasks do not create
new ledger IDs. The parent DFS frame returns only after its joins complete.

Remaining L-groups use a bounded number of group slots. Their worker allocations
sum to the requested worker budget, including when there are more groups than
workers. Reporter and coordinator threads are additional non-search threads.
The coordinator registers the complete group set before dispatch, imports local
ledgers into matching pending groups, and folds results in L/profile/root order.
Enumeration success checks every expected group's exhaustion, not merely SAT
existence at N.

## Validation

`crates/solver-core/tests/parallelism.rs` compares complete native result maps and
the preferred witness across all stages at 1, 4, and 32 workers. It separately
normalizes layout identities for comparison with the independent reference
enumerator. Other tests cover borrowed SAT results, exact-L cache isolation,
delayed children, donated-task failure, cancellation, and incomplete group gates.

```powershell
cargo test -p solver-core
cargo clippy --workspace --all-targets -- -D warnings
```

## Reproducing the measurements

Build before running timings. Do not run builds or tests alongside benchmarks.

```powershell
cargo build --release -p solver-core --example profile_40_25 --example profile_7_6_5_4_2
./scripts/parallelism-ladder.ps1 -Repeats 3 -Workers 32
```

The script randomizes the schedule and saves the order, revision, compiler,
machine, executable hashes, raw logs, JSON result identities, and quoted CSV.
Each sample starts a fresh process. It does not silently discard a warmup run.
Use a separate output directory for warmup or exploratory measurements.

```powershell
./scripts/parallelism-ladder.ps1 -Stages baseline,shared,groups -Workers 1,4,32
python scripts/analyze-parallelism.py target/parallelism-ladder/RUN_DIRECTORY --output target/parallelism-ladder/summary.json
```

Each result JSON includes the full native key set, preferred key, status, solver
wall time, diagnostic counters, and summed hotspot elapsed timers. The script
also records OS process CPU time and sampled peak working set. Accounted timers
are not CPU time; CPU utilization uses whole-process wall time. Cache payload
counters exclude allocator overhead, task snapshots, and witness storage, and
must not be presented as whole-process memory.

The analyzer fails on incomplete runs, a changed preferred witness, or any full
key-set difference, even if the number of layouts is unchanged. Speedup compares
median solver wall time against baseline at the same worker count.
