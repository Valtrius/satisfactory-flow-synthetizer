# Astra

Astra is a third engine (`solver-astra`, public tag `astra`). Custom remains the
default. Astra runs offline using cvc5 and uses the shared problem, execution,
validation, presentation, and history contracts.

## Setup

Install cvc5 and put its executable on PATH, or set `ASTRA_CVC5` to its absolute
path before starting the app. An explicit override is authoritative. Astra also
recognizes a cvc5 binary beside the running executable and the Windows installer
location `%LOCALAPPDATA%/Programs/cvc5/bin/cvc5.exe`. No download occurs at solve
time. The tested development installation is cvc5 1.3.4. The Tauri installer
currently requires this separate cvc5 installation; it does not bundle cvc5.

Missing cvc5, an unexpected exit, malformed responses, and `unknown` produce a
worker-failure incomplete result. They never switch to a different engine.

## Exact encoding

For fixed N, enumerate every physically possible node profile and exact operator
link count L, using the existing checked port accounting and arithmetic theorems
as subroutines. Astra does not call Custom or Z3's search. Normalize all external
rates and capacity with the same positive scale and retain terminal mappings.

Each external input and each operator is a source. One real variable per
operator describes the flow of each of its output belts. A merger has one output;
a splitter's outputs all have that variable's value. Each pair of source and
destination owners has a unary, prefix-ordered vector of Boolean belt variables.
The vector length is the minimum of their port arities. Anonymous discards use
one aggregate destination with a separate belt variable per available source
port. They are expanded to distinct discard terminals in the physical witness.

Constraints enforce:

- Source and destination belt counts equal the effective port arities; each
  external terminal has one belt. No owner connects directly to itself.
- For a destination with exactly one incoming belt, each possible edge implies
  equality between its source flow and the destination's required flow. The
  exactly-one belt constraint makes this equivalent to the conditional sum.
  This covers requested outputs, splitters, and a single discard belt. Merger
  inputs and multiple discards retain conditional sums. A splitter's required
  incoming flow is arity times its outgoing rate; a merger's is its outgoing
  rate. Requested outputs and total discard equal exact demands.
- Every flow is positive. An operator's outgoing flow times its output arity is
  at most capacity. This bounds a splitter's single input belt and every output;
  for a merger it bounds its output, while upstream constraints bound its inputs.
- L counts only operator-to-operator belts, including parallel physical belts.
- Every operator has an active predecessor on a decreasing input-reachability
  rank, or an external input; likewise it has a successor on a decreasing sink
  rank, or an output/discard. These ranks certify reachability without forbidding
  cycles: only a selected path must decrease, not every active edge.

All arithmetic remains linear and exact (`QF_LRA`). Conditional terms implement
bounded belt multiplicity without multiplication of unrestricted variables.

### Coverage and symmetry argument

Every legal graph maps to belt multiplicities, its unique physical rates, and
shortest-path reachability ranks. Symmetric splitter output ports and merger
input ports are interchangeable, so prefix ordering drops only their labels.
Operators of the same type may be relabeled in nondecreasing output-flow order;
this preserves at least one representative of every isomorphism class. No rule
depends on case filenames, corpus classifications, or a fixed capacity.

Conversely, each Boolean assignment expands deterministically into a complete
port graph. The independent validator solves the topology, checks global and SCC
uniqueness, and validates the exact restored witness again. Nonunique steady
states or invalid local SCC equations reject that topology. Unexpected structural,
capacity, extraction, or global equation failures stop the worker as errors.

After each SAT assignment, block exactly its Boolean routing assignment. Do not
block auxiliary rate or reachability values: they are not topology identities.
Fixed N has finitely many profiles, belts, and anonymous discards, so exhaustion
of every required model space is a finite proof. Only two finite global
contradictions are recognized: deficient supply and external rates above capacity.

## Enumeration, preferred witness, and interruption

Search N from the justified lower bound. At each N search exact L groups in
increasing order. `optimal` returns the first independently validated witness in
the first SAT group, after all smaller N/L obligations have finished. Same-group
roots are stopped and joined without marking them exhausted. `minimum_links`
exhausts that group and delivers its full set; `all` continues through every
feasible L group at that minimum N. This follows the user's updated requirement
that solution order and the choice between optimal ties do not matter.

