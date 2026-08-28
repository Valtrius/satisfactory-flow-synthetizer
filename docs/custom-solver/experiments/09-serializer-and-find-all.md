# 09. Isolated serialization and hard find-all scheduling

Date: 2026-08-28. State: analyzed; 42 verified records, no watchdog kills.
See [results and decisions](09-serializer-and-find-all-results.md).
Follow-up to [08 results](08-repeat-scheduling-results.md); original protocol below.

## Questions

1. Does compact semantic serialization account for the 24 all/baseline CPU increase
   when constructor code, helper policy and key allocation are identical?
2. Can partitions plus groups shorten hard 36 enumeration, including its initial
   SAT group, compared with groups alone at the same worker count?

## Source separation

The active constructor retains normalized-rate eligibility, per-N completed-result
reuse, exact integer subset sums and cancellation checks. Its provisional five-second
deadline was removed. No observed expiry had established a benefit. The old deadline
and deferred-state test are preserved in
`benchmarks/custom/variants/constructor-five-second-budget.patch`, not applied here.
This restores unbounded helper duration, not uninterruptible work. A difficult helper
can again consume the solve budget; capped hard optimal runs watch for that risk.

The constructor promotion candidate comprises `acyclic_incumbent.rs`, the crate-local
denominator export in `lower_bound.rs`, and coordination in `solver.rs`. It is tested
against committed `canonical.rs` in a detached checkout before building the reference.
No constructor or serializer commit is claimed by this experiment.

The serializer reference applies
`benchmarks/custom/variants/legacy-semantic-encoding.patch` to current sources.
Before uses legacy dense decimal rows; after uses compact sparse exact rows.
Both retain exact-sized boxed key storage and identical constructor code with no
helper deadline. Only semantic row encoding and test-only unused writers differ.
Public witness encoding, exact calculations, scheduling defaults and labels stay fixed.
This measures encoding, not boxing, the prior bundle, or a constructor-only speedup.

## Manifest and comparison

`benchmarks/custom/serializer-scheduling-screening.json`: 42 fresh randomized jobs,
max rate 1200. All workers are 32 unless specified. No one-worker hard baselines.

| Jobs | Case/mode/stage                  | Variants     | Repeats | Cap per job |
| ---: | -------------------------------- | ------------ | ------: | ----------: |
|   10 | 24 all baseline                  | before/after |       5 |        45 s |
|    4 | 24 all groups                    | before/after |       2 |        45 s |
|    4 | 65 all baseline                  | before/after |       2 |        45 s |
|    4 | 24/65 optimal baseline           | before/after |       1 |        10 s |
|    8 | 24/65 optimal p1                 | before/after |       2 |        10 s |
|    2 | 24 all baseline, diagnostics     | before/after |       1 |        45 s |
|    2 | 36 optimal p1                    | after        |       2 |       120 s |
|    8 | 36 all groups/p14, 16/32 workers | after        |       2 |       240 s |

Timing jobs have hotspots off. The two instrumented jobs use the separate corpus
alias `acyclic24_diagnostic`, the same exact case file and hotspots on. This keeps
their timing out of the uninstrumented aggregates. No corpus alias affects policy.

Search caps total 53 minutes, plus a 60-second cancellation watchdog per job if
needed. Expected duration is about 40 minutes from previous samples, not a guarantee.
The difficult jobs may remain incomplete; do not turn their caps into speedup ratios.

## Validation and provenance

The constructor-only source passed 199 solver-core library/integration/example
tests and strict all-target Clippy. The current compact variant passed 200 tests
and strict Clippy. Each test run has two ignored tests. All 22 runner/analyzer
tests passed. The reference encoding also passed 200 tests, two ignored, and strict
Clippy. Both release builds passed. Only `canonical.rs` differs between
the two serializer source trees, matching the preserved encoding patch.
`npm run format` passed. This is package/example validation, not a new workspace run.
Use one target per checkout. Both release builds run from the main repository with
the same local Cargo configuration; the reference uses an explicit manifest path.

Preparation archive: `target/parallelism-ladder/isolation-preparation-20260828/`.
It preserves the pre-edit diff, old constructor, isolated `constructor-candidate.patch`,
reference source and `comparison-provenance.json`. The launcher freezes current
source, both binaries, scripts, manifest and case files. SHA256:

- Before: `e019d4f846dfbbcb45168cd2c2112d7f00ee59dfac88913a4aa487dc49a53a1d`.
- After: `4e356ef36d181c1e979615c3d533c7fe0a0071d5f92fff77cbed3afdf43c555f`.

Logs under `target/`: `constructor-isolation-tests.log`,
`constructor-isolation-clippy.log`, `serializer-{compact,reference}-{tests,clippy,build}.log`,
`serializer-tool-tests.log` and `serializer-format.log`. The manifest passed PlanOnly
validation. All documentation links and the 120-line size check passed.

Run: `target/parallelism-ladder/serializer-scheduling-20260828/`.
`BENCHMARK-STATUS.txt` is authoritative. Completion/failure files and a Windows
dialog were enabled. Finished at 16:39:30 +02:00, with 34 optimal and eight capped
incomplete records. The preserved analyzer reproduces its summary exactly. See
the linked results for cross-run identity verification, timing caveats and decisions.

## Analysis and decision criteria

- Verify every scheduled identity, binary hash, exact problem, status, preferred
  witness and full saved solution set. Check incomplete proofs against known optima.
- Report five-repeat medians/ranges for 24 baseline. Compare CPU and retained work
  counters, then inspect semantic encoding/algebra timers separately. Summed timers
  are not CPU and the diagnostic pair alone is not a timing benchmark.
- Report memory alongside runtime. If encoding is slower for equal work, investigate
  allocation/conversion in the codec before discarding its storage benefit.
- For hard all, compare completion first, first witness second, and finished proof
  obligations separately. More layouts or CPU at the cap do not establish completion.
- Keep p1/32 as the hard optimal reference. Automatic scheduler selection, new
  coarse queues and the 10-case per-state diagnosis remain future work.
- Review constructor promotion separately after validation and the no-deadline
  regression screen. Do not promote compact keys or the deadline with that commit.
