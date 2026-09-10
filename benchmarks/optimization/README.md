# Prepared optimization screens

The adaptive batch is complete: see
[Adaptive results and scheduling stopping point](results-adaptive-20260910.md).
Keep the promoted static policy; both adaptive replacements regress hard
32-worker completion. The approved [final hybrid comparison](hybrid-final.md)
tests adaptive fallback while preserving static splitting. It has 48 jobs at
8/16/32 workers, a 2 h 58 m allowance and a three-hour hard limit. Analyze this
last batch, then pause; broad scheduling sweeps should end.

The focused combined-policy campaign is complete: see
[Partition promotion confirmation](results-promotion-20260910.md). All 56 timing
solves completed, both hard cases improved in all six pairs, and static Boolean
partitions are integrated into production alongside descending first-output order.

The recorded adaptive batch is [Adaptive comparison after partition promotion](adaptive-followup.md).
Use `--adaptive` to compare both adaptive candidates against production revision
`6852be9`, with a maximum allowance of 2 h 48 m and a three-hour controller limit.
New timing campaigns use at least 8 total workers.
Earlier decisions and performance requirements are in
[10 September results](results-20260910.md). The
discovery and follow-up sections describe the recorded experiments; reproduce
their source with the recorded revision and frozen manifests.

## Recorded promotion check

`python scripts/make-optimization-campaign.py --promotion --output <new-directory>`
prepares five suites: regression guards at 8, 16 and 32 workers, then six paired
repeats each for cases 258 and 97 All min N/L at 32 workers. Build `baseline` and
`pairs-boolean` from `cd703e9`, the source used in that campaign. Thus each pair
compares descending order against descending order plus Boolean second-output partitions.
The partition candidate preserves descending parent order and interleaves second
output producers using the existing static refinement policy.

The 56 jobs reserve 9,860 seconds for search and per-job cancellation cleanup,
plus 600 seconds for suite setup and verification: **2 h 54 m 20 s** in total.
The controller also stops its owned process tree if a stalled runner or verifier
reaches the session limit. Interrupted evidence remains explicitly incomplete.
Use `validate-optimization-followup.py --promotion` for release proof checks
before freezing. No candidate is automatically promoted by the runner.

The completed discovery findings and the next implementation are recorded in
[Scheduling and adaptive partitioning follow-up](followup.md).

Discovery baseline: merged revision `2d4e0d6` on `develop`.
The preparation scripts generate source variants outside the checkout, run exactness
and cancellation tests, run strict Clippy, build release runners, and freeze their
source, configuration, patches, logs, backend and hashes. Preparation runs no timing
screen. Candidate binaries are experimental and are not used by the desktop build.

## Questions and candidates

| Family                  | Candidates                                    | Question                                                                                                           |
| ----------------------- | --------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| Minimum-link partitions | `pairs-both`, `pairs-sparse`, `pairs-boolean` | Does second-output partitioning help `All min N/L` across the corpus and new requests? Which formulation benefits? |
| Worker allocation       | `sparse25`, `sparse75`                        | Can 8/24 or 24/8 beat the baseline's 16/16 split at 32 total workers?                                              |
| Startup                 | `delay-sparse250`, `delay-boolean250`         | Does delaying one formulation by 250 ms avoid startup overhead without losing hard-case completion?                |
| Worker controls         | `sparse-only`, `boolean-only`                 | How do the independent formulations compare with the portfolio at 1, 2, 8, 16 and 32 total workers?                |
| Root diagnostics        | Separate paired observations                  | Which exact obligations dominate, and which formulation owns each returned proof?                                  |

Allocation rounds to whole workers, keeping at least one worker per formulation.
Startup-delay variants retain the 16/16 allocation; the delayed workers remain idle
during the delay. Cancellation interrupts that wait, and all searches and backend
processes are joined. One-worker production behavior remains Boolean-only except
for the explicit sparse-only control. No policy depends on case names or rates.

Partitioning applies only to `All min N/L`, at groups with fewer live first-output
roots than workers and at least two requested outputs. For each parent, the second
output's unique producer gives disjoint children whose union is the entire parent.
Only children enter the ledger; all must finish before the parent profile counts as
exhausted. This experiment targets the previously observed enumeration benefit.
It does not establish that arbitrary early splitting is a good default.

Delayed splitting that preserves the parent and new mathematical cuts remain
follow-up hypotheses. Root identities and phase costs from this campaign will guide
their design; there is no unproved cut in these candidates.

## Coverage and order

The discovery matrix contains 578 jobs / 289 matched pairs in 12 independent
suites. There are nine corpus requests, seven additional shapes (rational,
surplus, capacity, cyclic and multi-output), and a scaling-equivalence check.
`scaled258` is an invariance check, not an independent new hard problem.