Each worker owns one cvc5 session and uses incremental model blocking. With
multiple workers, each feasible profile is partitioned by the source feeding the
first normalized requested output. That output has exactly one incoming belt,
so these roots are disjoint and cover every Boolean model. All source owners are
included; cvc5 explicitly proves impossible choices UNSAT. One worker uses an
unsplit profile. Recognized profile impossibility theorems discharge one unsplit
obligation without starting a backend.

Each root is marked complete only after exhaustion or a recognized theorem,
with process cleanup finished. The coordinator rejects duplicate completion
messages and counts a profile complete only after every one of its roots has
finished successfully. A cancelled root leaves the parent incomplete; completed
sibling roots retain their proof accounting. A group completes only when all
its profiles have completed successfully. Worker counts may change diagnostic
root counts, but cannot change completed mathematical results.

Astra sends independently validated witnesses with the existing exact `layout-v1`
identity. It no longer computes a second exhaustive byte-minimal witness merely
to select an optimal tie. The shared `SolutionCollector` retains its public
identity and deduplication behavior. Enumeration retains every required physical
layout up to isomorphism; delivery order and chosen representatives may vary.

Cancellation interrupts the polling reader, kills and waits for each owned cvc5
process, joins reader and worker threads, and returns incomplete with retained
incumbents and delivered enumeration. No detached cleanup contributes a false
completion time. Unexpected backend failures stop sibling work and preserve the
same result state.

## Verification and measurement

`cargo test -p solver-astra` invokes the installed cvc5. Its differential tests
compare full preferred and enumeration witnesses with independent Reference,
including arbitrary rational requests, surplus, duplicate terminals, scaling,
capacity, scope, cancellation, and different worker counts. App contract tests
include Astra through the production dispatcher.

Build the common release runner with:

```powershell
cargo build --release -p synthetizer-app --example profile_engines --offline
```

`scripts/start-benchmark-screen.ps1` accepts manifests with an explicit `Engine`
per job (`custom`, `z3`, `astra`). Those jobs use the common production API runner,
baseline stage and profiling off; legacy manifests retain the original runner.
Use separate binary variants for distinct engine identities. The launcher freezes
the binaries, cvc5 and adjacent DLLs, source, cases, schedule, and hashes, and pins
Astra to the frozen backend. It signals completion or verification failure through
status files, a sound, and a Windows dialog. Launch a screen only after all builds
and tests have finished, then leave the machine idle until it completes.

The runner saves the original `native_outcome` and a comparison representation
that canonicalizes full witnesses with the authoritative normalized byte protocol.
The latter is generated after the timed solve and its cost is separately saved as
`comparison_canonicalization_s`; it never substitutes another preferred topology.
The analyzer compares objectives, full key sets, preferred identities, and full
solution objects. Separate fields record first accepted witness, optimal scope
completion, minimum-link enumeration, and all-link enumeration. During `all`,
Astra publishes the minimum-link milestone; unavailable competitor milestones
remain null. Terminal wall time includes worker cleanup. Parent CPU and memory
samples do not include the cvc5 child processes and must not be interpreted as
total Astra resource consumption.

The first release screen is an experiment, not evidence that Astra is faster.
Only completed matching scopes qualify for speed comparisons. Timeouts and
watchdog kills remain incomplete or failed measurements. Large equal-rate symmetry
classes, repeated construction across L groups, and canonicalization are candidate
optimization areas to investigate from those results.

The initial screen has 112 jobs: two balanced Custom/Astra and Custom/Z3 pairs
for each selected corpus scope, using 32 workers. Its maximum scheduled allowance
is 5 hours 24 minutes, including every cancellation-cleanup allowance; completed
jobs finish sooner. Launch it with:

```powershell
./scripts/start-benchmark-screen.ps1 -JobManifest benchmarks/astra/screening.json -VariantBinaryMap benchmarks/astra/variant-binaries.json -OutputDirectory target/astra-screen
```

Use a new output directory for each screen. Read `BENCHMARK-STATUS.txt` and
`results/summary.json` when the completion dialog appears. Verification failures
must be investigated before making performance claims.

Local validation on 2026-09-07 passed: 6 Astra tests, 17 shared app tests,
10 Tauri/history tests, 108 frontend tests, and 40 Python harness tests with one
skip. Strict Clippy passed for every workspace target; Rust/changed-file frontend
formatting and Svelte checks passed. These checks establish the tested contracts;
hard-corpus correctness and speed remain subject to the release screen.

