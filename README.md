# Satisfactory Flow Synthetizer

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Version](https://img.shields.io/badge/version-0.2.0-blue.svg)](package.json)
[![Platform](https://img.shields.io/badge/platform-Windows-0078D6.svg?logo=windows&logoColor=white)](#)
[![Tauri](https://img.shields.io/badge/Tauri-2-FFC131.svg?logo=tauri&logoColor=white)](https://tauri.app)
[![Rust](https://img.shields.io/badge/Rust-orange.svg?logo=rust&logoColor=white)](https://www.rust-lang.org)
[![Svelte](https://img.shields.io/badge/Svelte_5-FF3E00.svg?logo=svelte&logoColor=white)](https://svelte.dev)

Offline Windows desktop app for exact Satisfactory splitter/merger flow synthesis. Set supply and demand, choose a result scope, and inspect, edit or export validated belt layouts.

## Use

The Windows x64 installer and portable ZIP include cvc5 1.3.4 and its license notices. Install the app, or extract the entire portable ZIP and run `satisfactory-flow-synthetizer.exe`, keeping `cvc5.exe` beside it. No separate cvc5 installation is needed. The app solves offline. Microsoft Edge WebView2 is required; the installer can install that runtime if it is missing.

`SOLVER_CVC5` can override the bundled executable for development. Without an override, the app checks beside its executable first, then PATH and `%LOCALAPPDATA%/Programs/cvc5/bin/cvc5.exe`.

Enter rates as decimals or fractions and set the maximum belt rate. Automatic supply uses one belt; totals above capacity require explicit input belts. Every physical belt, including discard, has strictly positive flow within capacity.

| Result scope | Meaning                                                                   |
| ------------ | ------------------------------------------------------------------------- |
| One min N/L  | One exact layout after proving minimum nodes, then minimum operator belts |
| All min N/L  | Every distinct layout at those minimum N and L values                     |
| All min N    | Every distinct layout at minimum N across all feasible L values           |

N counts splitters and mergers. L counts operator-to-operator belts, excluding external stubs and discard belts. Equal optimal ties and delivery order can vary. All scopes support queuing and cancellation.

`best_known` is a validated incumbent. `proven_optimal` requires a completed objective proof. Enumeration completion is separate: cancelling keeps already delivered layouts and incomplete proofs remain incomplete. Finite impossibility proofs are distinct from timeouts, resource limits and failures.

History stores requests, results, proofs and graph edits without a solver type. Imported entries migrate on load. The history card shows proved minimum L; an unknown minimum is shown as L=—.

## Developers

Requirements: current stable Rust, Node.js 20.19+ or 22.12+, PowerShell 7, Visual Studio C++ Build Tools and WebView2. `npm run dev` and `npm run build` prepare the pinned cvc5 package automatically. The first preparation downloads the official archive; subsequent runs verify and reuse its cached copy. Binaries are generated locally, not stored in Git.

For direct Cargo commands, first run `npm run prepare:cvc5`. Use `SOLVER_CVC5` to point standalone solver tests at the prepared binary if cvc5 is not on PATH.

```powershell
npm ci
npm run dev
```

```powershell
npm run format
npm test
npm run check
cargo test --workspace --locked
cargo clippy --workspace --all-targets -- -D warnings
npm run build
npm run package:release
npm run verify:release
```

- `crates/solver-core`: Exact cvc5 search, sparse/Boolean portfolio, proof ledger and diagnostics.
- `crates/solver-reference`: independent exhaustive oracle for small differential tests.
- `crates/solver-validation`: independent exact validation and layout identity.
- `crates/solver-api`: problems, rational arithmetic, result scopes, proof and progress contracts.
- `crates/synthetizer-app`: production solve and graph presentation.
- `src-tauri`: desktop jobs, IPC, cancellation and SQLite history migrations.
- `frontend`: Svelte UI, queue, graph editing and SVG export.
- `benchmarks`: cases, experiment records and current smoke manifest.

`npm run package:release` writes the installer, portable ZIP and SHA256 files to `target/release/bundle/distribution`. `npm run verify:release` uses 7-Zip to inspect both packages and checks an exact solve with external backend lookup disabled. The pin, download URL and archive checksum live in `src-tauri/cvc5-package.json`; bundled notices are under `licenses/cvc5`.

Project contracts and experiments live in the [Serena memory index](.serena/memories/index.md). The [benchmark guide](benchmarks/README.md) explains the current runner and recorded artifacts. Use release builds for performance comparisons.

### Contributing

- Use [Conventional Commits](https://www.conventionalcommits.org/): `type(optional-scope): summary` (for example `feat(ui): …`, `fix(solver): …`, `docs: …`).
- PR titles must follow the same Conventional Commits format (this repo uses [git-cliff](https://git-cliff.org/) for changelogs).
- Common types used here: `feat`, `fix`, `refactor`, `perf`, `chore`, `docs`, `test`.
- Keep commits focused; prefer small PRs over large mixed changes.
- Run the Development checks above before opening a PR (`npm test`, `npm run check`, `cargo test --workspace` as relevant).
- Do not commit machine-specific files such as a local `.cargo/config.toml`, secrets, or build artifacts.

## License

This project is licensed under the [MIT License](LICENSE).
