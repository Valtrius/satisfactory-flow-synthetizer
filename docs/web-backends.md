# Browser architecture and development

The shared Svelte interface runs all three exact result scopes locally in a browser or through the native desktop host. Browser computation uses independent workers and IndexedDB history. Selected-solution links contain their own data and need no storage service. Offline builds cache the complete runtime. Closing a tab stops computation; reopening restores saved results as incomplete without resuming a search.

The [milestone archive](web-backend-history.md) records the initial implementation and dated validation. It is historical evidence. The [release guide](web-release.md) covers publication artifacts and the manual GitHub Pages workflow.

## Build and run

Use the repository's Rust and Node development tools, Python 3.12 or newer, and the `wasm32-unknown-unknown` Rust target. Building cvc5 requires Linux x64 with a C++ compiler, make, git and Python virtual-environment support. On Windows the checked-in builder uses Ubuntu WSL. It installs pinned tools in a project-specific user cache without changing shell startup files.

```powershell
npm ci
rustup target add wasm32-unknown-unknown
npm run dev:web
```

`dev:web` and `build:web` build the production verifier and solver Wasm modules, then verify and reuse the cached cvc5 distribution. Missing or stale cvc5 outputs invoke the pinned source builder. Its first build downloads the toolchain and compiles with two jobs. The bare `npm run dev -w frontend` does not prepare missing Wasm assets.

| Command                                    | Purpose                                                          |
| ------------------------------------------ | ---------------------------------------------------------------- |
| `npm run build:web`                        | Development static build in `frontend/dist`                      |
| `npm run build:web:release`                | Publication build with corresponding source and relink downloads |
| `npm run build:web:verifier`               | Production selected-solution verifier, independently of cvc5     |
| `npm run build:web:verifier:qualification` | Separate low-level verification bridge for backend tests         |
| `npm run build:web:portable`               | Test-only portable planner/identity module                       |
| `npm run build:web:backends`               | Both verifier distributions and the pinned cvc5 source build     |

Generated modules stay in `target/web-backends`. `web/cvc5/toolchain.json` pins cvc5, Emscripten, CMake and upstream dependency hashes. The build record includes flags, source pins and artifact checksums. These identify the build inputs and outputs; they do not promise identical bytes across arbitrary hosts. For another WSL distribution, invoke the Linux builder through that distribution; it accepts `--cache /absolute/path` and `--link-only` for an existing matching configured build.

## Code and ownership

| Location                                     | Responsibility                                                                |
| -------------------------------------------- | ----------------------------------------------------------------------------- |
| `frontend/src/lib/platform`                  | Browser/desktop jobs, history, files and lifecycle adapters                   |
| `platform/browserJobs.ts`                    | Job admission, attempts, cancellation, worker termination and completion seal |
| `platform/browserComputePool.ts`             | Compute slots, dispatch, receipts and retirement                              |
| `platform/browserCheckpoint.ts`              | Checkpoint validation, durable graph batches and acknowledgement ordering     |
| `frontend/src/lib/solver/solve.worker.ts`    | Rust proof coordinator, without a cvc5 heap                                   |
| `frontend/src/lib/solver/leaf.worker.ts`     | One exact leaf at a time in its own Rust/cvc5 heap                            |
| `crates/solver-browser`                      | Production coordinator and leaf bridges using the portable exact solver       |
| `crates/synthetizer-app/src/jobs`            | Snapshot types/transitions and terminal/interruption projection               |
| `crates/synthetizer-app/src/presentation`    | Exact rate display, graph construction, feedback and outcome assembly         |
| `src-tauri/src/history`                      | SQLite facade, migrations/schema, reads, writes and conversion                |
| `frontend/src/lib/resultPages.svelte.ts`     | Selected collection page lifecycle                                            |
| `frontend/src/lib/sharing/session.svelte.ts` | Share previews, fragment navigation and explicit saves                        |
| `frontend/src/lib/graphSelection.ts`         | Prepare a selected graph before the graph session commits it                  |

The platform is selected once per page lifetime. Only its desktop implementation calls Tauri. Native jobs retain process/thread ownership, independent portfolio proof ledgers and SQLite persistence. A native branch uses a streaming planner: the outer collector owns accepted graphs while the branch retains its identity ledger.