## First screen findings and partition experiment (2026-09-08)

The frozen `target/astra-screen-20260907` screen attempted all 112 jobs without
worker failures or watchdog kills. Verification failed with 10 messages on six
runs, all for preferred-witness differences on `24 = 7+6+5+4+2`. The complete
six-layout canonical sets and full solution objects agree across all engines.
All reported objectives agree at N=7, L=8. Custom's `optimal` preferred graph
is different from its `all` preferred graph; Z3's `all` preferred graph is also
different from the first graph in canonical byte order. Astra selects that first
graph in both scopes. The new preferred-order regression validates all six saved
graphs, compares the independent Reference canonicalizer's keys and graphs with
the production canonicalizer, and checks Astra's preferred result. It passes.
The original strict verifier failure is retained; no competing engine behavior
or comparison rule was changed to conceal it.

Initial release medians with 32 workers and two repeats per pair:

| Scope             |    Custom seconds |     Astra seconds |        Z3 seconds |
| ----------------- | ----------------: | ----------------: | ----------------: |
| 36 all            |            60.568 |            66.976 | incomplete at 180 |
| 115 all           |            15.264 |            62.524 | incomplete at 180 |
| 238 all           |             5.168 |            26.026 |           101.429 |
| 10 optimal        | incomplete at 180 | incomplete at 180 | incomplete at 180 |
| 258 minimum_links | incomplete at 180 | incomplete at 180 | incomplete at 180 |

Custom values use the paired Custom/Astra cohort. Z3 has its own paired Custom
cohort. These are completed wall times where available; deadline entries are
not speed comparisons. Astra is slower than Custom on every completed initial
scope. On 258 it retains two N=9, L=14 layouts, with the first accepted witness
at about 116 seconds; this is an incumbent result, not completed enumeration.

The partition experiment adds explicit output-source roots and a completion
ledger. Eight Astra tests and 17 shared app tests pass, as does strict workspace
Clippy. The targeted 48-run manifest is `benchmarks/astra/partitions.json`, with
binary mapping `benchmarks/astra/partition-binaries.json`. It compares Custom
and partitioned Astra on six complete all-L cases, tiny minimum_links, 10 optimal,
and 258 minimum_links. It also compares frozen original Astra against the new
binary on 36/115/238 all. Both comparison pairs have two repeats at 32 workers.
The maximum scheduled allowance is 9,360 seconds (2 hours 36 minutes), including
cleanup. This experiment does not replace the first screen's strict verdict.

The original Astra runner is preserved at
`target/astra-before-partitions-20260908/bin/profile_engines.exe`, SHA-256
`f6546c55cffadec1f676a46f1be68c4403d62ae8c8334de190b95c1e33d518ca`, together
with its frozen source and origin metadata. A successful plan-only preparation
is at `target/astra-partitions-plan-20260908-v2`; no timing jobs ran there.

## Partition screen results and direct-flow experiment (2026-09-08)

`target/astra-partitions-screen-20260908` completed all 48 records and passed
strict verification. Full saved objects, key sets, preferred witnesses, objectives,
and completion agree with available completed Custom references. On 258 there
is no completed independent reference yet: Astra's two runs agree with each other,
and their witnesses validate, but this is not a cross-engine completeness check.
The initial 24 optimal preferred-order mismatch remains an unresolved competitor
contract difference; this narrower screen did not retest or erase that failure.

| Scope             |    Custom seconds | Partitioned Astra seconds | Interpretation                                   |
| ----------------- | ----------------: | ------------------------: | ------------------------------------------------ |
| 36 all            |            61.712 |                    52.882 | 14.3% less completion time, matching full result |
| 115 all           |            15.495 |                    22.532 | Astra takes 1.45 times Custom's time             |
| 238 all           |             5.148 |                    27.394 | Astra takes 5.32 times Custom's time             |
| 258 minimum_links | incomplete at 180 |                    70.519 | N=9, L=14, two layouts; no exact speedup ratio   |
| 10 optimal        | incomplete at 180 |         incomplete at 180 | No proof-speed comparison                        |

