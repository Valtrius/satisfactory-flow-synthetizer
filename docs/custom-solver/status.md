# Current decisions and handoff

Updated 2026-08-28. Start at [the entry point](../custom-parallelism.md).
Historical hypotheses and results remain in the [experiment index](experiments/README.md).

## Objective

Reduce time to a proven optimum and complete minimum-node enumeration on difficult
inputs. Record first validated witness separately. CPU and memory explain costs;
lower overhead alone does not justify slower completion.

## Permanent changes and commit state

| Change                                                | Evidence                                                            | Commit                                                       |
| ----------------------------------------------------- | ------------------------------------------------------------------- | ------------------------------------------------------------ |
| Lazy MRV and exact RREF/inequality arithmetic         | 792 verified runs, 1.81-2.36x serial gains                          | `319191e`                                                    |
| Remove ordering-only full-witness refinement          | 6.54x isolated replay, same keys/permutations                       | `3e804e8`                                                    |
| Constructor eligibility, per-N reuse, integer subsets | Avoided helper work; stable no-deadline screen                      | `099cc12`, [10](experiments/10-constructor-promotion.md)     |
| Compact exact state/SCC keys                          | Seven short-case medians favor compact rows; memory savings         | `2716fac`, [11](experiments/11-compact-key-promotion.md)     |
| Fixed hard-work profiling                             | Feature-gated production search and strict local-scope verification | `1b558a6`, [12](experiments/12-hard-obligation-profiling.md) |

Each optimization commit includes its related docs. Experiment 13 tooling/results
are committed in `473aba7`. [Calculation promotion](experiments/14-calculation-promotion.md)
has separate commits: exact-L `5ae5631`, witness `1fbe95d`, and direct RREF bounds
included with this update. All three are permanent. No push was performed.
The constructor deadline and scheduling flags remain unchanged.

## Latest measured findings

[Experiment 13 isolated results](experiments/13-calculation-results.md) and
[whole results](experiments/13-whole-results.md): 80/80 verified, no kills or
verification failures, 28.669 process minutes. Original/rechecked summaries match;
frozen hashes and job identities pass. Current Rust sources match the measured
combined candidate after newline normalization. No source change during analysis.

- Early exact-L rejection shortens the targeted complete 36 proof 77.5-77.6%,
  4.44-4.45x, in three samples per variant/mode. Two discarded witness calls and
  2,654,208 leaves disappear. Exact results and local exhaustion agree.
- Cached witness leaves improve isolated replay 14.87x, from 15.845 to 1.066 s.
  Exact public key, all 2,985,984 leaves and 13,214,106 branches agree.
- Direct RREF bounds shorten another complete proof 7.3-8.3% in both modes, with
  lower CPU. Separate diagnostics reduce summed inequality time 36.8%.
- Real 24/65 optimal/all completion medians improve 5.6-12.3%, three samples each.
  Hard 36 optimal improves 33.274 to 24.972 s in one sample, still provisional.
- Combined 36 N=9/L=12 p1/32 finishes all five profiles and 392 roots in 62.409 s
  all, with six witnesses. Reference remains incomplete at 110 s with five, an
  exact subset. This new local reference has no fully exhausted baseline SAT set.
- Whole 36 all/p14 remains incomplete at 120 s with the same two partial witnesses.
  First witness improves 33.784 to 25.071 s in one sample. CPU and memory at the
  cap increase; complete-enumeration speed is still unknown.
- Hard 10 stays incomplete with 32 roots active at cancellation. Basis/labeling
  costs remain substantial. The new 30 s diagnostics have 0.303-0.346 s root tails.

Earlier diagnoses remain in [12](experiments/12-hard-obligation-results.md).
The current results support all three calculation changes without justifying a
new scheduler default or a universal memory-saving claim.

## Next work

1. The [three measured changes](experiments/14-calculation-promotion.md) are
   promoted separately with tests and related docs. No new benchmark is running.
2. Compare p1 versus p14 on actual hard-36 all with the new calculations. Use
   bounded repeated timing and a separate trace to locate the remaining tail.
   Repeat hard optimal before treating its single-sample gain as stable.
3. Add a benchmark-only exact partition-prefix workload for hard 10, with frozen
   decisions/frontier identity and honest local proof scope. Seek completion in
   tens of seconds before testing further exact basis or labeling changes.

See [the detailed recommendations](experiments/13-whole-results.md#recommendations).
Keep scheduling opt-in. Revisit bounded donation only with evidence of idle
workers and an expensive DFS tail. Existing cases suffice; no hard single-worker
baseline is needed. Use normalized inputs/runtime facts, never benchmark labels.

## Validation records

Experiment 13 prelaunch: exact-L and witness variants each passed 209 solver-core
library/integration/example tests, the combined basis variant passed 210, two
ignored per run. Witness/basis strict all-target Clippy passed; final default-feature
Clippy passed. Release builds, 28 Python tooling tests and the 80-job frozen plan
check passed. See [13 changes](experiments/13-calculation-changes.md) for logs.
Experiment 13 post-run verification passed all 80 records and provenance checks;
this analysis changes documentation only, not the previously tested Rust code.

Profiling prelaunch: 208 solver-core library/integration/example tests passed with
`bench-internals`, two ignored; 26 Python tests passed, including actual tiny
completion and one-second hard cancellation. Strict all-target solver-core Clippy
passed with and without the feature; release build passed. Logs are in [12](experiments/12-hard-obligation-profiling.md).

Experiment 09 prelaunch: constructor-only source passed 199 tests; both encoder
variants passed 200 each, two ignored per run. Strict all-target Clippy, 22 tooling
tests, builds and formatting passed. Earlier benchmark commit `92b111c` passed
194 package/example tests, two ignored, and strict Clippy. The 297-test workspace
validation is a separate historical record. See [08](experiments/08-repeat-scheduling.md)
and [09](experiments/09-serializer-and-find-all.md) for those logs and source splits.
