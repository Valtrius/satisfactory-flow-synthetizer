# Satisfactory Flow Synthetizer

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Version](https://img.shields.io/badge/version-0.1.0-blue.svg)](package.json)
[![Platform](https://img.shields.io/badge/platform-Windows-0078D6.svg?logo=windows&logoColor=white)](#)
[![Tauri](https://img.shields.io/badge/Tauri-2-FFC131.svg?logo=tauri&logoColor=white)](https://tauri.app)
[![Rust](https://img.shields.io/badge/Rust-orange.svg?logo=rust&logoColor=white)](https://www.rust-lang.org)
[![Svelte](https://img.shields.io/badge/Svelte_5-FF3E00.svg?logo=svelte&logoColor=white)](https://svelte.dev)

Offline Tauri desktop app for exact Satisfactory splitter/merger flow synthesis. Set rates, pick a solver, and get validated belt layouts you can inspect, edit, and export as SVG.

<!-- TODO: replace docs/usage.gif with a real screenshot or demo GIF -->

![Usage demo](docs/usage.gif)

## Features

- Two solvers behind the same UI workflow: Custom (exact search) and Z3 (portfolio SMT)
- Queued jobs with history, cancellation, and minimum-node enumeration
- Problem solving history so you can revisit past jobs and results
- Topology graph with SVG export
- Rates as decimals or exact fractions
- Automatic supply on one belt; totals above capacity require explicit input belts

## Usage

### Installation / portable

Download a Windows build from this repository's GitHub Releases page. You can either run the installer, or use the portable executable with no install step.

To build from source instead, see [Getting started](#getting-started) under Developers.

### Usage

1. Follow the [installation / portable](#installation--portable) steps: install via the installer, or launch the portable executable.
2. Open the Constraints panel and choose **Custom** or **Z3** for the job.
3. Enter supply and demand rates (decimals or fractions).
4. Run the job. You can cancel, queue more work, and browse history.
5. Inspect the topology graph, edit if needed, and export SVG.

Every physical belt, including discard belts, must carry a positive flow that does not exceed the belt's capacity.

### Solvers

- **Custom** — deterministic exact topology search with proof accounting, independent validation, incremental incumbents, and complete minimum-node enumeration.
- **Z3** — portfolio SMT solver with parallel attempts, progress telemetry, feedback verification, cancellation, and minimum-node enumeration.

Both solvers optimize lexicographically by physical splitter/merger count, then by non-discard operator-to-operator link count. Custom proves that order with its link-group ledger. Z3 Opt finds any min-N layout, streams improving `best_known` incumbents under a strict belt cap, then proves nothing better exists. Returned witnesses pass an independent validator before they reach the UI.

### Result semantics

- `proven_optimal` — a completed Custom proof or a completed Z3 Opt run after the belt-cap improvement proof.
- `best_known` — an independently validated layout, not an optimality claim (including live Z3 Opt incumbents before the final proof).
- Both solvers report global UNSAT separately from incomplete/resource-limited work and internal failures.
- Cancelling enumeration keeps every layout already delivered. On successful completion, the preferred layout becomes `proven_optimal`; other minimum-node layouts stay validated `best_known` alternatives (the proof picks the preferred lexicographic witness). Cancelling Z3 Opt mid-improve keeps the best streamed incumbent as `best_known`.
- Common progress includes node bounds, current search size, exact-link obligations or link caps, and incumbent counts. Solver diagnostics are extensible name/value fields; unavailable common fields are null.

## Developers

### Requirements

- Node.js 20.19+ or 22.12+ (with a matching npm)
- Rust 1.85+ (workspace uses edition 2024)
- Visual Studio Build Tools 2022 with the **Desktop development with C++** workload (MSVC), plus WebView2 — required by Tauri and the bundled Z3 C++ build

### Getting started

```powershell
npm install
npm run dev
```

Benchmarks from a development build are not representative. Use a release build (`npm run build`) for timing comparisons.

`npm run build` creates the Tauri application bundle. The frontend alone can be built with `npm run build -w frontend`.

### Project layout

- `frontend` — Svelte 5 UI, queue/history model, solver telemetry, topology graph, edit history, and export tools
- `src-tauri` — desktop lifecycle, IPC job snapshots, engine dispatch, cancellation, and SQLite history
- `solver-z3` — Z3 implementation
- `solver-core` — Custom exact search, proof ledger, SCC analysis, and parallel coordinator
- `solver-api` — public problem, result, proof, validation, and progress types
- `solver-validation` — independent exact validation firewall
- `solver-reference` — simple exhaustive differential oracle for small cases
- `synthetizer-app` — shared solver dispatch and graph presentation for both production engines

### Shared solver API

All three engines accept `solver_api::Problem` and return `Result<SolveOutcome, SolverError>` through `solve_problem`:

```rust,ignore
let prepared = problem_request.prepare()?; // solver_api::ProblemRequest
let options = solver_api::RunOptions::default();
let cancel = std::sync::atomic::AtomicBool::new(false);
let outcome = solver_core::solve_problem(&prepared.problem, &options, &cancel, &|event| {
    // solver_api::SolverEvent: Progress, Incumbent, or SolutionFound
})?;
// solver_z3::solve_problem has the same signature.
// Reference deliberately has no observer:
let reference = solver_reference::solve_problem(&prepared.problem, &options, &cancel)?;
```

- **Problem:** request preparation parses decimals/fractions exactly and preserves terminal metadata outside the mathematical problem. Empty inputs produce exactly one input at the output sum. If that sum exceeds belt capacity, preparation fails with a message asking for explicit inputs; it never splits supply automatically.
- **Progress:** Custom and Z3 emit `SolverProgress` snapshots with common optional facts and `custom: Vec<Diagnostic>`. Diagnostics carry a stable name, label, typed value, and optional unit. Integer diagnostic values are decimal strings so JavaScript cannot truncate large counters. `LinkConstraint::Exact` and `AtMost` distinguish Custom obligations from Z3 optimization caps. Reference does not emit progress.
- **Solution:** `SolveOutcome` contains the mathematical result, validated physical witnesses, common optimality facts, and explicit enumeration completion. `Optimal` proves minimum N and minimum L at that N. `AllAtMinimumNodes` retains every distinct layout at minimum N, including different L values. An interrupted run keeps its witnesses without claiming the unfinished proof. Opt incumbents never enter the enumeration list.

`RunOptions` separates mode, worker count, and an optional inclusive node bound from the problem. Exhausting a bound is incomplete, not global UNSAT. Reference remains an exhaustive small-case oracle and uses a default bound of four nodes when none is supplied; its search and deduplication remain independent. Public solution keys use the same exact layout encoding across engines, after search has finished.

`synthetizer-app::presentation` is the single display projection. Tauri owns job lifecycle, sequenced snapshots, cancellation, and persistence; solver crates do not construct UI graphs. History preserves final progress and proof facts. Existing saved graphs remain readable; obsolete engine-specific progress is not reinterpreted as common progress.

Request preparation also rejects individual input or output rates above capacity, naming the offending terminal. Raw `Problem` callers still receive a finite global UNSAT proof for capacity contradictions. The application display wrapper keeps layout identity bytes in process, outside IPC and history payloads. Profile bars describe only the reported current group or search, never overall solve completion.

### Development

```powershell
npm test
npm run check
npm run build -w frontend
cargo test --workspace
cargo check -p satisfactory-flow-synthetizer
```

### Local build optimization

`.cargo/config.toml.example` documents optional machine-specific Z3 compiler flags. The local `.cargo/config.toml` is gitignored; do not commit machine-specific flags for someone else's build.

### Contributing

- Use [Conventional Commits](https://www.conventionalcommits.org/): `type(optional-scope): summary` (for example `feat(ui): …`, `fix(solver): …`, `docs: …`).
- PR titles must follow the same Conventional Commits format (this repo uses [git-cliff](https://git-cliff.org/) for changelogs).
- Common types used here: `feat`, `fix`, `refactor`, `perf`, `chore`, `docs`, `test`.
- Keep commits focused; prefer small PRs over large mixed changes.
- Run the Development checks above before opening a PR (`npm test`, `npm run check`, `cargo test --workspace` as relevant).
- Do not commit machine-specific files such as a local `.cargo/config.toml`, secrets, or build artifacts.

## License

This project is licensed under the [MIT License](LICENSE).