Separate matched before/after Astra pairs show 66.783 -> 51.460 seconds on 36
and 58.482 -> 20.445 seconds on 115, but 28.002 -> 31.492 seconds on 238.
The last is an adverse result, with individual partitioned runs spanning
27.804–35.179 seconds. Each pair has two repeats at 32 workers.

The next candidate simplifies conditional arithmetic for every destination that
has exactly one incoming belt. Exactly one active Boolean plus per-edge rate
equality is logically equivalent to the former conditional sum. No routing
assignment is removed. Multiple-input mergers and multiple discard belts retain
sums. This changes only Astra's encoding. A new Reference differential test uses
5 = 2+2+1 at capacity 6: its minimum topology requires feedback, and internal
flow exceeds external supply. It checks full enumeration and a cyclic preferred
witness, covering direct splitter equations and merger sums together.

The successful partition implementation is preserved with source and origin
metadata at `target/astra-before-direct-flow-20260908`; runner SHA-256 is
`8cb6248770aeaed5f28e54a192ec4a3f8d6a1c5b1e542f0dd5add41d026699ef`.
The candidate manifest is `benchmarks/astra/direct-flow.json`, with binary map
`direct-flow-binaries.json`. It contains 52 jobs: matched old/new Astra pairs on
nine scopes, plus Custom/new pairs on 36-all, 115-all, 238-all, and 258-minimum_links.
The latter gets a 900-second deadline per engine and repeat to seek an independent
completed reference. The maximum scheduled allowance is 10,440 seconds (2 hours
54 minutes), including cancellation cleanup. The measured results of this candidate are recorded below.

## Verified final release results (2026-09-08)

`target/astra-direct-flow-screen-20260908` completed all 52 records and passed
strict verification. A separate audit checked all 26 matched pairs: 24 pairs
completed with identical canonical key sets, full saved objects, preferred keys,
shared optimality proofs, and enumeration completion states; two pairs on
10-optimal remained incomplete. Options and problem identity matched within each
pair. The longer Custom runs completed 258, independently confirming Astra's
entire minimum-link result set at N=9, L=14, with two layouts.

Matched release medians, 32 workers, two repeats per pair:

| Scope             | Custom seconds | Astra seconds | Custom / Astra | Layouts |
| ----------------- | -------------: | ------------: | -------------: | ------: |
| 36 all            |         62.521 |        20.737 |           3.01 |      12 |
| 115 all           |         15.881 |        10.869 |           1.46 |      49 |
| 238 all           |          5.247 |         1.871 |           2.80 |       1 |
| 258 minimum_links |        203.874 |        51.739 |           3.94 |       2 |

These are completion speedups for the named scopes, with full-result equality.
They do not imply the same gains for optimal-only search or arbitrary problems.
Astra's current 10-optimal runs still stop incomplete at the 180-second deadline.
They retain validated N=11, L=18 incumbents found at 114.086 and 113.114 seconds;
zero enumerated solutions in optimal mode does not mean no incumbent exists. Smaller cases and all supported worker counts are not claimed
universally faster. The two repeats are screening evidence, not a statistical
confidence interval.

The paired older/newer Astra all-L medians improve from 52.919 to 20.434 seconds
on 36, 23.454 to 10.761 on 115, and 31.123 to 1.917 on 238. The new formulation
therefore resolves the previous measured 238 regression on this screen. The old
screen and its adverse result remain preserved.

Z3 was measured in the initial screen, not rerun here. It completed 238-all in
101.429 seconds and remained incomplete at 180 seconds on 36-all, 115-all,
258-minimum_links and 10-optimal. These are historical timings with a different
Astra version; no newly matched Z3 speed ratio is claimed. The initial 24-optimal
preferred-witness difference remains documented and checked with independent
Reference canonicalization. Custom and Z3 behavior was not changed to eliminate
that difference.

The current workspace solver sources and release runner were checksum-compared
with the successful frozen screen. No solver edits followed that screen. Nine
Astra tests, 17 shared app tests, strict all-target workspace Clippy and release
runner/NSIS builds passed before measurement. Earlier frontend and Tauri checks
are recorded above. The final installer requires the separate cvc5 installation
specified in Setup.

- Release runner SHA-256:
  `52ef07b33475f7e0d006a17f3084c2d199ff6c6324d140bc2d976a3a871cdcdc`
