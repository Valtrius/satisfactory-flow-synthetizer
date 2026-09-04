# Current decisions and handoff

Updated 2026-09-04. Hot path: `mem:critical_info` → `mem:solver/core`. Live: `mem:solver/active` (experiment54 promoting this commit; experiment51 committed `7799c11`; both unpushed).
Historical records: `mem:solver/experiments/index`.

## Objective

Reduce time to a proven optimum and complete minimum-node enumeration on difficult
inputs. Record first validated witness separately. CPU and memory explain costs;
lower overhead alone does not justify slower completion.

The product now has three exact scopes: one optimum, all layouts at minimum N and
minimum L, and all layouts across every L at minimum N. The benchmark names are
`optimal`, `minimum_links`, and `all`.

Experiment 28 is permanent. Recursive DFS reuses the existing state labeling for
its final open-port MRV tie-break. All six completed A/B pairs preserve exact outputs
and improve 18.2-49.4%. Root/frontier identity and p1 scheduling are unchanged.

Experiment 29 (`mem:solver/experiments/29-deferred-state-canonicalization`) is permanent. It
defers exact state labeling for the first visit to a cheap invariant bucket, then
promotes repeated buckets to authoritative canonical keys. All six completed medians
improve 18.3-38.1% with exact outputs intact.

## Permanent changes and commit state

| Change                                                | Evidence                                                            | Commit                                                                    |
| ----------------------------------------------------- | ------------------------------------------------------------------- | ------------------------------------------------------------------------- |
| Complete propagation variables                        | Exp44: 53/75 completed wall pairs favorable; retain adverse cells   | `520b352`, permanent after user approval                                  |
| Allocation-free cache accounting                      | Exp45: 21/24 completed wall pairs favorable; 24/24 CPU              | `331b87a`, permanent after user approval                                  |
| Lazy MRV and exact RREF/inequality arithmetic         | 792 verified runs, 1.81-2.36x serial gains                          | `319191e`                                                                 |
| Remove ordering-only full-witness refinement          | 6.54x isolated replay, same keys/permutations                       | `3e804e8`                                                                 |
| Constructor eligibility, per-N reuse, integer subsets | Avoided helper work; stable no-deadline screen                      | `099cc12`, 10 (`mem:solver/experiments/10-constructor-promotion`)         |
| Compact exact state/SCC keys                          | Seven short-case medians favor compact rows; memory savings         | `2716fac`, 11 (`mem:solver/experiments/11-compact-key-promotion`)         |
| Fixed hard-work profiling                             | Feature-gated production search and strict local-scope verification | `1b558a6`, 12 (`mem:solver/experiments/12-hard-obligation-profiling`)     |
| Unconditional adaptive partitions                     | 180 verified jobs; accepted hard-10 resource cost                   | `49ae34c`, 19 (`mem:solver/experiments/19-unconditional-p1-promotion`)    |
| Skip constructor after winning N                      | 26 exact jobs; removes redundant existence work                     | `57e34c0`, 20 (`mem:solver/experiments/20-results`)                       |
| Derive exact symmetric witness ports                  | 559,872x fewer leaves; root 2.38-2.57x faster                       | `f5df873`, 22 (`mem:solver/experiments/22-results`)                       |
| Bypass marked-child keys inside recursive DFS         | 28 verified jobs; completed medians improve 17.5-22.9%              | `1b62e5e`, 26 (`mem:solver/experiments/26-internal-dfs-promotion-repeat`) |
| Reuse state coordinates for DFS open-port selection   | 14 verified jobs; completed pairs improve 18.2-49.4%                | 28 (`mem:solver/experiments/28-state-open-port-coordinate-reuse`)         |
| Defer state keys until an invariant repeats           | 26 verified jobs; completed medians improve 18.3-38.1%              | 29 (`mem:solver/experiments/29-deferred-state-canonicalization`)          |
| Remove duplicate physical-flow bounds                 | 38 verified jobs; completed medians improve 5.36-9.40%              | `9f63f01`, 31 (`mem:solver/experiments/31-propagation-bounds`)            |
| Evaluate fully known rows with integer arithmetic     | 14 verified jobs; completed medians improve 9.73-16.41%             | `532aaa1`, 36 (`mem:solver/experiments/36-fully-known-row-substitution`)  |
| Reuse sparse rows with no known coefficients          | 14 verified jobs; completed medians improve 3.42-5.37%              | `109667c`, 37 (`mem:solver/experiments/37-no-known-row-substitution`)     |
| Substitute mixed rows with integer arithmetic         | 14 verified jobs; completed algebra improves 2.43-3.05%             | 38 (`mem:solver/experiments/38-mixed-row-integer-substitution`)           |

Experiment 29 remains unchanged after the no-cache ablation. Experiment 30 was rejected
and its source changes were restored before this result-only documentation update.

Experiment 31 (`mem:solver/experiments/31-propagation-bounds`) is permanent in `9f63f01`. Its
38-job frozen A/B verifies exactly, and all six completed medians improve 5.36-9.40%.

Experiment 32 (`mem:solver/experiments/32-weighted-sparse-quotient`) is rejected and absent
from the working solver source. Its exact outputs verify, but 36 and 238 regress and
hard sparse time per pass rises 6.79%. The next smaller calculation is row
deduplication without representative projection.

Experiment 33 (`mem:solver/experiments/33-sparse-row-deduplication`) is rejected and absent
from source. All 26 records verify, but the hard candidate removes zero duplicates
across 164,042,086 input-row instances. Stop pursuing duplicate equations.

Experiment 34 (`mem:solver/experiments/34-sparse-phase-profile`) adds diagnostic-only sparse
phase, reanalysis-cause, and matrix-shape counters behind the existing hotspot
recorder. Ordinary solves retain the timer-free analysis path. Full correctness,
Clippy, formatting, analyzer, release-build, smoke, and manifest checks pass. The
six-job screen verifies exactly. Results (`mem:solver/experiments/34-sparse-phase-profile-results`)
put 55.2-84.0% of sparse time in preparation and only 10.0-31.5% in forward
elimination. Split preparation before attempting an incremental basis. Scheduling
remains paused.

