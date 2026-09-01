# Benchmark guide

## Cases and modes

All new file cases are in `benchmarks/custom/cases/` and use max link rate 1200.
Names describe the test corpus only, not information passed to solver policy.

The completed isolated encoding screen is described in
`mem:solver/experiments/09-serializer-and-find-all`. Its before binary is a
fresh legacy-encoding build with the current constructor and boxed keys, not the
old recovery binary. The `acyclic24_diagnostic` alias uses the same case file to
keep instrumented samples out of timing groups. Source patches live in
`benchmarks/custom/variants/`; never apply them during a running screen.
Its verified results (`mem:solver/experiments/09-serializer-and-find-all-results`) include
eight incomplete hard enumerations; faster first witnesses do not prove faster completion.

| Label          | Exact problem      | Role and last reviewed scope                                |
| -------------- | ------------------ | ----------------------------------------------------------- |
| `acyclic24`    | 24 = 7+6+5+4+2     | Complete optimal/all regression, N<=7                       |
| `cyclic65`     | 65 = 40+25         | Complete optimal/all regression, N<=6                       |
| `acyclic36`    | 36 = 11+9+7+5+3+1  | Difficult optimal/all, N<=9; known optimum N=9, L=11        |
| `cyclic10`     | 10 = 6.04+3.96     | Difficult optimal, N<=11; starting proven lower bound is 11 |
| `cyclic238`    | 238 = 60+20+108+50 | Medium optimal/all; measured 69-73 s / 101-111 s; 1 layout  |
| `cyclic115`    | 115 = 75+40        | Medium optimal/all; measured 11 s / 160 s; 49 layouts       |
| `medium258`    | 258 = 195+63       | Longer minimum-link control; user observed about 4m33       |
| `profile_tiny` | 2+3 = 1+4          | N<=2, fixed scheduler overhead                              |

The old tiny records used capacity 5. The newer example uses 1200. Do not silently
merge them. The user originally reported roughly 2.5/30 s for 24 optimal/all and
1.3/31 s for 65; hard 36 optimal about 6m30s, all stopped after 30m with eight
found; hard 10 optimal stopped after 25m at N=11/L=18. These are user observations,
not controlled solver measurements or complete result-set references.

`optimal` calls the real single-result API. `minimum_links` enumerates every layout
at minimum N and minimum L. `all` enumerates every feasible L at minimum N. A node
cap limits the largest N, not the starting N. The N<=10 cyclic10 attempt was below
its lower bound and did no search. Before SAT, duplicate all-mode runs can repeat
the same proof work without exercising remaining-group concurrency.

The [prefix protocol](experiments/15-prefix-workloads.md) adds `scope: selected_prefix`.
Its certificate includes frontier keys, decisions and the selected path. Compare
only identical certificates; nested prefixes must not be added as disjoint work.
Search wall time excludes separately recorded preparation; process time includes it.
The [adaptive-root protocol](experiments/21-adaptive-root-profiling.md) instead
freezes the full ordered production partition-key plan and one ordinal, then reports
`scope: selected_root`. Its exhaustion applies only to that root. Root recipes require
one profile and p1; prefix and root selections are mutually exclusive.

## Source variants / patches

Patches under `benchmarks/custom/variants/` are benchmark inputs, not production
options. Never apply them during a running screen. Use a detached checkout and its
own Cargo target directory; do not share build artifacts across source variants.

- `legacy-semantic-encoding.patch` — restores dense decimal rational row encoding
  while keeping boxed keys and the current constructor. Exp 09 `before` binary;
  see `mem:solver/experiments/09-serializer-and-find-all`. Not the old recovery binary.
- `constructor-five-second-budget.patch` — restores the provisional five-second
  constructor deadline from exps 07–08. Preserved for future use; **not** applied in exp 09.
- `early-exact-l.patch`, `cached-witness-leaves.patch`, `direct-rref-bounds.patch` —
  sequential isolated diffs for exp 13 against reference `1b558a6`. Already in the
  current tree; do not re-apply. Details:
  `mem:solver/experiments/13-calculation-changes`,
  `mem:solver/experiments/13-calculation-screen`,
  `mem:solver/experiments/13-calculation-results`,
  `mem:solver/experiments/14-calculation-promotion`.

Apply with `git apply --check` then `git apply` (use `--ignore-space-change` when
checking against untracked frozen CRLF copies). Do not combine legacy/deadline
patches merely to reproduce old timings.

## Running a screen

The [38-job follow-up](experiments/16-post-calculation-screen.md) combines prefix
discovery with whole p1/p14 timing and repeated hard optimal. It preserves one
completion/failure signal after both phases. [Results](experiments/16-post-calculation-results.md)
pass after a reference-selection fix; all hard prefixes capped, so discovery must
go deeper before another repeated prefix comparison.

The verified [calculation comparison](experiments/13-calculation-screen.md) freezes
four cumulative variants and runs fixed-work/replay then whole optimal/all phases
sequentially. `start-hard-profile.ps1 -PlanOnly` validates without launching.
[Results](experiments/13-calculation-results.md) cover 80 jobs. The complete 36
N=9/L=12 group is now another bounded reference, not a full enumeration reference.

