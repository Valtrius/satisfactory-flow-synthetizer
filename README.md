# Satisfactory Flow Synthetizer

An offline Tauri application for exact Satisfactory splitter/merger flow synthesis. The desktop UI can run either solver through the same queued job, history, cancellation, enumeration, graph-editing, and SVG-export workflow:

- **Custom** — deterministic exact topology search with proof accounting, independent validation, incremental incumbents, complete minimum-node enumeration, and an optional verified SQLite component accelerator.
- **Z3** — the optimized portfolio SMT solver imported from `satisfactory-load-balancer`, including its parallel attempts, progress telemetry, feedback verification, cancellation, and minimum-node enumeration.

The engine is selected per job in the Constraints panel. Rates accept decimals or exact fractions. Automatic supply is split across as many capacity-safe input belts as required.

## Development

Requirements: current Node.js/npm, Rust, and the native Windows build tools used by Tauri and bundled Z3.

```powershell
npm install
npm run dev
```

Useful verification commands:

```powershell
npm test
npm run check
npm run build -w frontend
cargo test --workspace
cargo check -p satisfactory-flow-synthetizer
```

`npm run build` creates the Tauri application bundle. The frontend alone can be built with `npm run build -w frontend`.

## Workspace

- `frontend` — the complete Svelte 5 UI, queue/history model, solver telemetry, topology graph, edit history, and export tools.
- `src-tauri` — desktop lifecycle, IPC job snapshots, engine dispatch, cancellation, and final-schema SQLite history.
- `solver-z3` — optimized Z3 implementation.
- `solver-core` — Custom exact search, proof ledger, SCC/component machinery, and parallel coordinator.
- `solver-api` — exact public problem, result, proof, validation, and progress types.
- `solver-validation` — independent exact validation firewall.
- `solver-reference` — deliberately simple exhaustive differential oracle for small cases.
- `solver-db` — optional verified SQLite component persistence and background prewarming.
- `custom-solver-adapter` (under `crates/synthetizer-app`) — application request preparation and graph presentation for Custom.

## Result semantics

- `proven_optimal` carries a completed Custom proof or the Z3 solver's completed optimum.
- `best_known` is an independently validated witness, not an optimality claim.
- Custom distinguishes global UNSAT from incomplete/resource-limited work and internal failures.
- Cancelling enumeration retains every layout already delivered. On successful completion, the preferred Custom layout is upgraded to `proven_optimal`; other minimum-node layouts remain validated `best_known` alternatives because the proof identifies the preferred lexicographic witness.
- Engine-specific telemetry is shown only when it has a truthful counterpart. Custom does not fabricate Z3 portfolio slots or peak-throughput metrics.

Every physical belt, including discard belts, must have exact flow `0 < f <= B`. Custom optimizes lexicographically by physical splitter/merger count and then non-discard link count; its returned witness passes the independent validator before it reaches the UI.

## Local build optimization

`.cargo/config.toml.example` documents optional machine-specific Z3 compiler flags. The authorized local `.cargo/config.toml` is gitignored; do not commit it on behalf of another build machine.