- NSIS installer SHA-256:
  `3f620eeab98d34ab336e4435996de27c5f0d4f687a957a5e13be2f414c1ebfa7`
- Installer: `target/release/bundle/nsis/Satisfactory Flow Synthetizer_0.2.0_x64-setup.exe`

No additional benchmark is running. No installer was applied. The measured implementation is preserved on the
`codex/astra-exact-solver` branch.

## First optimum experiment (authorized 2026-09-08)

The user explicitly relaxed preferred-witness ordering and authorized further
optimization with screens up to eight hours. The first candidate implements
first-witness completion for optimal mode, removes preferred-order-only witness
canonicalization from Astra, and adds optional root diagnostics for the next
optimization decisions. It does not change Custom or Z3 search behavior.

A root can report `Optimum` after a validated witness or `Exhausted` after UNSAT.
Only the latter increments root/profile exhaustion. The coordinator returns an
optimum only after joining workers and checking user cancellation and worker
failures. Internal success stops do not become user cancellations; a user cancel
still returns incomplete with retained witnesses. Enumeration never uses the
early optimum path and still exhausts its complete required groups.

`ASTRA_DIAGNOSTICS=1` records root/profile IDs, objective, source partition, start
and wall times, time in cvc5 checks, independent validation and exact layout
identity, plus model/duplicate/validated counts and completion state. Diagnostics
are disabled by default. The production runner records these as `astra_roots`.
A manifest's `AstraDiagnostics` flag is checked and paired settings must agree.
Diagnostic-repeat pairs are evidence collection, not optimization speed claims.

The benchmark manifest's explicit `ResultPolicy: any_optimum` permits different
validated optimal ties. Optimal runs must agree on N/L and return their validated
terminal object; when a completed full enumeration exists, that witness must be
in it. Enumeration comparisons still require complete canonical key sets and
full saved objects; only the preferred tie is flexible. Existing manifests retain
the default strict ordered policy and frozen earlier screens remain unchanged.

`benchmarks/astra/first-optimum.json` has 94 sequential jobs: 60 old/new Astra
comparisons, 24 Custom/new comparisons and 10 separate diagnostic runs, with two
runs per diagnostic scope. It covers all eight corpus cases and all three scopes.
10 gets 900-second deadlines; the Custom/258 controls and diagnostic/258 runs get
600 seconds. The maximum total allowance is 24,900 seconds (6h55), including
cleanup, within the authorized eight hours. The committed baseline runner and
source are preserved at `target/astra-before-first-optimal-20260908`.

The next steps remain measurement-driven: reduce repeated isomorphic models,
split long roots more effectively, test Boolean belt-count encodings, and derive
stronger proven arithmetic cuts. None is assumed faster before a matched screen.

## Verified first-optimum screen (2026-09-08)

`target/astra-first-optimum-screen-20260908` finished verification with 94 records
and no failures: 90 completed and four explicitly timed out. The four timeouts
were the previous Astra and Custom, each twice on 10-optimal at 900 seconds.
The candidate completed that problem with validated N=11, L=18 witnesses: median
112.719 seconds in the prior/current pairs and 117.117 in the Custom pairs.
No finite completed-time speed ratio is assigned to those timed-out controls.

Matched 32-worker, two-repeat medians (seconds):

| Scope             | Previous Astra | First-optimum Astra | Result                     |
| ----------------- | -------------: | ------------------: | -------------------------- |
| 36 optimal        |          9.598 |               7.641 | about 20.4% less wall time |
| 258 optimal       |         50.327 |               6.807 | about 7.4 times faster     |
| 238 optimal       |          1.811 |               1.227 | about 32.2% less wall time |
| 36 all            |         20.445 |              20.639 | about 0.9% more wall time  |
| 115 all           |         11.009 |              11.025 | about 0.1% more wall time  |
| 238 all           |          1.904 |               1.958 | about 2.8% more wall time  |
| 258 minimum_links |         50.454 |              50.531 | about 0.2% more wall time  |

Separately paired Custom/Astra medians were 9.290/7.719 on 36-optimal,
61.986/20.346 on 36-all, 15.861/10.790 on 115-all, 5.207/1.867 on 238-all,
and 205.593/50.929 on 258-minimum_links. Completed enumeration sets and full
canonical objects matched available references, with 12, 49, 1, and 2 layouts
respectively for the latter four enumeration scopes. Two repeats establish
screening evidence, not a universal performance guarantee. Z3 was not rerun;
its prior results remain historical.

