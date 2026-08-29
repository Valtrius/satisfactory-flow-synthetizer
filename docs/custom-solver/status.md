# Current decisions and handoff

Updated 2026-08-29. Start at [the entry point](../custom-parallelism.md).
Historical hypotheses and results remain in the [experiment index](experiments/README.md).

## Objective

Reduce time to a proven optimum and complete minimum-node enumeration on difficult
inputs. Record first validated witness separately. CPU and memory explain costs;
lower overhead alone does not justify slower completion.

## Permanent changes and commit state

| Change                                                | Evidence                                                            | Commit                                                        |
| ----------------------------------------------------- | ------------------------------------------------------------------- | ------------------------------------------------------------- |
| Lazy MRV and exact RREF/inequality arithmetic         | 792 verified runs, 1.81-2.36x serial gains                          | `319191e`                                                     |
| Remove ordering-only full-witness refinement          | 6.54x isolated replay, same keys/permutations                       | `3e804e8`                                                     |
| Constructor eligibility, per-N reuse, integer subsets | Avoided helper work; stable no-deadline screen                      | `099cc12`, [10](experiments/10-constructor-promotion.md)      |
| Compact exact state/SCC keys                          | Seven short-case medians favor compact rows; memory savings         | `2716fac`, [11](experiments/11-compact-key-promotion.md)      |
| Fixed hard-work profiling                             | Feature-gated production search and strict local-scope verification | `1b558a6`, [12](experiments/12-hard-obligation-profiling.md)  |
| Unconditional adaptive partitions                     | 180 verified jobs; accepted hard-10 resource cost                   | `49ae34c`, [19](experiments/19-unconditional-p1-promotion.md) |
| Skip constructor after winning N                      | 26 exact jobs; removes redundant existence work                     | This commit, [20](experiments/20-results.md)                  |
| Derive exact symmetric witness ports                  | 559,872x fewer leaves; root 2.38-2.57x faster                       | `f5df873`, [22](experiments/22-results.md)                    |

Each optimization commit includes its related docs. Experiment 13 tooling/results
are committed in `473aba7`. [Calculation promotion](experiments/14-calculation-promotion.md)
has separate commits: exact-L `5ae5631`, witness `1fbe95d`, and direct RREF bounds
`af1682c`. All three are permanent. No push was performed.
Exact prefix tooling is committed in `809f7c9`.
Experiment 17 includes benchmark-only p12/p123 fixed-work access. The validated
[N<=9 guard](experiments/18-guarded-p1-promotion.md) was rejected before commit.
[Unconditional p1](experiments/19-unconditional-p1-promotion.md) is the shared
production Custom policy for both modes. The constructor candidate is absent from
the working production source.

## Latest measured findings

[Experiment 16 results](experiments/16-post-calculation-results.md): 38/38 verified,
35.126 process minutes, no kills or solver failures. The original final analyzer
failed on six p1 optimal jobs because it required baseline scheduling. The narrow
reference-selection fix passes all records without changing summary statistics.
Frozen scripts reproduce the original outcome; source/binary identities and exact
result checks pass. Rust sources still match the measured combined snapshot.

- Hard 36 optimal, p1/32, three samples each: median 32.129 to 24.652 s, 23.27%
  shorter. Before range 32.003-32.373, after 24.029-25.065 s. Full preferred
  solution and proof agree across all six runs.
- Hard 36 all, two repeats each: p1 and p14 both cap at 240 s. P1 closes L=12 and
  reaches L=13, with nine partial witnesses; p14's earliest unfinished group is
  L=12, with two visible witnesses. Exact common full solution objects agree.
  First-witness medians are 24.485/24.616 s. P1 uses about 13.1% less CPU and
  492-493 MiB peak versus p14's 699-728 MiB. No whole-all speedup is established.
- Single diagnostic runs identify 26.366 s of p1 optional constructor work after
  the winning group. P1 ends with one active DFS root for 4.617 s; p14 leaves eight
  freed workers unreassigned while three groups retain eight workers each.
- All 24 hard-10 prefix jobs cap at 30 s without witnesses. Two tiny controls
  exhaust. Certificates and scope checks work, but no completed hard control exists.

