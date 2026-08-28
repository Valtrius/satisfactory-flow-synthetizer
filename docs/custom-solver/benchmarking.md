# Benchmark guide

## Cases and modes

All new file cases are in `benchmarks/custom/cases/` and use max link rate 1200.
Names describe the test corpus only, not information passed to solver policy.

| Label          | Exact problem     | Role and last reviewed scope                                |
| -------------- | ----------------- | ----------------------------------------------------------- |
| `acyclic24`    | 24 = 7+6+5+4+2    | Complete optimal/all regression, N<=7                       |
| `cyclic65`     | 65 = 40+25        | Complete optimal/all regression, N<=6                       |
| `acyclic36`    | 36 = 11+9+7+5+3+1 | Difficult optimal/all, N<=9; known optimum N=9, L=11        |
| `cyclic10`     | 10 = 6.04+3.96    | Difficult optimal, N<=11; starting proven lower bound is 11 |
| `profile_tiny` | 2+3 = 1+4         | N<=2, fixed scheduler overhead                              |

The old tiny records used capacity 5. The newer example uses 1200. Do not silently
merge them. The user originally reported roughly 2.5/30 s for 24 optimal/all and
1.3/31 s for 65; hard 36 optimal about 6m30s, all stopped after 30m with eight
found; hard 10 optimal stopped after 25m at N=11/L=18. These are user observations,
not controlled solver measurements or complete result-set references.

`optimal` calls the real single-result API; `all` calls full enumeration. A node
cap limits the largest N, not the starting N. The N<=10 cyclic10 attempt was below
its lower bound and did no search. Before SAT, duplicate all-mode runs can repeat
the same proof work without exercising remaining-group concurrency.

## Running a screen

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

Before the current run: 297 workspace Rust tests passed, five ignored; 22 runner/
analyzer tests, strict workspace Clippy, release build and formatting passed. See
experiment 08 for exact logs. Run tests only after timing jobs have finished.
