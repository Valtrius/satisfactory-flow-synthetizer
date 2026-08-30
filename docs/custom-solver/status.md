# Current decisions and handoff

Updated 2026-08-30. Start at [the entry point](../custom-parallelism.md).
Historical hypotheses and results remain in the [experiment index](experiments/README.md).

## Objective

Reduce time to a proven optimum and complete minimum-node enumeration on difficult
inputs. Record first validated witness separately. CPU and memory explain costs;
lower overhead alone does not justify slower completion.

The product now has three exact scopes: one optimum, all layouts at minimum N and
minimum L, and all layouts across every L at minimum N. The benchmark names are
`optimal`, `minimum_links`, and `all`.

## Permanent changes and commit state

| Change                                                | Evidence                                                            | Commit                                                           |
| ----------------------------------------------------- | ------------------------------------------------------------------- | ---------------------------------------------------------------- |
| Lazy MRV and exact RREF/inequality arithmetic         | 792 verified runs, 1.81-2.36x serial gains                          | `319191e`                                                        |
| Remove ordering-only full-witness refinement          | 6.54x isolated replay, same keys/permutations                       | `3e804e8`                                                        |
| Constructor eligibility, per-N reuse, integer subsets | Avoided helper work; stable no-deadline screen                      | `099cc12`, [10](experiments/10-constructor-promotion.md)         |
| Compact exact state/SCC keys                          | Seven short-case medians favor compact rows; memory savings         | `2716fac`, [11](experiments/11-compact-key-promotion.md)         |
| Fixed hard-work profiling                             | Feature-gated production search and strict local-scope verification | `1b558a6`, [12](experiments/12-hard-obligation-profiling.md)     |
| Unconditional adaptive partitions                     | 180 verified jobs; accepted hard-10 resource cost                   | `49ae34c`, [19](experiments/19-unconditional-p1-promotion.md)    |
| Skip constructor after winning N                      | 26 exact jobs; removes redundant existence work                     | `57e34c0`, [20](experiments/20-results.md)                       |
| Derive exact symmetric witness ports                  | 559,872x fewer leaves; root 2.38-2.57x faster                       | `f5df873`, [22](experiments/22-results.md)                       |
| Bypass marked-child keys inside recursive DFS         | 28 verified jobs; completed medians improve 17.5-22.9%              | `1b62e5e`, [26](experiments/26-internal-dfs-promotion-repeat.md) |

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
and makes the optimization permanent. [Experiment 23](experiments/23-results.md)
verifies the whole translation: 238 improves 43.10% all and 51.74% optimal, 115 all
improves 9.98%, while 115 and hard-36 optimal are effectively unchanged. Exact
search coverage and results agree. Remaining diagnostics put 30-52% of accounted
worker time in legal-decision identity and 28-34% in state keys. Split open-port
and marked-child costs before attempting reuse; scheduling remains paused.
[Experiment 24](experiments/24-canonical-purpose-profile.md) implements this
diagnostic-only split. Four verified jobs put 94.5-96.2% of legal-decision time in
open-port and marked-child keys. Marked keys remove fewer than 0.3% of candidate
decisions in these samples. The next candidate bypasses them only inside dispatched
DFS roots, retaining canonical root and adaptive-frontier identities.
[Experiment 25](experiments/25-internal-dfs-marked-bypass.md) verifies all 12
screening records. Its four completed pairs preserve full exact outputs and improve
wall time 16.6-21.1%. Hard-10 processes 20.4% more states within the cap but raises
sampled peak working set 26.2%. [Experiment 26](experiments/26-internal-dfs-promotion-repeat.md)
verifies all 16 repeat jobs. Combined three-sample medians improve 17.5-22.9% with
identical exact outputs, so the internal DFS bypass is permanent. Root and frontier
planning remain keyed. Current telemetry attributes hard-run RAM mainly to 32 concurrent
worker-local state and SCC caches; separate aggregate byte counters are the first step
if memory optimization resumes. Scheduler experiments remain paused. The next speed
hypothesis is to reuse state-canonical labeling for invariant open-port selection;
the promoted hard-10 diagnostic attributes 303.6 of 316.5 legal-decision seconds to
open-port keys. Treat this as a capped profile, not completion evidence.

## Validation records

Experiment 26 post-run and promotion: 16/16 repeat records complete optimally and
match their references. Combined with experiment 25, all 12 completed A/B pairs
preserve every compared exact result field. Solver-core, exhaustive reference,
parallelism and shared application tests pass; strict default and benchmark-feature
Clippy pass. See [promotion repeat](experiments/26-internal-dfs-promotion-repeat.md).

Experiment 25 post-run: 12/12 records verify; eight complete optimally and four
fixed-time diagnostics remain explicitly incomplete. All completed and capped
pairs preserve their respective exact or partial result identities. No kills or
failures occurred. See [internal DFS bypass](experiments/25-internal-dfs-marked-bypass.md).

Experiment 24 post-run: 4/4 records verify; 115/238 complete with experiment 23's
exact full results and structural counters, while hard 36/10 remain explicitly
capped. No kills or failures occurred. See
[purpose profile](experiments/24-canonical-purpose-profile.md).

Experiment 23 post-run: 27/27 records verify; 20 completed references preserve
full result identities and seven capped diagnostics remain explicitly incomplete.
No kills or failures occurred. The original and rechecked summaries have identical
SHA-256. See [results](experiments/23-results.md).

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