Root diagnostics point at cvc5 checking, rather than identity/validation or
repeated models: each run of 36, 238, 258 and optimal 10 had zero within-root
duplicate models; 115 had one duplicate among 50 models. On 258, summed root
check time was 201.961–204.751 seconds versus 202.137–204.918 root wall seconds.
The longest 258 root took 50–51 seconds and returned UNSAT with no models.
The 10-optimal roots spent essentially all their wall time awaiting cvc5.
These are overlapping worker wall times, not process CPU or total solve time.

The verified first-optimum runner and sources are preserved at
`target/astra-before-cardinality-20260908`. Its runner SHA-256 is
`d344ead5543dbe41db799233c355d9a9a867486a047c42f33f81773ddb9b2b26`.

## Exact count constraint experiment (2026-09-08)

Two successive candidates address the observed SMT cost and are measured
separately. Their performance is not yet established.

First, replace the dense internal-link count by the equivalent sparse terminal
count. Let P be the total number of operator input ports, I the external input
count, X the belts from external inputs to operators, and D the belts from
external inputs directly to requested outputs or discards. The complete port
constraints give P=L+X and I=X+D, hence L=P-I+D. We assert D=L+I-P using only
the direct terminal edges. A negative required count is impossible. This proof
allows cycles, multiple inputs, parallel belts and discards; it assumes no rate
pattern or benchmark identity. The candidate is frozen at
`target/astra-sparse-links-candidate-20260908` after all 10 Astra tests passed.

Second, express exact edge counts in rows, columns and the direct-terminal set
with Boolean prefix thresholds. Each auxiliary q(i,j) is equivalent to at least
j selected edges among the first i edges, using both directions of
q(i,j) = q(i-1,j) OR (q(i-1,j-1) AND edge(i)). The final prefix requires threshold
k and forbids k+1. Complementing all literals for k > n/2 reduces counter width.
Zero, full and impossible counts are handled explicitly. Every edge assignment
with exactly k selected edges extends to the auxiliary variables, and every
satisfying auxiliary assignment implies exactly k edges. Flow equalities remain
exact rational arithmetic. No dependency or cvc5 option changes are involved.

A truth-table test queries cvc5 for every edge assignment up to six literals
and all requested counts through n+1. The solver contract suite also compares
full Reference enumeration with cycles, surplus, rational rates and scaling,
and exercises first-optimum stopping, cancellation and root proof accounting.

`benchmarks/astra/cardinality.json` prepares 132 sequential release runs:
60 first-optimum/sparse-count comparisons, 60 sparse-count/Boolean comparisons,
and 12 separate diagnostic observations. The timeouts plus cleanup allowances
total 19,080 seconds (5h18), below the authorized eight hours. This screen keeps
all eight corpus cases and all three scopes, with 300-second limits for 10,
180 seconds for 258-minimum_links and matched AB/BA pairs. Sources, binaries,
backend and harness are frozen. The result policy remains `any_optimum`; full
enumeration identity and saved-object comparisons remain mandatory.

## Verified count-constraint screen (2026-09-08)

`target/astra-cardinality-screen-20260908` verified all 132 records with no
failures. 128 solves completed. The four explicit timeouts were Boolean 10
optimal (two main timing runs and two diagnostic runs), all capped at 300
seconds. Diagnostic runs produced no 10 models before the cap.

Matched 32-worker medians, two runs per variant (seconds):

| Scope             | First-optimum | Sparse links | Sparse control |      Boolean counts |
| ----------------- | ------------: | -----------: | -------------: | ------------------: |
| 10 optimal        |       121.187 |       22.731 |         20.655 | incomplete at 300 s |
| 36 optimal        |         8.249 |        8.645 |          8.652 |               2.659 |
| 36 all            |        22.819 |       19.432 |         18.185 |               4.276 |
| 115 all           |        11.551 |       16.580 |         16.547 |               5.936 |
| 238 all           |         2.007 |        2.281 |          2.193 |               0.633 |
| 258 optimal       |         7.110 |       38.921 |         40.682 |               2.043 |
| 258 minimum_links |        50.268 |       68.810 |         61.699 |              49.198 |

