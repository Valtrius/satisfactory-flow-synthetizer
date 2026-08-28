# Current decisions and handoff

Updated 2026-08-28. Start at [the entry point](../custom-parallelism.md).
Historical hypotheses and results remain in the [experiment index](experiments/README.md).

## Objective

Reduce time to a proven optimum and complete minimum-node enumeration on difficult
inputs. Record first validated witness separately. CPU and memory explain costs;
lower overhead alone does not justify slower completion.

## Committed baselines

| Change                                                         | Evidence                                             | Commit                                        |
| -------------------------------------------------------------- | ---------------------------------------------------- | --------------------------------------------- |
| Lazy MRV and cheaper exact RREF/inequality arithmetic          | 792 verified runs, 1.81-2.36x serial gains           | `319191e`, published                          |
| Remove ordering-only full-witness refinement                   | 6.54x isolated replay, same key/permutation coverage | `3e804e8`, published by the user              |
| Benchmark tools, diagnostic hooks and experiment documentation | Independent commit validation completed              | `92b111c`, no push performed in this analysis |

## Candidates and promotion decision

| Candidate                                                    | Evidence                                                                                                   | Decision                                                            |
| ------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------- |
| Constructor eligibility, per-N reuse and integer subset sums | Earlier avoided helper work and 143.962 to 6.516 s construction; isolated tests; stable no-deadline screen | `099cc12`, local; see [10](experiments/10-constructor-promotion.md) |
| Compact state/SCC keys                                       | Isolated encoding comparison favors all seven short-case medians; large memory savings                     | Permanent; see [11](experiments/11-compact-key-promotion.md)        |
| Five-second constructor deadline                             | No prior expiry established a benefit                                                                      | Removed; preserved as an unapplied benchmark patch                  |
| Adaptive partitions and remaining groups                     | Earlier witnesses, but hard enumeration still incomplete                                                   | Keep opt-in; no default change                                      |

Constructor improvements are permanent in `099cc12`; compact keys are permanent
in the commit accompanying [11](experiments/11-compact-key-promotion.md). Both include
their related documentation and remain local, not pushed. The unproven helper
deadline is not included. New hard-work profiling follows in a separate commit.

## Latest measured findings

[Experiment 09 results](experiments/09-serializer-and-find-all-results.md): 42
verified records, 34 optimal and eight cooperative capped incomplete, no watchdog
kills, 40.10 process minutes. Original/rechecked analyzer summaries match. All
completed results and partial solution sets match experiment 08; source/binary
identities match the frozen run. Both timing variants use the same constructor
without a deadline and the same boxed key storage; only row encoding differs.

- 24 all/baseline, five repeats: legacy median 24.763 s versus compact 21.627 s,
  12.7% less time, 15.1% less CPU and 82.2% lower peak memory. Ranges overlap.
- All seven short-case medians favor compact encoding. States and decisions match.
  One diagnostic pair reduces summed encoding time 7.631 to 0.405 s. This weakens
  the earlier codec-regression hypothesis, but does not explain the old bundle regression.
- Hard 36 all, two repeats each: p14/32 first witness median 31.636 s versus groups/32
  95.078 s. P14 uses 70.2% more CPU and 18.0% less peak memory. All eight runs time
  out at 240 s with the same two layouts, 20 link groups and 45 profiles exhausted.
- Hard 36 optimal p1/32: 31.902 s median, same N=9/L=11 witness. No large regression
  appears after removing the optional deadline. This was not an isolated deadline A/B.
- The 10 case was not rerun. Experiment 08's capped runs remain incomplete at
  N=11/L=18 with no witness. More states or CPU did not establish faster completion.

## Next actions

1. Preserve the separate constructor and key commits as the profiling baseline.
   Their evidence and limits are recorded in [10](experiments/10-constructor-promotion.md)
   and [11](experiments/11-compact-key-promotion.md).
2. Use p1/32 for the next hard-optimal comparison and p14/32 as the next hard-all
   candidate for earlier witnesses. Do not claim faster hard enumeration yet.
3. Add a benchmark-only path for fixed exact N/L/profile work around 36 N=9/L=12
   and 10 N=11/L=18. Profile hard obligations, then select manageable complete ones
   where possible. Preserve exact witnesses and explicit incomplete/exhausted status.
4. Target repeated canonical graph construction, exact basis generation or witness
   work according to those hard traces. The short-case codec is no longer the first
   suspect. A common coarse pool is not justified by occupancy alone.

No benchmark is running. Last status:
`target/parallelism-ladder/serializer-scheduling-20260828/BENCHMARK-STATUS.txt`.
Existing difficult cases suffice. Automatic worker selection, sharing/donation
defaults, allocator replacement and a new coarse scheduler remain unpromoted.
Use only normalized inputs and runtime facts, never benchmark cyclicity labels.

## Validation records

Experiment 09 prelaunch checks: constructor-only source passed 199 solver-core
library/integration/example tests; both encoder variants passed 200 each, with two
ignored per run. All three passed strict all-target Clippy. The 22 tooling tests,
release builds and formatting passed. No solver changes or test reruns in this analysis.

The earlier isolated benchmark commit `92b111c` passed 194 solver-core tests, two
ignored, strict Clippy and 22 tooling tests in `target/benchmark-commit-validation-20260828/`
with its own target. These are package/example checks, not new full-workspace runs.
The earlier 297-test working-workspace validation remains a separate historical record.
Log paths and hashes are in [09 protocol](experiments/09-serializer-and-find-all.md)
and [08 protocol](experiments/08-repeat-scheduling.md).