Each optimization commit includes its related docs. Experiment 13 tooling/results
are committed in `473aba7`. Calculation promotion (`mem:solver/experiments/14-calculation-promotion`)
has separate commits: exact-L `5ae5631`, witness `1fbe95d`, and direct RREF bounds
`af1682c`. All three are permanent. No push was performed.
Exact prefix tooling is committed in `809f7c9`.
Experiment 17 includes benchmark-only p12/p123 fixed-work access. The validated
N<=9 guard (`mem:solver/experiments/18-guarded-p1-promotion`) was rejected before commit.
Unconditional p1 (`mem:solver/experiments/19-unconditional-p1-promotion`) is the shared
production Custom policy for both modes. The constructor candidate is absent from
the working production source.

## Latest measured findings

Experiment 16 results (`mem:solver/experiments/16-post-calculation-results`): 38/38 verified,
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

Experiment 13 (`mem:solver/experiments/13-calculation-results`) remains the isolated evidence
for the three calculations: exact-L 4.44-4.45x, witness replay 14.87x, direct bounds
7.3-8.3% shorter. Its whole results (`mem:solver/experiments/13-whole-results`) improve 24/65
optimal/all medians 5.6-12.3% and complete the fixed 36 N=9/L=12 group in 62.409 s.
Neither experiment establishes complete hard enumeration or a universal scheduler.

## Current decision

Experiment 17 (`mem:solver/experiments/17-results`) verifies 180/180 records in 38.135 process
minutes. Unconditional p1 fails the hard-10 memory gate by 2.84-6.32x. P1 improves
every completed N<=9 workload at 16/32 workers, including hard-36 optimal by 64-73%.
The user explicitly accepted the memory cost and deferred that issue. Production
therefore uses p1 at every N for both modes. Sharing, donation and parallel remaining
groups remain off.

Experiment 20 (`mem:solver/experiments/20-results`) verifies all 26 jobs and exact result
identities. The post-winning constructor guard is permanent. The timing screen is
mixed: 24 all improves 2.51%, 65 and 115 are effectively neutral, and the noisy
two-sample 238 comparison regresses 9.36% despite identical structural work. The
guard's basis is proof-state redundancy plus the earlier measured 26.366-second
hard-36 opportunity, not a claimed universal speedup.

The 238/115 process samples attribute their low-CPU intervals to uneven root-search
tails, not the optional constructor. Sharing, donation and parallel remaining groups
stay paused. Experiment 21 (`mem:solver/experiments/21-results`) verifies the isolated 238
root: best/all do identical structural work and 80,621,568 witness leaves consume
about 34 seconds. Experiment 22 (`mem:solver/experiments/22-analytic-witness-ports`) replaces
only the 559,872-fold symmetric-port factor with an exact analytic minimum. The
six-job replay preserves every exact identity, reduces the root median 58.03-61.15%
and makes the optimization permanent. Experiment 23 (`mem:solver/experiments/23-results`)
verifies the whole translation: 238 improves 43.10% all and 51.74% optimal, 115 all
improves 9.98%, while 115 and hard-36 optimal are effectively unchanged. Exact
search coverage and results agree. Remaining diagnostics put 30-52% of accounted
worker time in legal-decision identity and 28-34% in state keys. Split open-port
and marked-child costs before attempting reuse; scheduling remains paused.
Experiment 24 (`mem:solver/experiments/24-canonical-purpose-profile`) implements this
diagnostic-only split. Four verified jobs put 94.5-96.2% of legal-decision time in
open-port and marked-child keys. Marked keys remove fewer than 0.3% of candidate
decisions in these samples. The next candidate bypasses them only inside dispatched
DFS roots, retaining canonical root and adaptive-frontier identities.
Experiment 25 (`mem:solver/experiments/25-internal-dfs-marked-bypass`) verifies all 12
screening records. Its four completed pairs preserve full exact outputs and improve
wall time 16.6-21.1%. Hard-10 processes 20.4% more states within the cap but raises
sampled peak working set 26.2%. Experiment 26 (`mem:solver/experiments/26-internal-dfs-promotion-repeat`)
verifies all 16 repeat jobs. Combined three-sample medians improve 17.5-22.9% with
identical exact outputs, so the internal DFS bypass is permanent. Root and frontier
planning remain keyed. Current telemetry attributes hard-run RAM mainly to 32 concurrent
worker-local state and SCC caches; separate aggregate byte counters are the first step
if memory optimization resumes. Scheduler experiments remain paused. The next speed
hypothesis is to reuse state-canonical labeling for invariant open-port selection;
the promoted hard-10 diagnostic attributes 303.6 of 316.5 legal-decision seconds to
open-port keys. Treat this as a capped profile, not completion evidence.

Experiment 28 (`mem:solver/experiments/28-state-open-port-coordinate-reuse`) verifies all
14 records. Six completed pairs improve 18.2-49.4% and preserve exact results across
optimal, minimum-link, and full enumeration. The capped hard-10 diagnostic processes
45.1% more states while sampled peak working set rises 12.8%; it does not establish
completion speed. State-coordinate reuse is permanent. Scheduler work remains paused.

Experiment 29 (`mem:solver/experiments/29-deferred-state-canonicalization`) verifies all 26
records. Twenty-four complete optimally and preserve every exact result field; the two
hard-10 runs remain explicitly capped. Six completed medians improve 18.3-38.1%.
The fail-fast no-cache ablation (`mem:solver/experiments/30-no-state-cache-ablation`) now measures
the cache's net value directly. No cache is 12.3% slower on 115 minimum L and 3.00x
slower on 238 optimal. Keep the permanent deferred cache. Scheduling remains paused.