Each timing comparison has two balanced repeats, identical exact problem, scope,
node cap, total workers, deadline, affinity policy and instrumentation. Both pair
members run consecutively; pair order is shuffled. Diagnostic pairs have one
repeat and are excluded from primary timing conclusions. Search and cleanup
allowances are recorded explicitly. The entire discovery matrix has a 12 h 35 m
30 s worst-case allowance, split into suites capped at approximately 13 minutes
to 1 h 45 minutes each. This is a sum of deadlines, not an expected runtime or an
instruction to run every suite unattended.

Suggested order:

1. `pairs-both`, `sparse25`, `sparse75`: screen the main hypotheses.
2. `pairs-sparse`, `pairs-boolean`: attribute any partition benefit or regression.
3. Startup-delay suites and worker controls: assess overhead and portability to
   smaller worker budgets.
4. Root suites: capture and audit the expensive obligations separately.
5. Generate six-repeat confirmation suites for candidates supported by the
   discovery results. No candidate is designated a finalist in advance.

Incomplete stress cases are preserved. They are not completed timing samples.
Additional cases may prove bounded impossibility or reach their cap; either outcome
must be reported explicitly and cannot supply a completed-time speedup ratio.
Each partition suite also times case 10 `One min N/L`, case 258 `One min N/L`,
and case 36 `All min N` to guard the scopes that must retain their behavior.

## Prepare and run

```powershell
npm run prepare:cvc5
python scripts/prepare-optimization.py --revision 2d4e0d6 --output target/optimization-prep
python scripts/make-optimization-campaign.py --output target/optimization-matrix
./scripts/freeze-optimization-campaign.ps1 -MatrixDirectory target/optimization-matrix -VariantBinaryMap target/optimization-prep/variant-binaries.json -OutputDirectory target/optimization-frozen
```

`--resume` resumes variant preparation. Completed variants are reused only after
their stored hashes and newly generated formatted solver/test sources match.
Each variant retains the exact recipe hashes used for its own validation.

To launch **one** prepared suite when ready:

```powershell
./scripts/start-optimization-suite.ps1 -PreparedSuite target/optimization-frozen/pairs-both
```

The launcher checks frozen hashes, starts a hidden worker, writes durable status
and displays a completion dialog after both verifiers finish. End the agent turn
after launching and leave the machine idle. A finished suite is immutable; prepare
a new output directory for another run. The launcher does not chain other suites.

To launch the full prepared queue sequentially, with one final completion dialog:

```powershell
./scripts/start-optimization-campaign.ps1 -PreparedCampaign target/optimization-frozen
```

The campaign records each suite's outcome and continues with independent suites
if one fails verification. It never overlaps timing runs. Inspect
`CAMPAIGN-STATUS.txt`, `suite-outcomes.json`, and the individual verification logs.
The three-hour run allowance is enforced: a suite starts only if its
entire search/cleanup allowance plus two minutes for setup and verification fits
the remaining time. Otherwise the controller pauses between suites and writes
`remaining-suites.json`. This avoids interrupting a matched pair or comparing a
half-written suite. The full matrix can still finish in one run when completed
jobs use substantially less than their caps.

After screening, prepare a separate confirmation matrix, for example:

```powershell
python scripts/make-optimization-campaign.py --output target/optimization-confirmation --finalists pairs-both
```

The example names a candidate for command syntax only. Confirmation uses six
paired repeats across the relevant corpus and additional cases, plus a separate
four-repeat hard-case suite at 1, 2, 8 and 16 workers for that same candidate.
Freeze both with the same verified variant map before running them.

## Promotion criteria

- Exact independently validated witnesses and full canonical enumeration sets
  must match. Any optimal tie is allowed for `One min N/L`; order is irrelevant.
- Proof completion, cancellation and joins must pass. Diagnostic exhaustion must
  equal the returned proof owner's records, never the sum of competing ledgers.
- Preserve case-10 completion and guard case 36 `All min N` and case 258
  `All min N/L`. Prefer reductions in terminal wall time on hard scopes.
- Treat two repeats as screening evidence. Require confirmation of promising
  changes and retain all regressions, caps and missing complete comparisons.
- Assess allocation by each measured worker budget. Do not extrapolate 32-worker
  results to one-worker behavior or select production behavior by case name.
- Keep changes separate until their effects are established. Test a combined
  finalist against the baseline before promoting the combination.

`results/summary.json` contains exact-result and paired timing verification.
`results/root-audit.json` contains proof-owner checks, refinement activation and
the 20 longest exact roots per diagnostic request. Root times overlap across
workers; they are not CPU totals. Retain both files with all raw samples.