[Experiment 13](experiments/13-calculation-results.md) remains the isolated evidence
for the three calculations: exact-L 4.44-4.45x, witness replay 14.87x, direct bounds
7.3-8.3% shorter. Its [whole results](experiments/13-whole-results.md) improve 24/65
optimal/all medians 5.6-12.3% and complete the fixed 36 N=9/L=12 group in 62.409 s.
Neither experiment establishes complete hard enumeration or a universal scheduler.

## Current decision

[Experiment 17](experiments/17-results.md) verifies 180/180 records in 38.135 process
minutes. Unconditional p1 fails the hard-10 memory gate by 2.84-6.32x. P1 improves
every completed N<=9 workload at 16/32 workers, including hard-36 optimal by 64-73%.
The user explicitly accepted the memory cost and deferred that issue. Production
therefore uses p1 at every N for both modes. Sharing, donation and parallel remaining
groups remain off.

[Experiment 20](experiments/20-results.md) verifies all 26 jobs and exact result
identities. The post-winning constructor guard is permanent. The timing screen is
mixed: 24 all improves 2.51%, 65 and 115 are effectively neutral, and the noisy
two-sample 238 comparison regresses 9.36% despite identical structural work. The
guard's basis is proof-state redundancy plus the earlier measured 26.366-second
hard-36 opportunity, not a claimed universal speedup.

The 238/115 process samples attribute their low-CPU intervals to uneven root-search
tails, not the optional constructor. Sharing, donation and parallel remaining groups
stay paused. [Experiment 21](experiments/21-results.md) verifies the isolated 238
root: best/all do identical structural work and 80,621,568 witness leaves consume
about 34 seconds. [Experiment 22](experiments/22-analytic-witness-ports.md) replaces
only the 559,872-fold symmetric-port factor with an exact analytic minimum. The
six-job replay preserves every exact identity, reduces the root median 58.03-61.15%
and makes the optimization permanent. [Experiment 23](experiments/23-whole-translation-and-dfs-profile.md)
prepares whole-solve A/B plus updated state/legal-decision diagnostics. It will test
whether canonical labeling or DFS identity construction is the next safe target.

## Validation records

Experiment 22 post-run: 6/6 selected-root workloads verify and match experiment 21
requests, plan identities, statuses and full solutions. No kills, failures or open
activity spans occurred. The rechecked summary is byte-identical. See
[results](experiments/22-results.md).

Experiment 22 prelaunch: 183 solver-core tests pass with two manual benchmarks
ignored, plus one exhaustive outer differential, four parallelism integrations and
26 example tests. Strict all-target Clippy passes with `bench-internals`. The first
protocol-tag ordering attempt failed the outer oracle and was corrected before any
benchmark.

Experiment 21 post-run: 6/6 frozen root workloads verify with identical requests,
plan identity and exact witness. Reverification preserves the summary SHA-256. No
kills, failures or open activity spans occurred. See [results](experiments/21-results.md).

Experiment 18 guarded trial: 212 solver-core library/integration/example tests passed,
two ignored; six synthetizer-app unit and ten shared-API tests passed. Strict
all-target solver-core Clippy passed with and without `bench-internals`. Experiment
19 passes 211 solver-core tests, 16 application/shared-API tests, 32 analyzer tests
and both strict Clippy configurations. Details and logs are in its promotion note.

Experiment 17 post-run: 20/20 fixed and 160/160 whole records pass frozen
verification and exact-result checks; no kills or failed processes. Rechecked
summaries equal originals. Both 78-file variant snapshots and whole binary mappings
pass hashes. See [results](experiments/17-results.md).

Experiment 17 prelaunch: reference and candidate each passed 211 solver-core
library/integration/example tests, two ignored. Strict all-target Clippy passed with
`bench-internals`; candidate default-feature Clippy passed. 32 Python tooling tests,
release builds, formatting and the frozen 180-job plan check passed. See [17](experiments/17-p1-promotion-and-followups.md).

Experiment 16 analysis: 32 Python tests pass, including nonbaseline before-reference
acceptance, missing-reference rejection and changed-full-object rejection.
Log: `target/post-calculation-verifier-tests.log`. No Rust edits in this turn.

Prefix work: 211 solver-core library/integration/example tests passed, two ignored;
strict all-target Clippy passed with and without `bench-internals`; release examples
built. 30 Python tests passed, including native prefix identity/scope rejection. The
38-job frozen plan passed; see the protocol for source identity.
Logs: `target/prefix-{tests,clippy,default-clippy,build,tool-tests-final}.log`.

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