Experiment 31 (`mem:solver/experiments/31-propagation-bounds`) splits propagation telemetry and
removes a second evaluation of each registered port's positivity and capacity bounds.
The direct equivalent checks remain. All 38 jobs verify; six completed medians and
first-witness medians improve 5.36-9.40%. The capped hard pair advances 3.62% more
decisions. The change is permanent in `9f63f01`.

Experiment 32 (`mem:solver/experiments/32-weighted-sparse-quotient`) verifies 38/38 records but
fails its performance gate. The 115 medians improve 5.21-9.63%; 36 regresses 4.54%,
238 minimum L regresses 7.47%, and 238 optimal regresses 8.83%. Hard sparse time per
pass rises 6.79%. Keep experiment 31 and do not enable the quotient conditionally from
benchmark identity.

Experiment 33 verifies 26/26 records and exact structural counters. Its small timing
changes are mixed, and the hard prevalence counter is zero. The candidate and its
telemetry are restored. Profile sparse work by matrix size and reanalysis cause before
attempting a rollback-aware incremental basis.

Experiment 34 implements that diagnostic. Interpret its instrumented time breakdown,
not before/after wall speed, because the new clock reads and atomic counters add work.
All six records verify. The completed 115/238 pairs preserve exact results; hard 10 is
capped. Active matrices are usually <=15 rows and variables, while substitution removes
79.7-91.9% of stored rows. Retain the diagnostic and profile preparation internals next.

Experiment 35 (`mem:solver/experiments/35-sparse-preparation-profile`) implements that next
diagnostic split. It separates variable collection, substitution and normalization,
tautology filtering, sorting, and working-row conversion only when hotspot recording
is active. Ordinary solve behavior remains unchanged. Solver-core equivalence,
Reference, parallelism, Clippy, analyzer, formatting, release-build, and completed
smoke checks pass. All six records verify (`mem:solver/experiments/35-sparse-preparation-profile-results`):
substitution and normalization consume 89.99-91.42% of preparation and 49.12-76.30%
of total sparse time. Test exact integer evaluation for fully known rows next, then
isolate a no-known-variable clone path before changing mixed-row substitution.
Canonicalization remains the larger hard-run bucket; scheduling stays paused.

Experiment 36 (`mem:solver/experiments/36-fully-known-row-substitution`) implements the first
isolated production candidate. Fully known rows use one exact integer denominator LCM
and return their canonical tautology or contradiction directly; mixed rows retain the
existing rational path. Exhaustive substitution, solver-core, Reference, parallelism,
workspace, Clippy, analyzer, formatting, release-build, and completed smoke checks
pass. All 14 A/B records verify (`mem:solver/experiments/36-fully-known-row-substitution-results`).
Completed medians improve 9.73% on 115 all and 16.41% on 238 optimal with identical
structural work. Hard 10 advances 6.56% more states within the cap. The change is
permanent. Test the no-known-variable clone path next; mixed-row integer substitution
remains separate. Scheduling stays paused.

Experiment 37 (`mem:solver/experiments/37-no-known-row-substitution`) implements that isolated
clone path. A coverage scan returns immutable primitive rows directly when none of
their coefficients is known; fully-known integer evaluation and mixed rational
substitution remain unchanged. Solver-core, exhaustive Reference, parallelism,
workspace, Clippy, analyzer, formatting, release-build, and completed smoke checks
pass. All 14 records verify (`mem:solver/experiments/37-no-known-row-substitution-results`).
Completed medians improve 5.37% on 115 all and 3.42% on 238 optimal with identical
structural work. One capped hard sample processes 2.40% fewer states, so it does not
support a hard-run gain. The exact clone path is permanent. Test mixed-row integer
substitution separately; scheduling stays paused.

Experiment 38 (`mem:solver/experiments/38-mixed-row-integer-substitution`) implements the final
substitution case. Mixed rows use one denominator LCM and integer residual before the
established primitive normalization. Fully-known and no-known paths remain unchanged.
The exhaustive rational oracle and full validation pass. All 14 A/B records verify (`mem:solver/experiments/38-mixed-row-integer-substitution-results`).
Completed algebra medians improve 2.43-3.05% with identical exact work. The 238 wall
median improves 3.16%; 115 regresses 0.60% inside overlapping ranges while its CPU
median improves 1.15%. The integer path is permanent. Sparse substitution is closed;
return to canonicalization profiling. Scheduling stays paused.

Experiment 39 (`mem:solver/experiments/39-canonicalization-reprofile`) reuses the retained
purpose and subphase counters without changing solver source. Its four-job screen has
two completed controls and two 60-second hard caps. All four records verify (`mem:solver/experiments/39-canonicalization-reprofile-results`).
State keys account for 99.96-100.00% of graph-purpose time. Equality and inequality
encoding consume 58.9-67.2% of combined state/SCC canonicalization and inequality is
the largest canonical subphase in every case. Split equality construction/RREF and
inequality construction/normalization/sorting before changing calculations. Graph
labeling and legal-decision keys are no longer the first target. Scheduling stays
paused.

Experiment 40 (`mem:solver/experiments/40-equality-inequality-subphase-profile`) implements the
next diagnostic split. Equality records port indexing, row construction, and rational
RREF. Inequality records pivot indexing, row construction, primitive normalization,
and sort/deduplication. The timer-free production path is unchanged. All four records
verify (`mem:solver/experiments/40-equality-inequality-subphase-profile-results`). Equality RREF
owns 69.6-73.3% of equality time; inequality sort/dedup owns 52.1-54.3% of inequality
time. Keep primitive inequality rows as integers through sorting, deduplication, and
encoding as the next isolated candidate. Scheduling stays paused.