The first two timing columns are one paired comparison; the last two are
another. Both sparse columns use the same binary in separate pairs. This
experiment improved important cases but found adverse results for both single
formulations. In particular, Boolean counters cannot replace sparse counts
unconditionally on the evidence for 10. Full canonical enumeration objects
and objective/validation checks passed. Neither Custom nor Z3 was rerun here.

## Independent formulation portfolio experiment (2026-09-08)

The next candidate races complete sparse-count and Boolean-count searches.
Their worker allocations sum to the requested count: at 32 workers, each gets
16; the sparse search gets the extra worker for odd totals. A one-worker request
runs Boolean counts alone. Both searches use the same objective ordering,
exact validation and complete enumeration contracts. No rates, case names or
corpus labels select the formulation.

Each search owns an independent proof ledger. The first completed search can
supply the terminal proof only after both searches and their workers have been
joined. Internal cancellation stops the loser; explicit user cancellation
always produces incomplete and retains validated witnesses. Proof counters
are copied from the returned proof owner, never summed across formulations.
A failure or incomplete outcome in one search does not discharge any obligation
in the other; only an independently completed proof can win.

Incumbents are globally improved in N/L order, retaining the first equal tie.
The shared collector deduplicates enumeration by exact layout identity. Each
search independently proves the same minimum N before delivering enumeration;
therefore their delivered layouts belong to the same required scope. A
completed enumeration from the winner covers any layouts delivered by the
loser. An equal-optimum witness from the other search may be returned with the
winner's objective proof, since that proof concerns N/L, not representative
ordering.

Diagnostics attach a `branch` field to root records (0 sparse, 1 Boolean) and
publish `astra.portfolio_proof_owner` after joins. Interpret root identities as
(branch, N, L, root); only the returned owner's exhaustion records correspond
to terminal proof counters. Progress retains the current branch's valid
obligation and labels its source; the outer collector supplies global incumbent
and enumeration counts.

All 11 Astra tests passed with diagnostics, including full cyclic Reference
enumeration, rational/scaling fixtures, explicit cancellation, first optimum
and proof-owner root accounting. Strict workspace/all-target Clippy passed.
The new screen `benchmarks/astra/portfolio.json` has 130 sequential jobs:
60 sparse/portfolio runs, 60 Boolean/portfolio runs and ten separate diagnostic
observations. With cleanup, the maximum allowance is 18,240 seconds (5h04).
All timed jobs request the same total of 32 workers. The previous single
formulations remain frozen for matched comparisons. Portfolio speed is
unmeasured until this screen completes; resource competition may outweigh its
benefit and must be measured rather than assumed away.

## Verified portfolio screen (2026-09-08)

`target/astra-portfolio-screen-20260908` verified all 130 records with no
failures. All portfolio runs completed. The only two timeouts were the Boolean
10-optimal controls, capped at 300 seconds. All ten diagnostic runs were also
checked separately: the returned proof owner's exhausted-root records exactly
match its terminal root exhaustion counter.

Two-repeat matched medians at 32 total workers (seconds):

| Scope             | Sparse | Portfolio paired with sparse |             Boolean | Portfolio paired with Boolean |
| ----------------- | -----: | ---------------------------: | ------------------: | ----------------------------: |
| 10 optimal        | 20.133 |                       24.221 | incomplete at 300 s |                        24.522 |
| 36 optimal        |  8.427 |                        3.186 |               2.603 |                         3.124 |
| 36 all            | 18.153 |                        5.241 |               4.216 |                         5.170 |
| 115 all           | 15.745 |                        6.333 |               5.872 |                         6.180 |
| 238 all           |  2.166 |                        0.700 |               0.627 |                         0.693 |
| 258 optimal       | 35.108 |                        2.493 |               1.995 |                         2.555 |
| 258 minimum_links | 64.331 |                       59.582 |              49.474 |                        58.779 |

The portfolio combines completion on 10 with most Boolean gains elsewhere,
but sharing resources adds overhead relative to the best standalone form.
For example 36-all was about 23% slower and 258-minimum_links about 19% slower
than Boolean in their matched pairs. Tiny runs grew from about 0.03 to 0.04
seconds. These adverse results remain preserved. Full canonical enumeration
sets, saved objects, exact objectives and independent validation passed.
Custom and Z3 were not rerun; their earlier measurements remain historical.
