# Current decisions and handoff

Updated 2026-08-28. Start at [the entry point](../custom-parallelism.md).
This is the current decision summary, not a replacement for dated experiment records.

## Objective

Reduce elapsed time to a proven optimum and to complete minimum-node enumeration,
especially on difficult inputs. Record first validated-witness latency separately.
CPU and memory explain costs; lower overhead alone does not outweigh slower completion.

## What is permanent

| Change                                                                | Evidence                                                                                                               | Delivery                           |
| --------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- | ---------------------------------- |
| Lazy canonical MRV, cheaper exact RREF and inequality arithmetic      | [792-run suite](experiments/02-exact-kernel.md), same search coverage, 1.81-2.36x serial improvements                  | `319191e`, committed and published |
| Remove ordering-only refinement from exhaustive full-witness labeling | [Isolated replays](experiments/05-refinement-and-reuse.md), 6.54x replay speedup, identical key and permutation counts | `3e804e8`, committed and published |

The user pushed `3e804e8` after the agent's HTTPS/SSH attempts failed. Remote
`develop` was subsequently verified at that commit. Earlier local reports saying
publication was blocked describe the earlier state, not the current decision.

## Implemented but not yet committed

| Candidate                                                | What the evidence supports                                               | Remaining uncertainty                                                |
| -------------------------------------------------------- | ------------------------------------------------------------------------ | -------------------------------------------------------------------- |
| Constructor eligibility and per-N completed-result reuse | Avoids known ineligible/repeated optional work; exact fallback unchanged | Whole-solve effects measured alongside other changes                 |
| Compact exact state/SCC keys                             | Strong memory/cancellation improvement in the compact screen             | Bundle result, not isolated key-only timing attribution              |
| Incremental integer subset sums                          | Instrumented constructor time 143.962 to 6.516 s                         | Single samples and other concurrent changes                          |
| Five-second constructor budget                           | Correctly defers, never proves a miss/UNSAT                              | Zero expiries in compact diagnostics; performance benefit unmeasured |

These candidates are present in the working solver. "Uncommitted" does not mean
disabled. Do not stage the whole working tree to promote one candidate.

## Benchmarking and documentation baseline

The accompanying `chore(bench)` commit records the benchmark examples, diagnostic
module and measurement hooks, manifests, runner/analyzer, regression tests and
this documentation. It does not promote the remaining solver optimizations.
Diagnostic events specific to the new constructor's eligibility/reuse/budget
remain with that uncommitted implementation. No scheduler defaults change.

The full working version passed the validation recorded in experiment 08. This
isolated benchmark-only source split receives formatting and static dependency
checks now; its standalone build/test rerun is deferred while benchmarks run.
The commit is local unless separately published by the user or an authorized push.

## Scheduling decisions

- All four scheduling flags still default to false in the application.
- Groups-only is a useful find-all comparison at large worker counts; partitions
  are a useful find-optimal comparison. Neither is an automatic universal policy.
- Sharing is mixed and can retain much more memory. Donation stays off by default.
- A common bounded pool of coarse roots across eligible groups is proposed,
  not implemented or measured. Fixed group allocations can leave capacity unused.
- Do not select policy from benchmark names or externally supplied cyclicity.
  Only normalized problem facts and information already available during search
  may affect solver decisions. See [contracts](contracts.md).

## Current measurement

[Experiment 08](experiments/08-repeat-scheduling.md) was launched with 62 jobs.
The user reports it is still running. Results have not been reviewed for this
documentation update. Do not infer a finding from a partial CSV or an elapsed cap.
The run uses frozen scripts, inputs and binaries, so documentation edits do not
change its execution. Wait for the user to report completion before analysis.

## Open questions, in order

1. Do short-case repeats confirm the compact/constructor bundle, especially the
   observed 24 find-all baseline slowdown relative to recovery?
2. On the faster kernel, does adaptive partitioning improve hard 36 completion?
   Does 16 or 32 workers offer the better elapsed-time/memory tradeoff?
3. Can hard find-all complete sooner? The last reviewed 600-second run still had
   only two partial layouts. The last reviewed 10 run had no witness at 180 s.
4. If imbalance remains after serial work is reduced, test the coarse pool.
   Refine indivisible long roots separately; queue changes alone cannot split them.
5. Full-witness enumeration remains factorial. Any future symmetry pruning must
   retain the exact public minimum key and all layouts.

Existing hard inputs are sufficient for this next decision. More shapes and
another machine remain necessary before generalizing production defaults.

## Not adopted or not established

No automatic all-stage policy, allocator replacement, cache eviction policy,
detached cleanup, new witness symmetry pruning or common coarse pool was adopted.
The first has measured regressions; allocator cause and coarse-pool benefit remain
hypotheses. Hiding cleanup or weakening proof/result coverage is not an acceptable
optimization. A time-limited optional helper is acceptable only with exact fallback.