Experiment 41 (`mem:solver/experiments/41-primitive-integer-inequality-rows`) implements that
candidate. The generated exact oracle preserves the previous rational rows and encoded
bytes. Two seven-round isolated release measurements improve by 9.49% and 10.02% with
disjoint before/after ranges. All 16 whole records
verify (`mem:solver/experiments/41-primitive-integer-inequality-rows-results`), but 115 improves
slightly while 238 regresses slightly in overlapping ranges. The committed candidate
is not permanent. The completed-control repeat is now analyzed in experiment 42:
integer rows remain mixed, with 115 wall +1.47% and 238 -2.32%. Do not rerun capped
work. Fresh matched data is primary; cross-build pooling is only exploratory.

Experiment 42's original 36 factorial records and ten focused follow-ups all verify.
See `mem:solver/experiments/42-258-confirmation-results` for pooled timing,
binary/source identities, exact solutions/proofs and structural counters.
The three-sample 258 reference median is 198.178s (range 195.515-268.175s).
Integer/variables/combined medians are 252.431/242.696/251.305s: 27.38/22.46/26.81%
slower, with median CPU 7.84/9.51/10.03% higher. Keep every sample; the reference
variation prevents an isolated causal claim. Neither candidate is permanent.

Two diagnostics establish roots 23 and 7 as the search tail: only these remain for
the final ~108s. Their search spans are 255-266s, finish spans under 0.6s. Witness
canonicalization totals ~0.01s. Complete variables cut aggregate variable collection
27.384 to 4.846s (82.30%) with equal structural work, but this is not a whole gain.

Experiment 43 (`mem:solver/experiments/43-root258-affinity-results`) verifies all
28 selected-root completions, all four binaries and 1,156 archive hashes.
The frozen summary rerun is byte-identical; exact root plans/proofs, empty solution
sets and all 15 structural counters agree. Runtime 51.578 process minutes.
Complete variables improve each same-CPU/root wall comparison 0.29-1.70%, with CPU
also lower. Integer rows regress every wall comparison 0.60-2.30%, with CPU higher.
CPU 31 runs 14-20% slower than CPU 0. This proves placement sensitivity on isolated
roots, not the cause of all earlier whole-solve regressions. All roots are empty,
so the run does not measure successful witness discovery.

Experiments 44/45 finished at 12:41:03 Europe/Paris on 2026-09-02, about 3h29m.
All 198 jobs completed optimally, with zero caps/failures. Independent frozen
recheck passes all 905 hashes, topology/affinity, full exact results and proof
checks; summary SHA256 d92a22be3d10147bd98180b0b7f56d79cc647a77c2b1525b3ad11ff392af3837.
All 99 matched pairs preserve proofs and 15 structural counters.
Details: `mem:solver/experiments/44-topology-variable-confirmation-results`
and `mem:solver/experiments/45-cache-accounting-results`.

Recommend retaining complete variables and allocation-free cache accounting as
small optimizations, not universal speedups. Variables wins 53/75 completed wall
pairs and 12/15 cell medians, CPU 67/75 and all medians. Preserve adverse cells:
258/CCD32 +1.07% paired wall median, exploratory interval -2.21 to +2.78%;
115/unrestricted +0.32%; 36/CCD32 +0.18%. CPU savings alone would not justify
promotion; repeated completed wall gains and exact-work preservation support it.
Accounting wins 21/24 wall pairs and every CPU pair, with all six cell medians
favorable. 115 all improves in all ten pairs. 258 has only two pairs per CCD and
accounting lacks unrestricted/36/10 coverage; add selective regression controls
to the next useful screen, not another full matrix.

Topology policy remains benchmark-only. 258 reference medians are 193.189s on
CCD96/w16, 287.085s on CCD32/w16, 244.719s unrestricted/w32. Other controls favor
unrestricted CPU. Worker counts can change adaptive plans. Do not pool or infer
a universal production affinity rule. Unrestricted 258's final half averages
about 2.3 busy logical cores, so the long CPU trough remains.

Next isolated hypothesis: rational_rref's final sort/dedup may be replaced with
reversal because its retained exact unit-pivot rows are unique and already in
ascending pivot order. Preserve byte order via the dense oracle, including
contradictions and rank deficiency. No implementation or speed result yet.
Integer inequality rows remain restored. Scheduler extras remain paused.
No source changes or new tests in this analysis-only turn.

Source status: variables active in 520b352, accounting active in 331b87a.
Retention recommended, no new promotion commit. Restoration 5d8d924 and tooling
7309fb7 unchanged. No commit or push during analysis; result/handoff memories
uncommitted. Nothing running; `mem:solver/active`.

Experiment 33 validation: 181 default and 186 benchmark-feature solver-core library
tests pass, with two ignored in each configuration. The exhaustive Reference and four
parallelism integrations, full workspace, both strict Clippy configurations, and 33
analyzer tests pass. The frozen screen has 24 optimal runs, two capped diagnostics,
zero failures, and no kills.

Experiment 34 prelaunch: 180 default and 185 benchmark-feature solver-core library
tests pass, with two ignored in each configuration. The exhaustive Reference and four
parallelism integrations, full workspace, both strict Clippy configurations, 33
analyzer tests, release build, format, hotspot smoke, and six-job plan check pass.

## Validation records

Experiment 29 post-run: 26/26 records verify with no analyzer failures. All 24 completed
results match their frozen references; two hard-10 records reach the common cap and
remain incomplete. Full workspace tests, both strict Clippy configurations, analyzer
tests, formatting, and frozen-plan validation passed before launch.

Experiment 30 fail-fast: exact, deferred, and disabled cache modes pass solver-core and
Reference differentials. Full workspace tests, both strict Clippy configurations, and
33 analyzer tests pass. Two fresh completed A/B pairs preserve all exact outputs. The
candidate regresses 115 minimum L by 12.3% and 238 optimal by 3.00x, so the larger
screen was not launched and the source candidate was restored.

