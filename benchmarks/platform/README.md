# Native and browser solve timing

The [September 12 result](results-20260912.md) is the first native/browser
comparison. Raw evidence stays in ignored `target/` directories, located through
[evidence.json](../evidence.json). Preserve completed directories when preparing
later experiments.

Preparation currently requires Windows, PowerShell 7, Python 3.12+, Node 24,
installed Chrome, the repository's Rust toolchain and `wasm32-unknown-unknown`
target. Run `npm ci`, `npm run prepare:cvc5`, and `npm run build:web:assets` first
if their dependencies or cached backend/binding tools are missing. Preparation
builds both native runners and current Rust Wasm from frozen source snapshots;
it reuses the validated cvc5 Wasm distribution.

Prepare without running solves:

```powershell
python scripts/prepare-platform-benchmarks.py --output target/platform-benchmarks-YYYYMMDD-HHMMSS
```

Launch the prepared directory, with durable progress, an independent three-hour
watchdog, and a Windows completion dialog:

```powershell
./scripts/start-platform-benchmarks.ps1 -PreparedDirectory target/platform-benchmarks-YYYYMMDD-HHMMSS
```

The suite compares the pre-Wasm 1.0.0 Git tree (`0b60d36`), a frozen copy of the
current native code, and the current production browser jobs/workers. Both native
runners call the same production solve API used by Tauri and use the same bundled
cvc5 1.3.4 binary. The browser uses its pinned cvc5 Wasm build in installed Chrome.
The release profile and Vite production bundler build all measured artifacts.

For a later comparison, pass `--baseline <commit>` and optionally
`--suite <path-to-json>`. A custom suite uses the same schema as `suite.json`;
case names resolve to `benchmarks/cases/<name>.json`. Keep a quick known-complete
case first: its initial triple must complete and agree before the rest starts.
Use even repeat counts, at least eight workers, and a session budget no greater
than 10,800 seconds. Preparation rejects invalid settings before building.
Every preparation uses a new directory; a failed preparation is preserved and
cannot be resumed with a different source tree. The native build cache is reused.

`suite.json` schedules 66 runs in matched triples with two repetitions in reversed
order. Five scopes run at 8 and 16 workers; the long case 97 runs at 16 workers with
a 600-second cap. A fresh browser process/context or native process starts each
run. There are no warm-up solves or concurrent solver runs. CPU affinity and the
power plan are recorded without changing machine policy. Other applications can
still affect timing.

Native wall time includes backend startup and worker cleanup, through the
production solve API return. Browser wall time starts at job admission and ends
at the sealed terminal snapshot, including worker/module startup, accepted
IndexedDB collection writes and worker termination. Browser launch and navigation
are outside timing. Tauri IPC/UI, graph rendering, SQLite history and full browser
history metadata persistence are outside these measurements. These are solve-path
comparisons, not click-to-render application benchmarks.

The frozen Rust verifier independently validates every retained graph and converts
both native and browser results to canonical graphs with exact flows. Complete
enumeration sets and optimum proof projections must agree. One min N/L allows any
validated witness with the same proven objective. Verification occurs after
timing. The first tiny-case triple gates the remaining suite. Root diagnostic
tracing is disabled. This suite does not replace the existing independent
proof-owner correctness tests.

`BENCHMARK-STATUS.txt` tracks progress. `REPORT.md` and `summary.json` are refreshed
after every run; `results/` preserves raw outputs, verification and failures.
`metadata.json`, `topology.json`, `schedule.json`, `sources/`, `bin/`, `web/` and
`frozen-hashes.json` identify all inputs. Installed Chrome is fingerprinted and
checked at launch. The copied Playwright runtime is frozen with the suite.

A timeout is censored, never a solve-time denominator. A group with a missing,
failed, capped or unequal result has no aggregate timing ratio. Two repetitions
are screening evidence; a native regression signal requires both pairs to be
over 10% and 0.5 seconds slower. Absence of a signal does not prove every input
is unchanged. Session admission reserves time for cleanup and verification and
can leave an explicit `remaining-jobs.json` if the deadline is near.

The two native variants use the selected reference and current source, while the
browser variant uses current source. This harness does not directly pair two
browser revisions or measure warm worker reuse. Do not pool browser timings from
different campaigns as though they were adjacent paired runs. Add that comparison
explicitly if a future browser optimization needs promotion evidence.

Revalidate a saved campaign without running search or overwriting its reports:

```powershell
node scripts/run-platform-benchmarks.mjs --verify target/platform-benchmarks-YYYYMMDD-HHMMSS
```

This checks frozen hashes, raw/record agreement, every successful record with the
frozen graph verifier, and exact reproduction of the saved summary using its
original analyzer. Capped records retain their incomplete proof state. The
verifier checks concrete graphs and proof-state consistency; it does not replay
an exhaustive search or SMT proof certificate. Failures remain visible in the
verification output. The verifier does not require the original Chrome install.

Harness checks:

```powershell
node --test scripts/test-platform-benchmarks.mjs
python -m unittest discover -s scripts -p test_platform_benchmarks.py
node scripts/run-platform-benchmarks.mjs --watchdog-test .
```

The watchdog check launches only a synthetic child and descendant. Preparation,
verification and harness checks do not launch benchmark solves. A timing launch
requires a new user request; after launching, provide the status location and end
the agent turn.
