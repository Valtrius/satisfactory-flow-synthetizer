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
- Automatic supply split across as many capacity-safe input belts as needed

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

- **Custom** — deterministic exact topology search with proof accounting, independent validation, incremental incumbents, complete minimum-node enumeration, and an optional verified SQLite component accelerator.
- **Z3** — portfolio SMT solver with parallel attempts, progress telemetry, feedback verification, cancellation, and minimum-node enumeration.

Both solvers optimize lexicographically by physical splitter/merger count, then by non-discard operator-to-operator link count. Custom proves that order with its link-group ledger. Z3 Opt finds any min-N layout, streams improving `best_known` incumbents under a strict belt cap, then proves nothing better exists. Returned witnesses pass an independent validator before they reach the UI.

### Result semantics

- `proven_optimal` — a completed Custom proof or a completed Z3 Opt run after the belt-cap improvement proof.
- `best_known` — an independently validated layout, not an optimality claim (including live Z3 Opt incumbents before the final proof).
- Custom reports global UNSAT separately from incomplete/resource-limited work and internal failures.
- Cancelling enumeration keeps every layout already delivered. On successful completion, the preferred Custom layout becomes `proven_optimal`; other minimum-node layouts stay validated `best_known` alternatives (the proof picks the preferred lexicographic witness). Cancelling Z3 Opt mid-improve keeps the best streamed incumbent as `best_known`.
- Engine-specific telemetry appears only when it has a real counterpart. Custom does not invent Z3 portfolio slots or peak-throughput metrics.

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
- `solver-core` — Custom exact search, proof ledger, SCC/component machinery, and parallel coordinator
- `solver-api` — public problem, result, proof, validation, and progress types
- `solver-validation` — independent exact validation firewall
- `solver-reference` — simple exhaustive differential oracle for small cases
- `solver-db` — optional verified SQLite component persistence and background prewarming
- `custom-solver-adapter` (under `crates/synthetizer-app`) — request preparation and graph presentation for Custom

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