Experiment 31: 179 default and 184 benchmark-feature solver-core library
tests pass, with two ignored in each configuration. The exhaustive Reference and four
parallelism integration tests, full workspace tests, both strict Clippy configurations,
and 33 analyzer tests pass. The frozen analyzer verifies 38/38 benchmark records with
36 optimal completions, two capped diagnostics, no kills, and zero failures.

Experiment 32: 182 default and 187 benchmark-feature solver-core library tests pass,
with two ignored in each configuration. The exhaustive Reference and four parallelism
integrations, full workspace, both strict Clippy configurations, and 33 analyzer tests
pass. The frozen analyzer verifies 38/38 records: 36 optimal, two capped, no failures
or kills. Performance is mixed and the candidate source was restored.

Experiment 28 post-run and promotion: 14/14 records verify with no analyzer failures.
All six completed pairs match their frozen references. The hard-10 pair reaches the
same 60-second cap without a witness. Full workspace tests, 92 frontend tests,
frontend diagnostics, formatting, and strict all-target Clippy pass.

Experiment 26 post-run and promotion: 16/16 repeat records complete optimally and
match their references. Combined with experiment 25, all 12 completed A/B pairs
preserve every compared exact result field. Solver-core, exhaustive reference,
parallelism and shared application tests pass; strict default and benchmark-feature
Clippy pass. See promotion repeat (`mem:solver/experiments/26-internal-dfs-promotion-repeat`).

Experiment 25 post-run: 12/12 records verify; eight complete optimally and four
fixed-time diagnostics remain explicitly incomplete. All completed and capped
pairs preserve their respective exact or partial result identities. No kills or
failures occurred. See internal DFS bypass (`mem:solver/experiments/25-internal-dfs-marked-bypass`).

Experiment 24 post-run: 4/4 records verify; 115/238 complete with experiment 23's
exact full results and structural counters, while hard 36/10 remain explicitly
capped. No kills or failures occurred. See
purpose profile (`mem:solver/experiments/24-canonical-purpose-profile`).

Experiment 23 post-run: 27/27 records verify; 20 completed references preserve
full result identities and seven capped diagnostics remain explicitly incomplete.
No kills or failures occurred. The original and rechecked summaries have identical
SHA-256. See results (`mem:solver/experiments/23-results`).

Experiment 22 post-run: 6/6 selected-root workloads verify and match experiment 21
requests, plan identities, statuses and full solutions. No kills, failures or open
activity spans occurred. The rechecked summary is byte-identical. See
results (`mem:solver/experiments/22-results`).

Experiment 22 prelaunch: 183 solver-core tests pass with two manual benchmarks
ignored, plus one exhaustive outer differential, four parallelism integrations and
26 example tests. Strict all-target Clippy passes with `bench-internals`. The first
protocol-tag ordering attempt failed the outer oracle and was corrected before any
benchmark.

Experiment 21 post-run: 6/6 frozen root workloads verify with identical requests,
plan identity and exact witness. Reverification preserves the summary SHA-256. No
kills, failures or open activity spans occurred. See results (`mem:solver/experiments/21-results`).

Experiment 18 guarded trial: 212 solver-core library/integration/example tests passed,
two ignored; six synthetizer-app unit and ten shared-API tests passed. Strict
all-target solver-core Clippy passed with and without `bench-internals`. Experiment
19 passes 211 solver-core tests, 16 application/shared-API tests, 32 analyzer tests
and both strict Clippy configurations. Details and logs are in its promotion note.

Experiment 17 post-run: 20/20 fixed and 160/160 whole records pass frozen
verification and exact-result checks; no kills or failed processes. Rechecked
summaries equal originals. Both 78-file variant snapshots and whole binary mappings
pass hashes. See results (`mem:solver/experiments/17-results`).

Experiment 17 prelaunch: reference and candidate each passed 211 solver-core
library/integration/example tests, two ignored. Strict all-target Clippy passed with
`bench-internals`; candidate default-feature Clippy passed. 32 Python tooling tests,
release builds, formatting and the frozen 180-job plan check passed. See 17 (`mem:solver/experiments/17-p1-promotion-and-followups`).

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
check passed. See 13 changes (`mem:solver/experiments/13-calculation-changes`) for logs.
Experiment 13 post-run verification passed all 80 records and provenance checks;
this analysis changes documentation only, not the previously tested Rust code.

Profiling prelaunch: 208 solver-core library/integration/example tests passed with
`bench-internals`, two ignored; 26 Python tests passed, including actual tiny
completion and one-second hard cancellation. Strict all-target solver-core Clippy
passed with and without the feature; release build passed. Logs are in 12 (`mem:solver/experiments/12-hard-obligation-profiling`).

Experiment 09 prelaunch: constructor-only source passed 199 tests; both encoder
variants passed 200 each, two ignored per run. Strict all-target Clippy, 22 tooling
tests, builds and formatting passed. Earlier benchmark commit `92b111c` passed
194 package/example tests, two ignored, and strict Clippy. The 297-test workspace
validation is a separate historical record. See 08 (`mem:solver/experiments/08-repeat-scheduling`)
and `mem:solver/experiments/09-serializer-and-find-all` for those logs and source splits.

## Accepted promotions, 2026-09-02

User approved permanent retention of complete variables (520b352) and cache byte
accounting (331b87a). The earlier recommendation is now accepted; no source
change was required. Preserve the measured uncertainty and all adverse cells.
Promotion documentation precedes the next isolated RREF candidate. New benchmark
limit is one hour, including cleanup; no run launched yet. No push authorized.

## Experiment 46 ready to launch

`mem:solver/experiments/46-rref-order`: candidate a79ecf7 replaces only final
canonical RREF sort/dedup with exact reverse pivot order. Dense oracle including
zero-variable, scaled/permuted, dependent and contradictory inputs passes.
221 solver-core tests, 330 workspace all-target tests, both strict Clippy builds,
41 tooling tests, release and 12 frozen real-executable smokes pass.
Source/binary provenance verified, only canonical.rs differs from accounting.

