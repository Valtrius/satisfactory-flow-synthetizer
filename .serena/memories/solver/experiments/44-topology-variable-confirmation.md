# 44. Topology-controlled variable-discovery confirmation

Date: 2026-09-02. State: completed and analyzed.
Results and recommendation: `mem:solver/experiments/44-topology-variable-confirmation-results`.
The launch/preparation record below describes what was known before results.
User approved the benchmark policy and final complete-variable check, plus extra
independent experiments while AFK for approximately 8h.
Prior results: `mem:solver/experiments/43-root258-affinity-results`.

## Benchmark policy implemented

benchmark-affinity.ps1 creates hidden Windows children with CREATE_SUSPENDED,
sets and reads their process affinity, retains a process handle, then resumes.
Any setup error terminates the suspended child. Argument quoting and redirected
output work for spaced paths. Fixed-work and whole-solve runners share this helper.
No production affinity or scheduler change.

Topology records include cores/SMT relationships, efficiency class, L3 groups,
available mask and active power scheme. This host validates CPUs 0-15 mask `ffff`
with 96MiB L3, and CPUs 16-31 mask `ffff0000` with 32MiB L3.
Whole one-CCD jobs use 16 workers; unrestricted checks use 32. Fixed-root planning
can still retain its original 32-worker certificate while executing serially.

Whole manifests support ProcessorAffinity, CacheBytes, PairId, PairRole,
Comparison and MaxScheduledSeconds. Plans reject oversubscribed controlled jobs,
cache-domain mismatch, inconsistent pairs and exceeded search+cleanup budgets.
Pair blocks are randomized; reference/candidate order alternates by repeat,
giving balanced adjacent AB/BA comparisons rather than separated batches.

Both verifiers check before-resume evidence. Whole analysis keeps CPU masks
separate, allows a verified PairRole reference to anchor exact output checks, and
reports paired candidate/reference ratios, median changes and percentile-bootstrap
95% intervals. These intervals are exploratory, particularly with two/five pairs.
Incomplete pairs never produce completed speedups. Legacy unpaired jobs remain
supported; no historical timing is silently pooled into the new cohorts.

The whole launcher gains PlanOnly and hashes every frozen input/source/binary/tool.
The analyzer rechecks those hashes. Process CPU, first validated witness, wall time,
existing per-second process samples and exact results remain recorded.
Boost, thermal drift and external load remain possible noise sources.

## Solver state and variants

Integer inequality rows restored; related record:
`mem:solver/experiments/41-integer-rows-restoration`.
Complete variables remain a candidate. An independent allocation-free accounting
candidate is active but unpromoted: `mem:solver/experiments/45-cache-accounting`.

Frozen variants at target/parallelism-ladder/topology-final-variants-20260902:
before and variables reuse the original immutable factorial executables;
accounting is a new matched-environment build with only the additional size helper.

- before: 8b2dd2768976469ba74617cf5e0d31e72324cf2b41e831f8473777dc837d2ea2
- variables: b19b601f29e8f9bed6b3427a89152c376466ab68408e0b201d87ad384f594acf
- accounting: 253b8245c9106211ee1a0a44f006bffc246138b0cb59ef2e895bfb4f1b6b1573

## Screen

Manifest: benchmarks/custom/topology-variable-final-screening.json.
Map: target/parallelism-ladder/topology-final-variants-20260902/variants.json.
198 sequential jobs, 99 adjacent pairs; all hotspot-off, capacity 1200, p1.
No hard 10 capped-throughput repetitions.

Primary complete-variable confirmation: 150 jobs, five pairs per workload/placement.
Each of the two CCDs at 16 workers and unrestricted 32 workers gets:
115 all, 238 optimal, 238 minimum_links, 258 minimum_links, 36 optimal.
Caps respectively 90/30/45/360/60s. Maximum N10 for 115/238, 12 for 258, 9 for 36.
This includes successful witness paths and full minimum-link completion, unlike
the previous empty-root experiment. Compare new same-worker plans only.

Extra accounting screen: 48 jobs as detailed in experiment 45. Compare variables
versus accounting, with separate aliases; never mix with the primary before/variables.
All jobs are reference cohorts and must complete optimally to pass verification.

Search caps total 380.5 minutes. Explicit 20s cleanup grace adds 66 minutes.
Total scheduled allowance 446.5 minutes, 7h26m30s plus launch/verification overhead.
Manifest guard 27000s includes search and cleanup, not Python analysis overhead.
Expected completion is likely much earlier; no result is inferred from that.

## Validation and failures

198-job frozen plan passes. Twenty real-executable tiny smokes verify exact
optimal/minimum_links results across requested placements and all three binaries.
Native immediate-entry affinity test and cleanup-watchdog continuation test pass.
Policy tests reject changed masks, pre-resume evidence, worker/cache mismatch,
unmatched/separated pairs and altered frozen source. All 41 runner/analyzer tests pass, including native child entry, frozen-source tamper
rejection and paired policy checks.
Rust/accounting validation is in experiment 45.

Prelaunch corrections: initial manifest used guessed medium115/hard36 filenames;
existence assertions caught these before launch, corrected to existing corpus paths.
A test-only assertion was briefly attached to the wrong frozen-hash test and fixed.
No real benchmark was launched by these failures.

Evidence: target/exp44-tool-tests-final.log; target/exp44-smoke-analysis.log;
target/parallelism-ladder/topology-final-plan-20260902 and topology-final-smoke-20260902.
Final hash-aware smoke passes 20/20 using the frozen runner, all three binaries
and all selected placements. Evidence: target/exp44-smoke2-analysis.log and
target/parallelism-ladder/topology-final-smoke2-20260902. Formatting and diff checks
pass. Restoration `5d8d924`; accounting candidate `331b87a`; tooling committed in `7309fb7`. Active output:
target/parallelism-ladder/topology-final-20260902.

## After completion

Verify all scheduled results, binary/source/tool hashes, exact outcomes/solutions,
proof status, first-witness fields and structural work. Analyze each placement
separately, with paired ratios and ranges; do not normalize by a CPU speed factor.
Primary comparison decides complete-variable promotion. Accounting results decide
whether the independent candidate merits confirmation. Preserve any capped/failing
records; do not claim speed from partial work. Scheduler extras stay paused.

No performance result yet. Restoration committed in `5d8d924`; accounting candidate committed in `331b87a`.
Benchmark tooling/manifest committed in `7309fb7`; no push. Launch with completion/failure
dialog, save active handoff, then end turn. No builds/tests during the real run.

## Actual launch

Started 2026-09-02 09:11:57 Europe/Paris, runner PID 42192. Frozen launcher validates all 198 jobs.
Initial startup check: first job running, runner alive, empty stderr.
Root status: target/parallelism-ladder/topology-final-20260902/BENCHMARK-STATUS.txt.
Current job: results/BENCHMARK-STATUS.txt under that run.
Completion/failure dialog enabled. No performance/completion inference yet.
Restoration 5d8d924, accounting candidate 331b87a, tooling 7309fb7 committed; no push.
These live launch-state memory updates are uncommitted. End turn; no builds/tests.