For exact N/L/profile diagnostics, use the separate [fixed-work protocol](experiments/12-hard-obligation-profiling.md).
Its `best` mode is profile-local; keep it separate from whole `optimal` solve results.
The [verified hard results](experiments/12-hard-obligation-results.md) supply four
complete 36 profile controls and identify which 10 profile still requires a cap.

Do not build/test concurrently with timing runs. Use a separate Cargo target
directory for an isolated checkout; sharing targets across different uncommitted
sources previously produced stale metadata. Do not delete evidence to fix a build.

```powershell
cargo build --release -p solver-core --example profile_case --example profile_witness
./scripts/start-benchmark-screen.ps1 -JobManifest benchmarks/custom/repeat-scheduling-screening.json -ReferenceBinaryDirectory target/parallelism-ladder/recovery-20260828/bin -OutputDirectory target/parallelism-ladder/NEW_RUN
```

Use a new output directory; existing directories are rejected. The background
launcher freezes scripts, binaries, case inputs, manifest and solver sources.
It displays completion/failure and writes authoritative `BENCHMARK-STATUS.txt`
plus `BENCHMARK-FINISHED.txt` or `BENCHMARK-FAILED.txt`. Current job status is under
`results/`. Keep the machine awake; end the agent turn after launch.

File-manifest jobs accept `Variant: before|after`, default `after`. Before jobs
require a compatible `profile_case.exe` in `ReferenceBinaryDirectory`. They do not
implicitly duplicate every expensive job. Per-job `Hotspots` may be `on` or `off`.
Every job writes `<job>.process-samples.csv` with cumulative process CPU, working set,
and thread count at one-second intervals. This sampling reuses the runner's existing
process polling and does not enable solver hotspot instrumentation. Use CPU deltas
between samples; do not compare hotspot-on timings with ordinary timing jobs.
The sequential hard-profile wrapper expects fixed-manifest variants named `basis`
and `reference` when it feeds them into a whole before/after phase. Preserve a path
alias failure and relaunch into a new directory; never rewrite its failure marker.

Named profiler positional arguments are:

```text
timeout-seconds workers max-nodes engine stage output-json all|optimal on|off
```

`profile_case` additionally takes a case JSON path as argument nine.
The legacy named-case ladder remains available for full matrices and tiny cases.
The launcher can also alternate preserved/current `profile_witness` binaries over
a saved validated witness, verify equal keys/counts and test interrupted replay.

## Verification and interpretation

Reference jobs must complete and preserve exact results. `--allow-incomplete`
accepts only correctly classified stress caps, never watchdog kills. It checks
deadline cancellation or exhaustion through the requested N, valid incumbents,
partial key sets against available complete references, and proof consistency.

A completed preserved `before` variant can supply the exact-result reference
without baseline scheduling, for example p1 before versus p1 after. This does not
create a baseline-stage timing ratio. Matched timing cohorts still require the
same stage/settings; reference selection never permits incomplete reference jobs.
When fixing a verifier, retain its original failure and frozen script, reproduce
the failure, then write corrected summaries separately. Never replace raw results.

```powershell
python scripts/analyze-parallelism.py C:/absolute/path/to/RUN/results --allow-incomplete --output C:/absolute/path/to/RUN/results/summary-rechecked.json
```

Check schedule identities and count, exact normalized inputs/capacity, node/time
budgets, mode, worker count, profiling flags, executable hashes, keys, preferred
witness and full saved solution objects. Preserve original results and summaries.
The analyzer can summarize survivors but still fails a screen containing a kill.

Use fresh processes, randomized recorded order, equal caps and worker counts,
instrumentation off for timings, and repeats for a decision. Keep diagnostics
separate. Do not discard a warmup silently. Compare medians and ranges; do not
report speedups across instrumentation settings or from capped runs.

Record first validated witness, optimal/all completion, sampled process memory,
whole-process CPU and cancellation-to-return separately. Nested/overlapping timers
cannot be added as CPU. Per-worker cache counters omit allocator and task overhead.
Zero sampled memory or CPU on a tiny process can mean sampling/timer resolution,
not zero resource use. Partial layout counts need not match between timed-out runs.

## Evidence and validation

Raw results are local and ignored under `target/parallelism-ladder/`. Each experiment
names its directory, snapshots and important files. These Markdown records preserve
portable conclusions; obtain raw archives for independent rechecking on another machine.
The old compact analysis JSON/report still says push failed; publication was later
completed by the user. Historical artifacts are not rewritten to change that history.

Before experiment 08: 297 workspace Rust tests passed, five ignored; 22 runner/
analyzer tests, strict workspace Clippy, release build and formatting passed. See
experiment 08 for exact logs. After that run, isolated commit `92b111c` passed
194 solver-core library/integration/example tests, two ignored, strict solver-core
Clippy and 22 tooling tests in a separate target directory. See status (`mem:solver/status`)
for logs and scope. Run tests only after timing jobs have finished.