40-job plan is ready with exact order balance, 2520s search caps and 600s cleanup.
User one-hour limit leaves eight minutes for startup and verification. Compare
accounting/rref separately from before/variables and variables/accounting.
New long RREF/258 and accounting/unrestricted258 work is deferred. No timing result.
No run launched yet. Variables/accounting permanent, promotion note 6812173.
Candidate not promoted. No scheduler change or push. `mem:solver/active`.

## Experiment 46 launched

Launched 2026-09-02 17:02:38 Europe/Paris, runner PID 20048; awaiting analysis.
Startup verified job 1/40, runner alive, empty stderr. Output:
target/parallelism-ladder/rref-order-hour-20260902. No result inferred.
40 jobs; search+cleanup maximum 52 minutes, user limit one hour.
Promotion 6812173, candidate a79ecf7, benchmark/handoff f6f422c. No push.
Live launch notes uncommitted. End turn; no builds/tests during timing.

## Experiment 46 analyzed, 2026-09-02

`mem:solver/experiments/46-rref-order-results` is authoritative. Finished 17:39:06
Europe/Paris after about 36m29s. 40 attempted, 39 optimal, one clean 320s variables
cap on 258/CCD32. Whole screen FAILED its required completion coverage. Independent
frozen recheck reproduces the same sole failure and summary hash
7e36e0f85414ef9a437c719c83d6efb2519211595f973ffa103ee92917269715.
977 frozen hashes and all 39 exact outputs pass; 19 complete pairs preserve proofs
and 15 structural counters. No crash, watchdog kill or false optimal outcome.

RREF candidate wall medians improve 0.44/1.68/0.83% on 115/238/36. All ten CPU
pairs improve; only six wall pairs do, and every exploratory wall interval crosses
zero. Recommend a focused confirmation, not promotion yet. No RREF/258 coverage.
Variable/258 has one completed -0.64% pair and one cap against 302.350s reference;
no aggregate completion speedup. Other variable controls favor the change.
Accounting/unrestricted115 wall +0.54% and 36 -1.06%, two pairs each. Retain these
limitations without pooling 16/32-worker timings or rolling back prior promotions
solely from this small screen. No causal background-load claim is established.

Nothing running. Next proposed run stays <=1h including cleanup, with fewer jobs
and more per-job time for the capped 258 control. No new calculation candidate
before confirming RREF. No source edits, new commits or push in this analysis.
Result/handoff notes uncommitted; `mem:solver/active`.

## Focused experiment 46 confirmation prepared

User approved follow-up; `mem:solver/experiments/46-rref-order-confirmation`.
20 jobs reuse the unchanged four frozen executables. RREF 115/238 repeat, new
RREF/258 CCD96 comparison, and variables/258 CCD32 with cap increased to 400s.
3000s search caps + 300s cleanup = 55 minutes, leaving five minutes within one
hour for startup/verification. Frozen plan, provenance and exact AB/BA balance
pass. No solver change, build, new performance result or promotion. Not launched
yet. Original failed run preserved; no push. `mem:solver/active`.

## Focused confirmation launched

Launched 2026-09-02 17:48:36 Europe/Paris, runner PID 55420; awaiting analysis.
Run target/parallelism-ladder/rref-order-confirmation-20260902. First job running,
empty stderr, runner alive. Completion/failure dialog enabled. No result inferred.
55-minute search+cleanup allowance, one-hour user limit. Commit eac9608 includes
manifest, prior result notes and handoff. No push. Live launch notes uncommitted.

## Focused RREF confirmation analyzed, 2026-09-02

`mem:solver/experiments/46-rref-order-confirmation-results`: all 20 runs optimal,
finished 18:26:23.904 Europe/Paris after about 37m48s. All 976 frozen hashes,
exact solutions, paired proofs and 15 structural counters pass. Independent frozen
summary hash matches original:
4d9838bb82b3c8c34952da2a8be56fb47c7a31c598a896a75d1405704ffdc7cf.

RREF paired wall medians -1.49% on 115 all, -1.44% on 238 optimal, +0.38% on
258 minimum_links. All eight new CPU pairs improve, 18/18 across both screens.
These are descriptive counts, not pooled speedups. Repeated completed 115/238
benefits support recommending permanent retention of a79ecf7. 258 is mixed:
+1.60% and -0.84% individual pairs, no demonstrated completion gain.
Variable/258 finishes both new pairs, -1.09% median, with candidates 293.548 and
304.898s. Old timeout did not recur, but its cause is not established. Preserve it.

No promotion commit, source edit, push or new benchmark during analysis. Next
proposed work: profile pivot normalization/elimination/nonzero-suffix preparation
inside canonical RREF, with factor/coefficient patterns to choose exact shortcuts.
Existing +1 and zero-factor handling must not be rediscovered as new work.
Stop broad ordering repeats; future benchmark budget remains <=1h including
cleanup. Scheduler paused. Nothing running; `mem:solver/active`.

## RREF promotion accepted, 2026-09-02

User approved permanent retention of reverse RREF pivot ordering in a79ecf7.
Confirmation evidence and limitations remain in
`mem:solver/experiments/46-rref-order-confirmation-results`. Promotion notes are
committed separately before the new diagnostic split. No production code rewrite
needed for promotion. No push. Next work is RREF arithmetic profiling, not a new
arithmetic optimization; benchmark budget remains <=1h including cleanup.

## Experiment47: RREF arithmetic profile results, 2026-09-02