## Exact computation and cancellation

`solver-core` supplies `ExactPlanner` and `LeafDriver` without native execution to Wasm hosts. Exact preparation, reconstruction, independent validation, canonical identity and result projection run in Rust workers. Rate literals remain arbitrary-precision strings until exact parsing.

Automatic uses the logical processor count reported by `navigator.hardwareConcurrency`. The selector offers lower powers of two and the exact reported count, remembers explicit choices, and falls back to one when the hardware report is invalid. The coordinator is additional to the compute budget. Compute workers are created only for dispatched work and reuse initialized modules between disposed leaf sessions. Each new worker has a 120-second startup limit; readiness removes that limit and there is no production solve deadline.

One compute worker uses Boolean search. Larger budgets split between independent Sparse and Boolean branches, with Sparse receiving the odd extra slot. Each branch retains its own proof ledger. One complete branch owns the final proof; incomplete branches cannot combine their counters into a proof. A failed independent branch can coexist with a healthy complete owner. Static second-output partitions and adaptive children retain the native root-coverage rules: a parent or a complete disjoint child cover owns an obligation, never both.

`one_min_nl` proves minimum N then operator-link L without claiming enumeration exhaustion. `all_min_nl` exhausts that optimum collection. `all_min_n` includes higher-L layouts at minimum N. L excludes external terminal stubs and discard belts. Imported shares remain proofless.

Each leaf executes complete internal SMT batches in one incremental cvc5 session. The session wrapper preserves declarations, assertions, push/pop and exact models across calls. It resets parser input after EOF while retaining the solver. Only typed `sat` and `unsat` results advance proof; `unknown`, invalid models, traps and disposal failures cannot discharge an obligation. Shared data never becomes an arbitrary-SMT endpoint.

The page validates protocol/job/attempt identities, monotonic checkpoints, contiguous snapshot sequences and graph counts. A worker publishes accepted witnesses before its next blocking backend call. The page acknowledges only after storing a bounded graph batch. Cancellation terminates all active and idle workers directly, including a worker blocked inside synchronous cvc5. If cancellation arrives during a received write, the page retains that accepted batch and applies Rust's interruption projection. A disposal failure requires whole-worker termination.

Normal completion requires all dispatched backend work to retire. The registry terminates every owned worker and seals the job before notifying subscribers. Cancellation accepted before that seal wins; cancellation after it returns the sealed result. Accepted witnesses survive interruption as incomplete results. Existing minimum-N evidence may survive, while interruption cannot establish minimum L or enumeration exhaustion.

## Paged results and persistence

IndexedDB schema 2 separates history metadata, graph layouts, collection graphs and compact solution summaries. Each history operation batch is one transaction and resolves only on transaction completion. Writes compare the stored revision with the tab's last acknowledged revision. A stale tab receives an error and cannot overwrite a newer checkpoint; there is no multi-tab merge. Unsupported or corrupt schemas do not trigger deletion.

Jobs publish a preferred graph and an immutable collection-prefix reference. They do not copy the whole enumeration into snapshots. Graph batches are bounded to 16. A failed write retains a bounded tab-owned overlay for export or retry and ends the job incompletely. Terminal collection references remain usable until their history/job owners release them.

The collection page reader keeps summaries and a sorted source-index order for the latest viewed collection. Repeated pages reuse them; appends read and merge only the new suffix. Sort comparisons use exact rationals and stable source-index ties. Only requested graph pages or the selected graph are read. Summary and canonical-identity indexes still grow with collection size; paging bounds graph ownership, not every index.

Selection uses stable source indices across sorting and pages. The graph session commits the selected index, displayed solution and saved history only after both graph retrieval and layout preparation succeed. Read/layout failures preserve the existing selection and undo history. Selecting the displayed row again preserves edits and cancels another pending switch.

History writes coalesce for 400 ms with a five-second maximum checkpoint delay while timers run. Visibility/pagehide flushes supplement that process but cannot guarantee a final write when a tab closes. Queued work is not restored; running work becomes incomplete, retains saved witnesses and layouts, clears job IDs and never restarts automatically. Storage is origin-local, best-effort and subject to eviction. Export backups for data that must be retained. There is no desktop/browser synchronization.