`mem:solver/experiments/47-rref-arithmetic-profile-results`.
All 14 records verify in 18m54: 10 optimal controls, 4 intended hard caps. 192 frozen
hashes and identical rechecked summary. Completed full solutions, keys, preferred
witnesses, outcomes/proofs and 15 structural counters match; repeated profile counts
agree exactly. Hard36 has 8 matching partial solutions, hard10 no witness.
Elimination owns 66–69% of measured RREF time. Zero destinations 78–80% and
factor -1 updates 26–30% motivate two separate small tests, in that order. Checked the
installed rational operators; no explicit shortcuts for either pattern.
No production optimization or speedup established by this diagnostic. Keep exact
arbitrary-size arithmetic; source-value decisions only. Scheduling stays paused.
Nothing running. Source/docs 1006e6f, manifest 14d3f73, RREF promotion docs c106d4c.
Results/handoff notes uncommitted; no new commit or push.

## Experiment 48 analyzed, 2026-09-02

`mem:solver/experiments/48-rref-elimination-shortcuts-results`.
Finished 20:20:55.038 Europe/Paris in about 33m45, all 32 jobs optimal.
All 263 frozen hashes, exact complete outputs/outcomes/proofs and 15 paired
structural counters pass. Schedule, adjacent AB/BA pairs, hotspot-OFF setting and
before-resume affinity verified. Rechecked summary equals original:
3387537fdc14efd9f36359b4861b566be2e3f4ac399e421e3f03d07b0841a458.

Minus completion paired medians on 115 all / 238 optimal / 258 minimum_links /
36 optimal: -1.16% / -0.65% / effectively 0% / -1.14%.
Seven of eight wall pairs and all eight CPU pairs improve.
Zero: +0.52% / -2.64% / +4.71% / -2.28%, including an unexplained +10.62% 258
pair that must not be discarded. Only two pairs per cell; no promotion yet.
115 first witness remains mixed for minus, median +0.81%; zero -3.37%.

Recommend minus-only confirmation with four additional pairs in each workload,
same frozen binaries/settings, 32 jobs, 54m40 search+cleanup plus overhead margin
under one hour. New screen first analyzed separately. Hold zero, no combination.
Scheduling paused. No follow-up approved or launched.

Candidate preparation/tests/patches/manifest fadb316; prior profile results
da2c89f. Current unrelated history HEAD 6683546 is preserved. Main canonical.rs/
solver.rs production prefixes still equal the frozen before source; neither
candidate promoted. Result/handoff memories uncommitted, no new commit or push.
Nothing running; `mem:solver/active`.

## Experiment48 confirmation approved

User approved minus-only confirmation. New manifest
`benchmarks/custom/rref-negative-unit-confirmation.json`, repeats 3 through 6,
32 jobs. Same frozen binaries, all three modes and prior placements/caps.
41 tool tests pass; all 145 original variant hashes and 192 plan hashes rechecked.
Plan confirms 16 adjacent pairs, balanced order and 54m40 search+cleanup allowance.
No arithmetic change, no new build or promotion. Formatting/commit then launch;
not running yet. `mem:solver/experiments/48-rref-negative-unit-confirmation`.

Confirmation launch: 2026-09-02 21:29:14 Europe/Paris, runner PID 4088.
Verified alive, initial status 1/32 `minus_r36_allcpu-optimal-before-p1-w32-r6`,
empty stderr. New output `target/parallelism-ladder/rref-negative-unit-confirmation-20260902`.
Manifest, initial results and handoff committed 3b32c51 after formatting. No push.
Post-launch notes uncommitted. Results pending; no completion or speedup inferred.
End turn, no builds/tests during timing. `mem:solver/active`.

## Negative-unit confirmation analyzed, 2026-09-02

`mem:solver/experiments/48-rref-negative-unit-confirmation-results`.
Completed 22:05:15.996 Europe/Paris after about 36m02. All 32 jobs optimal;
191 frozen hashes and identical original/rechecked summary:
2d724ba0ec39a0c3d5d9d0fa946edf269345b684b5614af15b47f6184084d5e5.
Exact solutions/outcomes/proofs, 15 structural counters, pair order and verified
before-resume placement match. Secondary audit verifies both screens' exact
outputs, binary identity and structural work for all 48 minus comparison records.

New four-pair wall medians on 115 all / 238 optimal / 258 minimum_links / 36 optimal:
-1.59% / -2.05% / -1.96% / +0.39%. New first witness on 115 -4.44%, all four improve.
Six-pair secondary completion medians: -1.16% / -1.42% / -1.35% / -0.05%.
19/24 wall pairs and 23/24 CPU pairs improve. Do not pool across placements or claim
hard36 completion improvement; retain its +4.70% pair. 238 also has one +1.13% wall
pair. Confidence intervals remain exploratory with small samples.

Recommend permanent negative-unit arithmetic only, with fresh integration tests,
strict Clippy and this evidence in its isolated commit. Not applied in this analysis.
Hold zero and keep +10.62% initial 258 evidence; no implicit combination or guard.
Scheduler extras paused; future benches <=1h including cleanup.
Current HEAD edb4f0b and unrelated commits preserved. Main arithmetic remains the
frozen before implementation. Prior commit 3b32c51 includes initial results/manifest;
new result/handoff notes uncommitted, no source edit, new commit, push or run.
Nothing in flight; `mem:solver/active`.

## Permanent negative-unit integration, 2026-09-02

User approved permanent commit. Integrated seven lines in canonical.rs ordinary
and diagnostic RREF paths. Exact factor -1 uses addition; no flags, N/cyclicity/
affinity/memory policy. Full canonical.rs matches tested minus source after newline
normalization. Existing matrix and heartbeat tests reused, solver.rs unchanged.

Fresh release solver-core all-targets: 223 tests with bench-internals, 213 default;
two ignored each. Both strict release Clippy configurations pass. No new benchmark.
Source, confirmation results and promotion documentation prepared for an isolated
perf(custom) commit; no push. Related record: `mem:solver/experiments/48-negative-unit-promotion`.
Zero held; scheduling paused. Preserve the hard36 limitation and all adverse pairs.

Promotion committed as `6c10b35eee8b25fa7fabd55b29d042f8e67da74b`,
`perf(custom): specialize negative-unit RREF elimination`.
Includes source, confirmation evidence and related Serena records. Formatting and
staged diff checks passed. No push. Documentation-only follow-up records this
resulting commit hash; no additional source changes or benchmark launches.

## Wall-time recommendations, 2026-09-02

Source audit at f89b6d9: `mem:solver/experiments/49-wall-time-priorities`.
Recommend an isolated removal of derived inequality rows from internal state/SCC
key encoding. Capacity, variable count and exact equality basis already determine
them. Mathematical key-equivalence argument, no implementation or speed measurement.
Keep actual feasibility checks, existing memoization and all production defaults.
Later proposals: current reprofile, canonical RREF exact arithmetic, conditional
pruning order, then separately scoped tail scheduling. Sharing/donation currently
disable deferred state keys and must be accounted for explicitly.
No source edits, tests, benchmark, commit or push; proposal/handoff notes only.

## Experiment49 implementation ready, 2026-09-02

User authorized all recommendations. First candidate removes deterministic inequality
rows from internal state/SCC semantic keys, version 2. Public topology/witness format
and actual feasibility checks remain unchanged. Main source contains the uncommitted
candidate. `mem:solver/experiments/49-wall-time-priorities`.
224/214 release tests pass, two ignored each; final test-helper oracle and both strict
Clippy configurations pass. 41 tooling tests and 12 actual CLI parity checks pass.
347 frozen/prepared hashes and 24 balanced-as-possible pairs verify. No speed result.
48-job screen ready, 53m40 search+cleanup, one-hour overall limit. Remaining authorized
reprofile/arithmetic/pruning/tail work follows results; no repeat approval needed.
No commit or push. See `mem:solver/active` for launch state.

Experiment49 launched, runner PID 33320. Run target/parallelism-ladder/derived-key-screen-20260902; completion/failure signal enabled. Awaiting analysis, no speedup inferred. No builds/tests while running. See active and experiment49 for remaining authorized work.

2026-09-03: Experiment49 complete and retained: all17 substantive timing pairs improved, median wall -10.2% to -21.3%, full public result/proof identity preserved,48/48 records verified. See `mem:solver/experiments/49-derived-key-results`. Active source uncommitted, no push. New profile motivates isolated checked-small-rational RREF experiment50; preparation only, no run yet. Later pruning/scheduler stages remain authorized.

Experiment50 implemented and validating, see `mem:solver/experiments/50-checked-rref`: checked64 canonical RREF with untouched-original BigInt fallback and explicit diagnostic coverage counters. Separate candidate worktree sf50-checked and target sf50-checked-build; baseline frozen49after. No run launched or speedup claim. All main changes uncommitted.

Experiment50 ready for56-job paired screen2026-09-03:227/217 final release tests and both strict Clippy pass,16 CLI parity checks including real BigInt fallback,353 hashes verified, no source mismatch. Main49+50 active/uncommitted, no push, no50 performance claim. `mem:solver/experiments/50-checked-rref`; check `mem:solver/active` for live launch state.

Launched2026-09-03 Europe/Paris, runnerPID34104, target/parallelism-ladder/checked-rref-screen-20260903. Startup active; completion/failure dialog and durable markers available. Awaiting analysis, no performance/completion inferred. End turn now per AGENTS.md; no builds/tests while timing. All changes uncommitted, no push.

2026-09-03 final checkpoint: derived inequality-key removal permanently committed4d711b1 after user request, exact frozen49after source. Experiment50 completed56/56 records, full public outputs/proofs and15 structural counters match. Checked64 candidate not promoted: wall -2.48%115, -0.22%238, -0.21%258, +2.80%36;36 CPU+8.49% with all5 pairs worse. Restored production to49; preserved patch/manifest/fixture. `mem:solver/experiments/50-checked-rref-results`. No new benchmark, no push. Supporting research/handoff commit pending. Remaining approved pruning/scheduling work is not complete.

Experiment51 skip+borrowed dead-verdict is permanent in `7799c110e8266b96cb59dd0480b62e59c576cda7`, unpushed. Early not applied. Experiment52 tail-only sibling help rejected after a 200-job screen: 115 all-mode lost layouts (49→43/44) and identity-passing 238/36 slowed. Source restored before the 51 commit. Experiment53 hotspot-on profile verified 12/12 at `target/parallelism-ladder/post51-cost-profile-20260904`. Propagation 54–67% of accounted; 258 Bareiss forward 21% of accounted on leftover 16–31-row systems; overlapping long roots, not one unique tail. `mem:solver/experiments/53-post51-cost-profile`.

Experiment54 isolated Bareiss-forward bookkeeping launched 2026-09-04 ~15:44 Europe/Paris. Runner PID 24648. Finished 16:34:39+02:00. 66/66 verified, `failures=[]`. Output `target/parallelism-ladder/bareiss-forward-screen-20260904`. Before SHA256 `d3b1f0f2b4f2bab2ed03b58ebc5634f9c710cd57a591e4d9043a2daa1d465952`. After SHA256 `ad645b376c898607281701fe453c76b304ba05acfa55c50d599ecaee7c27fc5d`. Summary SHA256 `5949585c770c23a7389a942c71d52ef87b6348a10c764cbd5dfb88035332cd9b`. Permanent this commit, unpushed. 115 all wall −10.85% CCD96 / −10.58% CCD32; 258 min-links −8.49% wall / −9.54% CPU; 238 −2.89%/−2.39% wall (CCD32 wall bootstrap includes 0); 36 −0.83% wall with bootstrap including 0. 31/31 completed pairs keep exact keys/proofs/structural counters. Experiment55 sharing is next. `mem:solver/experiments/54-bareiss-forward`.