## Selected-solution sharing and identity qualification

Sharing encodes one physical topology with its endpoint rates and belt capacity. The self-contained `#/share/v1/` link or JSON file omits graph coordinates, solve mode, proof claims and full result collections. The verifier reconstructs exact flows and checks the graph before previewing it. Previewing does not save history; an explicit save creates a proofless entry. Input-only sharing is not a supported format.

The production `solver-web` module exports the selected-solution codec and verifier. Its low-level `verify_witness_json` and `reconstruct_topology_json` Wasm exports are enabled only by the `qualification` feature in the separate `verifier-qualification` distribution. The underlying Rust APIs remain available for native fixtures. All untrusted verification runs in a terminable worker with payload, decompression, graph and literal-size bounds. The verifier is separate from cvc5 and can be built or executed independently.

The vendored Canonaut patch preserves 64-bit KISS state and seeds, while bitsets and index operations respect the target word width. Production disables unused entropy dependencies. The immutable `solver-portable-tests/fixtures/identity-v1.json` was captured before the port and compares 30 colored graphs plus 12 physical witnesses, with equivalent relabelings and word-boundary cases, on native Rust and actual Wasm. Never regenerate that baseline to hide a mismatch. `vendor/canonaut/PORTABILITY.md` records source provenance, changes and the curated package targets. `npm run test:vendor` runs the active bundled tests independently of workspace tests; absent full-corpus cases are excluded from this package.

## Asset delivery and updates

The verifier and solver use separate content-hashed local directories. `frontend/localAssets.ts` owns common distribution mechanics; solver-specific preparation checks cvc5 provenance and emits its protocol manifest. Desktop mode includes the small verifier and omits browser search and service-worker assets. Development execution loads Wasm only when needed. Production offline installation downloads the complete runtime, while execution remains demand-driven.

Before mounting the app, browser bootstrap waits for the initial complete service-worker installation and reloads to obtain that worker's document and assets together. An already controlled document checks its build ID against its controller before mounting. Every network cache insertion checks recorded byte count and SHA-256. The service worker never calls `skipWaiting` or `clients.claim`.

If initial installation fails, the app can run online. A later successful retry saves the files and asks the user to finish work and reopen; it does not attach a different build to a running document. Updates prepare the full candidate runtime and wait until every old app tab closes. Activation removes only old caches for the same scope and never clears IndexedDB. Source downloads stay outside runtime caching. Relative URLs, normal 404 responses and a local-only CSP support the GitHub project path without cross-origin isolation headers.

## Validation

Prepare the native backend for the fixture generators and choose installed Chrome or Playwright Chromium:

```powershell
npm run prepare:cvc5
$env:SOLVER_CVC5 = Join-Path (Get-Location) 'src-tauri/binaries/cvc5-x86_64-pc-windows-msvc.exe'
$env:WEB_TEST_CHANNEL = 'chrome'
npm run build:web
npm run build:web:verifier:qualification
npm run build:web:portable
npm run test:web:portable
npm run test:web:backends
npm run test:web:platform
npm run test:web:shares
npm run test:web:solver
npm run test:web:delivery
npm run test:vendor
```

For Chromium, run `npx playwright install chromium` and omit `WEB_TEST_CHANNEL`. The portable suite prepares native/reference fixtures used by production solver tests. Tests serve local files under the project subpath without cross-origin isolation. Result reports are written beneath `target/web-*`.

Production solver tests compare 39 application-compatible cases at budgets of 1, 2, 4 and 32 workers, including full canonical collections, source indices and proof projection. Portable tests cover 42 lower-level requests and the pre-port identities. Separate suites exercise real IndexedDB transactions, quota/abort/conflict recovery, summary-read reuse, selected sharing, cancellation, offline integrity and multi-tab upgrades. Synthetic large worker counts and storage fixtures test bounds and ownership; they are not performance measurements.

Run the common frontend, native, tooling and lint checks in [Development](development.md) too. Local validation does not establish remote CI, public deployment, Firefox/Safari/mobile qualification or packaged WebView2 behavior.
