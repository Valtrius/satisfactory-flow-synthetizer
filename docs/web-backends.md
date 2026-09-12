# Browser backend foundations and platform services

The shared Svelte application runs exact searches locally through independent browser workers or the existing native desktop host. Step 05 adds bounded parallel scheduling, independent portfolio proof ownership and IndexedDB-backed result pages. Step 06 adds integrity-checked offline delivery, version-coherent upgrades, source/relink packaging and a manual GitHub Pages workflow. All three search scopes remain available. Selected-solution links are self-contained and need no backend storage. No public Pages deployment is claimed by these local changes. Native threads, cvc5 process ownership, portfolio scheduling and SQLite storage remain in the desktop host.

## Agreed product scope

The browser release must support exact single-solution optimization and complete minimum-N,L enumeration. Keep all minimum-N enumeration in the implementation plan too. A viewer-only build does not meet the browser release requirements.

Use one Svelte interface with host-specific adapters where needed. Sharing publishes one selected physical solution with its input/output rates, belt capacity and topology. It does not initially include graph coordinates, solve mode or the full result collection. Do not add input-only sharing. Generate the default display from the shared topology.

Long, self-contained selected-solution links and browser-local history are required. Sharing needs no backend storage. Offline use must cover locally available data and solving after the required assets have been cached. Asset caching is not implemented yet.

Tab closure stops browser computation. Exact search resumption and initial desktop/browser history synchronization are out of scope.

## Build and run

Requires the existing Rust and Node development tools, Python 3.12 or newer, and a Linux x64 build environment. On Windows the cvc5 build command uses the Ubuntu WSL distribution. Linux needs a host C++ compiler, make, git and Python virtual-environment support. The build installs pinned tools in a project-specific user cache, without changing shell startup files.

```powershell
rustup target add wasm32-unknown-unknown
npm ci
npm run build:web:backends
npx playwright install chromium
npm run test:web:backends
```

The first cvc5 build downloads the toolchain and compiles its libraries with two build jobs. Subsequent builds reuse them. The browser tests load only local files from a loopback HTTP server. They use the project subpath and do not use cross-origin isolation headers.

To use an installed Chrome on Windows:

```powershell
$env:WEB_TEST_CHANNEL='chrome'; npm run test:web:backends
```

To run the verifier independently of the cvc5 source build:

```powershell
npm run build:web:verifier
npm run test:web:backends -- --grep "exact Rust"
```

For another WSL distribution or a different Linux cache location, invoke the Python script through that distribution. The Linux script accepts `--cache /absolute/path` and `--link-only`. The latter requires an existing matching configured build.

## Exact Rust verification

`solver-web` builds as `wasm32-unknown-unknown` and as a native library for cross-target comparisons. Its dependencies deliberately disable `solver-validation/identity` and `synthetizer-app/native-runtime`. Native callers retain those default features.

The bridge exports `verify_witness_json` and `reconstruct_topology_json`. Both accept a JSON envelope containing `request` and `graph`. The first requires exact supplied link flows and rejects incorrect ones. The second accepts topology links without flows and reconstructs their unique exact values. Both call the existing independent validator and presentation code.

Rates remain strings until bounded parsing into arbitrary-precision rationals. The bridge checks payload size, node/link counts and rate literal lengths before the expensive operations. It returns either `verified` with a physical graph and display data, or `rejected` with an error. All verified displays remain `best_known` with no optimality proof. Validity does not establish an optimum.

This is a verification bridge, not the final public share schema. It neither trusts nor exports a canonical identity. The empty internal key passed to the presenter is unused by that projection and never enters a collection or proof ledger. A production caller must execute verification in a terminable worker, including when opening untrusted data.

The fixture generator executes the same inputs natively and in a browser worker. Fixtures include exact fractions, large rationals, splitters, mergers, feedback, anonymous discard, invalid topology and malformed data. The browser test also checks that verification does not download cvc5.

## Incremental cvc5 sessions

`web/cvc5/session.cpp` owns a term manager, solver, symbol manager and incremental parser per session. The Emscripten module exposes create, execute, read-output and destroy functions. It does not emulate standard input or invoke the cvc5 CLI repeatedly.

`session.mjs` copies each command batch onto the Wasm heap, invokes the synchronous API and copies the response before freeing the input. This avoids putting large encodings on the Wasm stack. Parsing and API failures invalidate that session. The owner must dispose it. Invalid sessions do not yield UNSAT or a completed search.

Each batch must contain complete SMT commands. The wrapper explicitly resets the parser input after EOF while keeping the solver and symbol manager alive. This avoids a cvc5 1.3.4 parser-API bug where automatic EOF reinitialization clears the incremental-input flag. The multi-call browser test caught that failure. No upstream source patch or repeated solver startup is needed.

For `check-sat`, the wrapper reads the typed cvc5 result and emits exactly `sat`, `unsat` or `unknown`. The default library stream can append a `RESOURCEOUT` annotation, which is not the wire verdict. The reason remains available through `get-info :reason-unknown`.

Command batches are internal solver commands, not a feature for executing SMT text from shared links. The wrapper accepts a restricted command set and does not expose file input, exit commands or arbitrary option changes. The per-session resource limit exists to test `unknown` handling. It is not a browser deadline implementation.

Run each module in a dedicated worker with its own memory. A blocked synchronous SMT call cannot process a cancellation message. The owner terminates that worker and creates a fresh one. Discarding that session is not proof that any search obligation was exhausted. The shared scheduler must preserve already delivered, independently verified solutions when it implements this lifecycle.

The integration tests exercise persistent declarations and assertions, exact models, model blocking to exhaustion, push/pop, two isolated sessions, parse and command errors, resource-limit `unknown`, disposal and replacement. A separate test terminates an active SMT query while the page's event loop remains responsive, then solves in a replacement worker.

## Build records and distribution

`web/cvc5/toolchain.json` pins cvc5 1.3.4, Emscripten 3.1.70, CMake 3.31.6 and the upstream dependency archives. The builder checks source SHA-256 hashes and verifies the dependency URLs/hashes selected by the upstream CMake build. Both library compilation and module linking enable exception handling.

Generated files live under `target/web-backends`. The cvc5 output contains `build.json`, copied source license notices, the module and `session.o`. The build record records source pins, compiler version, flags and output checksums. Source and static-library build directories stay in the user cache. This establishes repeatable build inputs, not a claim of byte-identical output across all host environments.

The proof build compiles upstream C++ at `-O3` and links the final module at `-O1`. Whole-module `-O3` optimization exceeded nine minutes in the initial local attempt and was stopped. Browser runtime performance and final download-size tuning are separate work, not claims made by this backend proof.

These outputs are development artifacts. Before publishing the browser application, complete the distribution package, including the corresponding source and relinking material for statically linked dependencies. Copying notices alone is not the release gate. The native Windows cvc5 package is separate and unchanged.

## Step 00 validation on 11 September 2026

The first backend proof gate passed on Windows with Chrome, with the cvc5 module built in Ubuntu WSL. All four browser tests passed, including 28 native/Wasm verification fixtures, persistent session isolation, coexistence of both Wasm modules and termination/recreation during a blocked query. The session test enumerated two exact models to UNSAT, returned `unknown` with `resourceout` on a resource-limited query, and disposed/recreated 16 sessions.

The full native workspace passed 155 tests. The frontend passed 150 tests, and Python tooling passed 40 tests. Native workspace Clippy, verifier wasm32 Clippy, Svelte checks, formatting, release metadata and the frontend production build also passed. `npm run build:web:backends` completed using the checked-in Windows-to-WSL build path.

The verifier Wasm file is 484,367 bytes, or 170,188 bytes under local gzip compression. The proof cvc5 Wasm file is 32,908,722 bytes, or 6,454,698 gzip bytes. These exclude JavaScript and other assets. Compression sizes are file measurements, not measurements of HTTP delivery, browser memory or search performance.

Browser evidence is written to `target/web-backends/browser-results.json`; source pins and cvc5 artifact checksums are in `target/web-backends/cvc5/build.json`. The added GitHub workflow has not run remotely. Firefox, Safari, mobile behavior and full browser search parity have not been qualified in this phase.

## Step 00 canonicalization audit

The unpatched registry `canonaut` 1.0.0 dependency initially failed the wasm32 build in its default `rand`/`getrandom` dependency. In a detached source probe with only seeded `SmallRng` enabled, wasm32 compilation reached the Canonaut code and reported eleven overflowing `usize` literals. These were in `src/rng.rs` and `src/utilities.rs`.

The audit also found hardcoded 64-bit operations in `src/structs/schreier_arena.rs`, including division/remainders by 64 on `usize` bitsets, plus shifts by 58 and 43 in the RNG. Later inspection also found word-size assumptions in the one-word and two-word refinement and permutation routines. Step 00 did not modify the dependency. Step 03 below records the portability patch and its qualification.

The identity acceptance gate compares pre-port native bytes against the patched native implementation and actual Wasm execution. It covers word boundaries, relabeled graphs, duplicate terminals and physical `layout-v1` identities. Compiling the dependency alone does not satisfy this gate.

Step 01 extracts platform services and implements browser history. Step 02 implements selected-solution sharing. Step 03 separates mathematical transitions from native execution. Step 04 records the first production browser integration; Step 05 replaces its single-worker transport with parallel scheduling and paged collections. Step 06 adds offline and publication delivery. The standalone backend tests do not replace application lifecycle tests.

## Step 01: platform services

`frontend/src/lib/platform/index.ts` selects one host for the page lifetime. `contracts.ts` defines jobs, history, file actions, lifecycle and capabilities. Existing public helpers such as `api.ts`, `historyPersist.ts` and `historyIo.ts` delegate to this host rather than making their own Tauri decisions. Only the platform implementation imports Tauri APIs in production frontend source.

The desktop adapter retains the existing command names and snapshot event. Watching subscribes before fetching the full snapshot; the queue still reconciles sequences and missing appends. Native close still hides the window, joins solver work, retries cleanup or returns to the app, and asks explicitly before exiting without saving. `resume_jobs` reopens native job admission. It does not resume a mathematical search.

At the Step 01 milestone, the browser adapter rejected job creation and disabled Find. Step 04 replaces that adapter with real local worker jobs. History import/export and SVG export use browser file actions. Cancelling import returns `null`; file read and JSON errors propagate instead of pretending the picker was cancelled.

### Shared job projection

`crates/synthetizer-app/src/jobs.rs` owns `SolveRequest`, `JobStatus`, `JobSnapshot<Id>` and pure snapshot projection. Native jobs use UUIDs; another host can use strings without changing the JSON fields. The host supplies start time and owns cancellation acceptance, locks, threads and the completion seal. Thin progress/append packets keep the existing sequence, omission flags and result counts without cloning the accumulated graphs.

`solution.rs` contains the portable presented solution type. `runtime::Solution` remains a re-export for existing native callers. Terminal projection preserves published indices and only reuses unproved cached displays. It consumes normalized canonical keys already returned by the solver API instead of recalculating them in the Tauri layer. Native identity generation and `layout-v1` bytes are unchanged. Projection tests compare the supplied terminal key to native canonicalization.

The shared code builds with `synthetizer-app --no-default-features` for wasm32. This proves the projection boundary is portable, not that Canonaut or the search scheduler has been ported. Public share payloads must not call this projection with unverified proof or canonical-key claims.

### Browser history storage

The database is `satisfactory-flow-synthetizer.history`, IndexedDB schema version 1. It has four stores:

| Store       | Contents                                                                     |
| ----------- | ---------------------------------------------------------------------------- |
| `meta`      | Schema, acknowledged revision, entry order and selection.                    |
| `entries`   | Entry fields, request/form, preferred result, sort columns and solver state. |
| `solutions` | One record per enumerated solution, keyed by entry ID and source index.      |
| `layouts`   | One saved graphical layout per entry ID and source-index key.                |

Each `HistoryOp` batch uses one transaction. Saving resolves only on transaction completion, never on the last request's success event. Failed writes leave both the committed records and the acknowledged revision unchanged. Append checks validate the expected count and contiguous zero-based indices. The shared persister retains its detached snapshots, coalesced writes and atomic append-prefix replacement. Growing collections append solution records instead of rewriting every solution in one opaque value.

Every write compares the stored revision against the revision that this tab last acknowledged, inside the same transaction. A stale tab gets a visible error telling the user to export unsaved work and reload. It cannot overwrite a newer checkpoint. This is conflict rejection, not multi-tab merging or synchronization. Even an automatic checkpoint in another tab can make a tab stale. The adapter closes connections on database version changes; unsupported versions, damaged metadata, blocked opens and storage failures do not trigger a destructive reset.

Queued entries remain unpersisted. Running and cancelling jobs become incomplete checkpoints, retain accepted solutions and layouts, clear their job IDs and never restart automatically. Selection/order and graph edits survive reload. The existing local history importer still handles legacy formats; it is not the future public-link validation boundary.

The shared persistence controller keeps its 400 ms debounce and five-second maximum wait while timers run. Browser visibility/pagehide flushes supplement those saves. They do not guarantee a final write when a tab or process closes. Storage remains best-effort and can be cleared or evicted by the browser. Export backups for data that must be retained. No service worker, asset caching or persistent-storage permission request is implemented yet.

Storage is origin-local. Different GitHub project paths on the same origin are not security boundaries. Moving the application to another origin does not migrate history automatically. There is no desktop/browser history synchronization.

### Run the browser development interface

```powershell
npm run dev -w frontend
```

Import an existing history JSON file, edit a title or graph, and reload after a checkpoint. Browsing history does not load cvc5. Use `npm run dev:web` to prepare all local assets before running searches; the bare frontend command does not build missing Wasm modules.

```powershell
$env:WEB_TEST_CHANNEL='chrome'; npm run test:web:platform
cargo check -p synthetizer-app --no-default-features --target wasm32-unknown-unknown --lib --locked -j 2
```

`web/platform/playwright.config.mjs` runs the shared Svelte app at `/satisfactory-flow-synthetizer/` on loopback port 4179, with one Chrome/Chromium test worker and no cross-origin isolation headers. The tests exercise real IndexedDB transactions, the native incremental HistoryOp fixture, cross-tab conflicts, fault-injected quota errors, late aborts, append-acknowledgement recovery, ordering, interrupted jobs and UI graph edits across reload. Browser tests install an IPC sentinel that fails if the browser calls Tauri.

Results are written to `target/web-platform/results.json`. The existing four backend tests remain separate on port 4178. The workflow also checks the shared wasm32 projection and browser history contracts; remote GitHub execution still requires pushing the branch and is not claimed here.

### Step 01 validation on 11 September 2026

The native workspace passed 158 tests, including the three moved projection tests and three new snapshot/terminal tests. Native cancellation sealing, relational history and full solver/reference contracts passed. The frontend passed 158 tests. All nine real-browser platform/history tests and the four existing backend tests passed in installed Chrome; the verifier still matched all 28 native fixtures. Python tooling passed 40 tests.

Native workspace Clippy, portable job-projection wasm32 Clippy and verifier wasm32 Clippy passed with warnings denied. The verifier rebuilt successfully, Svelte checks found no errors or warnings, and the production frontend build passed. Formatting and release metadata checks passed; the version remains 1.0.0. No performance campaign or installer packaging was run.

The graph interaction test emitted a non-failing `svelte-put/shortcut` configuration warning. Shortcut code is outside this extraction and unchanged. Firefox, WebKit/Safari, mobile storage pressure, public deployment and GitHub-hosted CI execution are not qualified by these local Chrome checks.

## Step 02: selected-solution sharing and verified viewing

The shared interface now has Open shared solution and a Share selected solution action in the graph toolbar. The toolbar uses the currently displayed layout, not the collection's preferred result. Shared data contains the original input/output rates and names, belt capacity and one physical topology. It excludes graphical positions, solve mode, collections, history IDs, result status and proof claims. Input-only links are not supported.

Opening a fragment or JSON file previews the graph after verification. It does not save automatically or start a search. Save to history and view stores a `best_known` witness with no optimality proof and `enumerationComplete=false`. The local form defaults to one minimum-N,L search when copying that configuration later. This default was neither shared nor solved. Retrying a failed save reuses its existing local entry instead of appending duplicates.

### Public format and verification

The public JSON envelope is `{ kind: "selected-solution", version: 1, request, topology }`. Request endpoints contain names and rate strings, without application IDs. An empty input list retains the existing automatic one-belt supply rule. Topology nodes are splitter/merger kinds indexed by array position. Links connect typed producer and consumer ports, including anonymous discard consumers. They omit flows because the receiver reconstructs unique exact flows.

`solver-web::verify_share_json` rejects unknown fields throughout the public envelope, endpoints, topology and ports. It calls the existing exact reconstruction and validation routines and regenerates the presentation. A successful response never claims optimality or complete enumeration. It does not call Canonaut or cvc5. Source labels, embedded scripts and proof fields do not enter trusted presentation data.

`share_from_presentation_json` converts the known `modelVersion=4` display convention to physical topology. It maps terminals by the presenter's documented IDs and checks every supplied exact edge flow before omitting flows from the public format. Display status, proof, statistics, labels and graphical coordinates are not trusted. Older or unknown presentation conventions are rejected rather than guessing a terminal mapping. Existing history import remains separate and does not become a public verification boundary.

### Bounded URLs and workers

Self-contained links use `#s1.<base64url-gzip>`. The codec bounds decoded UTF-8 JSON to 256 KiB, compressed tokens to 65,536 characters and complete inline URLs to 32,768 characters. Limits also cover 256 operators, 1,024 links, 24 endpoints per side and 256-byte rate literals. The public file picker checks file size before reading it. For solutions that exceed the inline URL bound, export the bounded JSON file instead.

Decompression feeds small compressed chunks and checks the accumulated output limit. Truncated gzip, invalid UTF-8, malformed base64url and unsupported versions reject. Both decoding and exact verification run in a dedicated worker. The page terminates the worker after success, failure, cancellation or a 20-second verification deadline. A late response cannot overwrite a newer attempt. This deadline is not a solve-time limit or an optimality result.

The verifier's JavaScript and Wasm files ship together under a content-versioned asset directory. Vite emits the pair from `target/web-backends/verifier`, serves them locally in development and rejects production builds without the pair. No cvc5 asset is copied into this viewer path. The static browser test exercises a GitHub-style project subpath with a restrictive CSP and no external asset requests. Offline caching and upgrade management are still later work.

### Viewer address

`VITE_PUBLIC_APP_URL` can set the public browser viewer address. Browser builds otherwise use the current project directory. Desktop builds require that configuration or an address entered in the dialog, rather than generating an unusable native-app URL. The viewer address must use HTTPS, or HTTP loopback during development, and must contain no credentials, query or fragment. No sharing service or network publication is involved.

### Develop and validate sharing

```powershell
rustup target add wasm32-unknown-unknown
npm run dev:web
```

That command prepares the verifier and browser solver assets, then starts the shared frontend. Opening or creating a share loads only the separate verifier. Import a current history file, choose a layout and use Share selected solution. A desktop share needs the browser viewer address, either configured with `VITE_PUBLIC_APP_URL` or entered in the dialog. Tauri's build hooks build the verifier but omit the browser search distribution. Its CSP permits only the local worker/Wasm assets and native IPC.

```powershell
npm run build:web
$env:WEB_TEST_CHANNEL='chrome'; npm run test:web:shares
```

The tests compare 15 native presentation exports and topology reconstructions against the actual Wasm worker. They also exercise selected-layout export, invalid payload rejection, proofless persistence, text escaping, decompression bounds, worker cancellation and static-only sharing. The browser suite uses ports 4181 and 4182. It starts and stops its own servers and writes evidence under `target/web-shares`.

### Integration after sharing

Step 03 supplies the portable exact planner, leaf driver and canonicalization qualification. Step 04 connects single-worker browser jobs to the shared interface. Offline assets and Pages deployment remain unfinished. No public browser release has been deployed.

### Step 02 validation on 11 September 2026

After removing backend link storage and its UI, all 165 frontend tests, nine sharing browser tests, nine history browser tests, four backend browser tests and 40 Python tooling tests passed. Sharing tests include 15 native/Wasm round trips, selected-layout export, proofless persistence, JSON download/import and a static-only network check. The seven Rust verifier tests and verifier wasm32 Clippy also passed again. The earlier full native workspace run passed 162 tests; this cleanup did not change Rust source.

Native workspace Clippy, verifier wasm32 Clippy and portable job-projection wasm32 Clippy passed with warnings denied. The verifier and production frontend built successfully. Svelte checks, formatting and release metadata checks passed. Tauri library tests also passed after adding the file-size-check capability. The graph shortcut configuration warning still appears during some browser interactions without failing them.

These are local Windows/Chrome results. No public browser deployment, packaged desktop sharing UI test, new installer, Firefox/WebKit qualification, mobile memory-budget test or remote GitHub workflow execution is claimed. Step 01 was committed as `d42ff639f4a509fd0a229fb6c8b35a31865ac6e8`.

## Step 03: portable exact planner and leaf driver

`solver-core` now builds without its default `native-runtime` feature. `planner/` owns objective progression, root dispatch, deduplication, disjoint proof coverage and terminal sealing. `leaf.rs` owns SMT command/reply progression, model blocking, exact reconstruction, validation and witness restoration. Neither owns a process, thread, channel, clock or cancellation atomic. Time enters the planner only as host-supplied telemetry.

The native API names and exact scopes are unchanged. `native_api.rs` retains the public facade and final cancellation check after public identity normalization. `native.rs` owns scoped worker threads, cvc5 sessions, cancellation observations and diagnostic timing. `process.rs` still resolves the same backend paths and kills, reaps and joins each session on drop. The native `portfolio.rs` still divides the worker budget between independent Sparse and Boolean searches, gives Sparse the odd extra worker, and selects one complete proof owner after joining both branches. A one-worker request remains Boolean-only.

### Planner and host contract

`ExactPlanner::new` validates the problem/options and computes the same exact normalization and lower bounds. `poll(elapsed_ms)` returns a `PlannerUpdate` with leaf tasks, internal solver events and closed-group diagnostic evidence. The planner visits N, then exact operator-link L, then the accounted profiles in the existing order. `all_min_n` continues through higher-L groups at the minimum N; `all_min_nl` stops after exhausting the minimum-L group; `one_min_nl` accepts the first validated optimum without claiming equal-L enumeration exhaustion.

Each dispatched leaf has a group/index identity. `PlannerEvent::Witness` accepts trusted output from the exact leaf driver, not arbitrary imported graphs or proof labels. `PlannerEvent::Retired` means the host has already disposed the leaf's backend session. Unknown, duplicate, stale or post-seal events reject. A rejection poisons an unfinished branch rather than discharging an obligation. Browser job/worker epochs remain a host responsibility; the group identity does not replace them.

`poll` is the branch's completion-sealing point. A valid witness alone does not seal completion. The planner waits for every dispatched leaf to retire, including competing searches outside the winning parent/child cover. Cancellation accepted before sealing produces an incomplete result and preserves accepted witnesses. Cancellation after sealing cannot rewrite that branch result. The application host still owns its later cancellation/presentation seal, as the existing native facade does.

The hybrid scheduling rules are unchanged. First-output producers dispatch in descending order. Static second-output partitions replace parents when there are fewer live roots than branch workers. Otherwise, a validated current-group optimum enables adaptive children in spare slots after all unstarted parents. Stable rounds interleave second producers. A parent completion or an exhausted full child cover owns the root, never both. Partial and duplicate child completions cannot establish exhaustion.

Internal branch events retain the existing private search identity convention. A host must normalize caller-scale public identities before app delivery, as native `SolutionCollector` already does. `ExactPlanner::outcome` normalizes terminal identities and sorts the result collection by the public keys. It must not be used as an import verifier. No JS callback implements `SolveObserver: Sync`, and no unsafe Send/Sync declaration was added.

### Leaf protocol

`LeafDriver::advance` starts with no reply, then accepts one complete `check-sat` or `get-value` reply at a time. It returns the next SMT batch, an optional accepted witness and optional completion. A host keeps one incremental session for that leaf. It must publish an accepted witness before the next synchronous backend call, then dispose the session before reporting retirement.

Only exact `sat` and `unsat` replies advance the proof protocol. `unknown`, backend errors, malformed model lists, missing/duplicate variables, non-Boolean values and unexpected replies poison the leaf. A later `unsat` cannot revive a poisoned leaf. Exact reconstruction still rejects nonunique steady states and checks the requested objective and restored caller rates. No floating-point rate conversion or new graph restriction was introduced.

### Canonaut and canonical identity

`vendor/canonaut` contains the audited registry 1.0.0 library with its Apache-2.0 license, notices and source provenance. A root Cargo patch selects it for native and Wasm builds. `PORTABILITY.md` records the original archive SHA-256 and changed paths.

The patch preserves full 64-bit KISS random state, arithmetic and seeds. Conversion to a host index occurs after bounded reduction. Bitsets instead use each target's actual `usize` width throughout masks, scans, one-word/two-word specializations, permutation scratch storage and search bounds. The dependency's unused default entropy path is disabled. Canonical labeling does not use JavaScript randomness or a wall clock. The `layout-v1` encoding in `solver-validation/src/identity.rs` is unchanged.

The immutable `crates/solver-portable-tests/fixtures/identity-v1.json` was captured with the unpatched registry dependency before the port. It contains 30 colored-graph cases and 12 physical witnesses. Every case is compared directly and under three equivalent relabelings in native Rust and actual wasm32 execution. Cases cover 31/32/33, 63/64/65 and 127/128/129 vertex boundaries, symmetric graphs, node/link storage order, symmetric operator ports, equal-rate terminal permutations, discards and feedback. Separate tests compare bitset operations, permutation powers and the full 64-bit RNG sequence against direct expected operations.

The fixture generator now emits explicitly labeled review candidates. It cannot silently label newly generated data as the old unpatched baseline. Do not replace the stored pre-port evidence to make a failed comparison pass.

### Qualification commands and CI

The `solver-portable-tests` crate is a test-only cdylib, separate from the application verifier. It links the production portable planner and leaf driver without the native runtime. Its browser worker runs those components against the existing local cvc5 Wasm session API. This does not enable browser jobs in the Svelte application.

```powershell
cargo test -p solver-core --no-default-features --lib --locked -j 2
cargo clippy -p solver-core --no-default-features --target wasm32-unknown-unknown --lib --locked -j 2 -- -D warnings
npm run build:web:portable
$env:SOLVER_CVC5=Join-Path (Get-Location) 'src-tauri/binaries/cvc5-x86_64-pc-windows-msvc.exe'; $env:WEB_TEST_CHANNEL='chrome'; npm run test:web:portable
```

The last command requires the native backend and existing cvc5 Wasm assets. The fixture generator compares 42 native requests against the independent reference across all three modes. Step 04 extended the original 36 cases with wide rational literals and reordered terminals. Browser tests compare the exact objective, optimality projection, enumeration status and full canonical result sets. Cases include arbitrary fractions, surplus/discards, duplicate terminals, feedback, multiple optimal layouts, global contradictions and a finite node cap. Separate tests cancel after an accepted witness, inject `unknown` after a witness, and exercise actual cvc5 resource exhaustion. All sessions must be disposed before the test accepts a terminal result.

The suite serves only local assets at the project subpath on port 4183, with one browser test worker and no cross-origin isolation headers. Evidence goes under `target/web-portable`. The workflow creates native comparison fixtures on Windows with the pinned cvc5 package, then sends those fixtures to the Linux browser job. It does not rely on an unpinned distribution cvc5 package. Existing verifier, sharing and history suites remain separate.

### Application integration after extraction

Step 04 below connects real single-worker browser jobs to platform services, shared snapshots, UI progress, hard cancellation, history checkpoints and job attempts. The qualification worker remains separate from production code. Step 05 adds parallel browser workers and paged collections. Step 06 covers offline assets, version-coherent upgrades and Pages delivery. No public deployment, browser performance claim or packaged desktop sharing qualification follows from these local tests.

### Step 03 validation on 12 September 2026

The full native workspace passed 173 tests, including all 12 solver/reference integration contracts and the existing desktop cancellation/history tests. The core passed 34 tests with the native runtime and 32 without it. The two additional native tests exercise the cvc5 process and Boolean encoding through a real child process; their modules are correctly feature-gated. The frontend passed 165 tests, and Python tooling passed 40 tests.

All five portable browser tests passed after rebuilding the final Wasm module. They checked 42 pre-port canonical identities with three permutations per case, nine primitive word-boundary sizes, 36 native/reference search comparisons through real cvc5 Wasm, cancellation after a witness, injected unknown after a witness and actual resource exhaustion. The existing nine sharing, nine browser history and four backend tests also passed. Sharing still matched 15 native/Wasm round trips; the backend verifier still matched 28 fixtures.

Native workspace Clippy and wasm32 Clippy for the portable core, qualification module, verifier and shared job projection passed with warnings denied. Svelte checks reported no errors or warnings. Verifier and qualification Wasm builds, production frontend build, formatting, whitespace and release metadata checks passed. The immutable pre-port fixture SHA-256 remains `759271f2ba1ff2b647719650806bc1c050fc0c5f36442c7c017e8a540292f214`. The production identity serialization and native portfolio files are unchanged.

Evidence is in `.tmp/web-step03-native-final.log`, `.tmp/web-step03-frontend-final.json` and the four browser result files under `target/web-portable`, `target/web-shares`, `target/web-platform` and `target/web-backends`. These are local Windows/Chrome results. No benchmarks, installers, public deployment, Firefox/Safari qualification or remote workflow execution were performed. Step 02 is committed as `3dbf9b237200c55b5c6d6be7a455d1ca0984b118`. Step 03 is committed as `0d7cbe28440f95ddf91e3971b26a3e34340526ad` on `feature/web-backend-foundations`.

## Step 04: single-worker browser solving

This section records the completed Step 04 milestone, committed as `00419e54c2265bb8bbb11db1743510a1f8242535`. Step 05 below supersedes its `BrowserRun`, single-worker transport, in-memory collections and acknowledgement boundary.

Find now creates a real browser job through the existing platform `JobClient` and `HistoryQueue`. `solver-browser` is the production Rust/Wasm bridge. It depends on `solver-core` without `native-runtime`, exact validation with canonical identity, and `synthetizer-app` without native execution. Neither `solver-portable-tests` nor the small sharing verifier performs production searches.

`BrowserRun` parses the application `SolveRequest`, limits its UTF-8 JSON to 256 KiB and checks job/endpoint identifier lengths before exact preparation. The shared preparer still checks endpoint counts, names, positive rates, automatic supply and external belt capacity. Rates remain arbitrary-precision strings, not JavaScript numbers. There is no public arbitrary-SMT endpoint. The production options use one Boolean branch and one active leaf. Find imposes no node cap or solve deadline; an optional host node cap is inclusive and returns incomplete on exhaustion, never global UNSAT.

### Exact results and live delivery

The Rust bridge uses the same `ExactPlanner`, `LeafDriver`, exact reconstruction and independent validation as native solving. It normalizes caller-scale canonical keys before live presentation and uses the shared `project_outcome` for terminal results. Enumeration deduplication and terminal source-index preservation therefore use the same public identity convention as desktop jobs.

`one_min_nl` proves minimum N and minimum operator-link L without claiming equal-L exhaustion. `all_min_nl` exhausts that minimum-L collection. `all_min_n` continues through higher-L groups at the same minimum N. Optimized L still excludes external terminal stubs and discard belts. Imported selected solutions remain proofless regardless of the browser's new ability to solve their configurations.

`solve.worker.ts` loads the production Rust module in a dedicated module worker. It loads cvc5 only when a non-impossible leaf needs a session. Global contradictions and rejected requests need no cvc5 download. The worker sends progress and one-result append packets through the existing snapshot contract. The page retains a full snapshot, so `get` and a newly registered `watch` can recover packets missed by the UI. The job registry allows one active search, retains at most eight jobs by default and supports explicit terminal release. `shutdown` closes admission and cancels active work; `resume` reopens admission without restarting old searches.

### Cancellation, failures and the host seal

Each packet carries a protocol version, job ID, unique attempt ID and monotonic checkpoint. Snapshot sequences and append counts must be contiguous. The host ignores foreign/retired attempts and rejects malformed checkpoints without accepting their results or proof.

Before entering the next blocking backend call, the worker publishes each accepted witness and waits for the page's acknowledgement. That checkpoint also includes thin cancellation and worker-failure packets prepared by Rust's shared `interruption_packet`. The page retains the graphs separately. The acknowledgement confirms receipt in page memory, not an IndexedDB commit. These packets describe interruption at that checkpoint, not persistent search state or a resume certificate.

Cancellation terminates the worker from the page. It does not depend on a cancel message reaching synchronous cvc5, a shared Wasm heap or a JavaScript callback marked `Send`/`Sync`. After termination, the page combines the retained graphs with the Rust-projected interruption packet. Any accepted witnesses remain `best_known`; enumeration stays incomplete and interruption cannot establish minimum L. Proven minimum N may remain when the completed lower-bound evidence supports it. A failure before the first Rust checkpoint has no witness or proof to retain.

For normal completion, the worker disposes every backend session, prepares the shared terminal projection and frees its Rust run before sending the completion proposal. The page terminates the worker and seals the job when it accepts that proposal, before notifying subscribers. Cancellation accepted before this host seal wins. Cancellation after it returns the sealed result unchanged. Unknown, resource exhaustion, malformed models, traps, transport failures and failed session disposal cannot become UNSAT or complete enumeration.

The page enforces a 120-second startup limit. It removes that timer once cvc5 loads; no solve timeout applies after readiness. A timed-out load becomes a failed, incomplete job and a later search creates a fresh worker. Progress packets describe completed planner/leaf work. A single synchronous backend call can delay new solver telemetry, but the page and its elapsed-time display remain independent.

### History and local assets

Completed jobs and acknowledged partial witnesses use the existing IndexedDB transaction and checkpoint path. Live running jobs serialize as incomplete checkpoints, clear job ownership and retain their solutions and graph edits. Reload never resumes the search or restores queued work. Pagehide saves supplement normal checkpoints but cannot guarantee a final transaction when the page is destroyed. Storage remains origin-local and best-effort.

`frontend/solverAssets.ts` emits the production Rust JavaScript/Wasm pair, cvc5 JavaScript/Wasm pair, session adapter, manifest, build record and notices into one content-hashed directory. It verifies cvc5 artifact hashes against its build record. URLs resolve beneath the application's base path. There are no runtime CDNs, backend services or cross-origin isolation requirements. Opening a share or browsing history does not load the search modules. The separate verifier directory remains unchanged.

`prepare-web-solver.py` checks cached source pins, build flags, wrapper hash, runtime hashes and notices before reusing cvc5. A matching build avoids relinking the toolchain. A stale or absent build invokes the existing pinned builder. Desktop Vite mode omits the browser search distribution; Tauri continues to use its native sidecar. Notice copies and build records do not replace the source/relink distribution work required before public release.

### Build and validate the application

```powershell
npm run dev:web
npm run build:web
```

Both commands prepare browser assets. They use the existing Linux/WSL cvc5 builder only when its verified outputs are missing or stale. To validate production jobs against fresh native/reference fixtures:

```powershell
$env:SOLVER_CVC5=Join-Path (Get-Location) 'src-tauri/binaries/cvc5-x86_64-pc-windows-msvc.exe'; node scripts/prepare-portable-fixtures.mjs
$env:WEB_TEST_CHANNEL='chrome'; npm run test:web:solver
```

The production suite uses one Playwright worker, Vite on port 4184 and the static subpath/CSP server on port 4182. It verifies 39 application-compatible cases from the 42-case native/reference corpus. The other three cases exceed external terminal capacity and are rejected by the application preparer before solving. The test-only native `verify_browser_jobs` example reconstructs and validates every emitted graph, compares complete canonical collections and checks that terminal projection preserves live source indices. Comparison evidence is attached under `target/web-solver/test-results` and indexed by `target/web-solver/results.json`.

The application tests also cover all three Find controls, completed history reload, interrupted history reload, missed-append reconciliation, cancellation during a real blocked cvc5 call, cancellation after a witness, worker replacement, unknown, resource limits, traps and disposal failure. Fault injection replaces test HTTP responses rather than adding a production test switch. The existing sharing, history, portable and backend suites remain separate. The workflow prepares comparison fixtures on Windows and consumes them in its Linux browser job.

### Remaining phases

Step 05 adds independent parallel compute workers, paged/batched collections and bounded collection backpressure. Step 04 acknowledges worker checkpoints but still retains full collections in memory and returns full terminal snapshots. It does not qualify very large result sets or device memory limits.

Step 06 covers offline asset caching, version-coherent upgrades, distribution/source/relink packaging and authorized Pages delivery. Firefox, Safari/WebKit, device-pressure testing and packaged Tauri/WebView2 end-to-end checks remain unqualified. No benchmark campaign, installer, public deployment, push or version change is part of Step 04.

### Step 04 validation on 12 September 2026

The full native workspace passed 179 tests, including the 12 solver/reference integration contracts, 19 desktop tests, four production browser bridge tests and 14 shared application tests. The portable core also passed its 32 no-native unit tests. Frontend tests passed 182 cases, including 17 browser job ownership tests. Python tooling passed 45 tests, including five new cached-asset preparation tests.

All 40 browser tests passed in installed Chrome, with no skipped, flaky or failed cases: 13 production job/UI tests, five portable tests, nine sharing tests, nine history tests and four backend tests. Production jobs matched 39 application-compatible native/reference cases, including full canonical result collections. The expanded portable corpus matched 42 lower-level search cases. The existing 42 pre-port identities, 15 selected-solution round trips and 28 verifier fixtures still passed.

The two assertions left unresolved in the initial Step 04 run now pass. Reload tests correctly expect the stored running checkpoint to be `incomplete`, while the live page still shows a running job. Missed-append reconciliation now uses an all-min-N problem with several layouts rather than assuming a one-layout problem has more results. Disposal-failure handling also passed after changing it to require whole-worker termination instead of claiming leaf retirement.

Native workspace Clippy and targeted wasm32 Clippy passed with warnings denied for the production browser bridge, portable core, shared application, verifier and qualification module. Svelte checks reported no errors or warnings. `npm run build:web` rebuilt both Rust modules and reused the verified cvc5 artifacts without relinking them. The search directory was `assets/solver-53a0331e11fbbf29`; the separate verifier remained `assets/verifier-cb682091b747c5b7`. These hashes identify the tested asset sets, not a guarantee of byte-identical builds on every host.

Evidence is in `.tmp/web-step04-native-final.log`, `.tmp/web-step04-frontend-final.json`, `.tmp/web-step04-clippy-native.log` and the five browser result directories. The existing non-failing graph shortcut configuration warning still appears in sharing/history interactions. These are local Windows/Chrome checks, not remote CI or other-browser qualification. Step 04 is committed as `00419e54c2265bb8bbb11db1743510a1f8242535` on `feature/web-backend-foundations`; Step 03 remains commit `0d7cbe28440f95ddf91e3971b26a3e34340526ad`.

## Step 05: independent workers and paged collections

### Worker ownership and exact search

Production protocol version 2 separates `BrowserCoordinator` and `BrowserLeaf`. `solve.worker.ts` runs the Rust coordinator with no cvc5 heap. `leaf.worker.ts` runs one exact leaf at a time in an independently owned Rust/cvc5 heap, reusing its initialized modules between disposed sessions. The document owns both worker types through `browserJobs.ts`. Exact preparation, planning, reconstruction, validation, canonical identity and proof/status projection remain in Rust workers, not on the main page.

The browser control defaults to Automatic, which uses `navigator.hardwareConcurrency`, the number of logical processors reported by the client browser. It offers lower power-of-two budgets and the exact reported count, including non-power-of-two counts such as 12 or 24. It remembers an explicit choice; Automatic follows the current client after reload. A missing or invalid hardware report falls back to one worker. A stored count above the current report falls back to Automatic. The new `sfs.browser-workers.v2` preference replaces the earlier preview's automatically stored one-worker default.

The page validates budgets against the reported thread count. The Rust bridge accepts positive 32-bit counts instead of imposing an eight-worker limit. A coordinator worker is additional to the compute budget. Workers are created only for dispatched leaves, so a small search can use fewer workers than the selected maximum. More workers consume more memory and need not improve small searches. The browser can report fewer logical processors than the machine physically has, as allowed by the HTML Standard's concurrent hardware capabilities section. Startup has a separate 120-second limit for the coordinator and each new compute worker. There is no production solve deadline after readiness.

One compute worker uses a Boolean branch. Larger budgets use independent Sparse and Boolean branches; Sparse receives the odd extra slot. Explicit Boolean/Sparse strategies remain host qualification options. Each branch uses the existing exact root planner, including static second-output partitions and adaptive children where eligible. Adaptive children require a validated witness and spare execution slots. Parent exhaustion and complete disjoint child coverage remain alternative proof owners.

One complete branch owns the returned proof. Its layout set is a subset of the collected union, so equal cardinality checks complete set coverage without adding proof counters from other branches. A failed independent branch does not invalidate a healthy complete owner. Conversely, several incomplete branches never make a complete result. `all_min_n` still visits higher-L groups at minimum N; `all_min_nl` requires equal-L exhaustion; `one_min_nl` does not claim enumeration exhaustion. Native portfolio behavior is unchanged.

Every compute packet includes job/attempt identity, a worker incarnation, a branch-qualified leaf ID and a contiguous per-task sequence. Coordinator checkpoints are independently sequenced. Foreign and late packets cannot resume retired work. The page terminates stopped workers before acknowledging retirement, and a leaf reports exhaustion/optimum only after session disposal. A disposal exception requires terminating its whole worker. Completion is accepted only after every tracked compute obligation is retired; the page then terminates remaining idle heaps and the coordinator before sealing and notifying subscribers.

### Bounded delivery and cancellation

A compute worker holds at most one unacknowledged event. The coordinator queue allows at most `max(16, 2 * workerCount)` single-event envelopes, accounting for a witness and a subsequent forced retirement per worker. It drains at most 16 envelopes and publishes at most 16 new indexed solutions in one checkpoint. Only one coordinator checkpoint may be in flight. Queue and retired-attempt bookkeeping scale with the selected budget instead of failing at fixed eight-worker or 32-event limits. Each witness is validated and assigned its caller-scale canonical identity before publication. The page commits the append and its summary index to IndexedDB before acknowledging the compute workers or requesting more coordinator work. Heartbeats between synchronous checks do not transfer proof or witnesses.

Cancellation immediately terminates all owned workers, including a worker blocked in cvc5. If an accepted append transaction is already committing, the page waits for that transaction, retains the prefix and then applies the accompanying Rust interruption projection. Cancellation before the host completion seal wins; cancellation after it leaves the result unchanged. Worker traps, unknown replies, failed disposal, malformed packets and posting failures preserve the previously accepted prefix and cannot become UNSAT or exhausted enumeration.

A quota/transaction failure stops the job. The failed batch remains in a bounded tab-local overlay for viewing, export and save retry. The shared history store owns that overlay after job metadata is released. A retry verifies an already committed immutable batch rather than overwriting its source indices. Failed history revisions or late transaction aborts do not acknowledge a save.

### Collection and UI ownership

`ExactPlanner::streaming` retains canonical keys and a best witness instead of every graph. It deliberately returns no full `SolveOutcome`; `public_result` provides scalar terminal evidence for the browser projection. Native callers continue using the graph-retaining planner. The browser coordinator retains stable key-to-source-index mappings and bounded pending graphs. Job `get`/`watch` return scalar snapshots, a preferred graph and a `CollectionRef`, never the full enumeration. Every update carries the complete immutable prefix reference, so a missed UI notification is repaired by a later snapshot without downloading the graph collection.

IndexedDB schema 2 adds `collections`, `collectionSolutions` and `collectionSummaries`. Each full solution has a stable source index and a separate stats-only summary. The table requests 64 rows; storage reads reject page sizes above 128; selecting a graph reads one solution. Sorting uses exact rational peak values and stable source-index ties. Pagination and sort order do not change selected-source identity, preferred-result identity or saved graph edits. Async page/graph requests reject stale responses. Graph edits and telemetry do not restart an unchanged page query.

Large schema-1 histories are read through lazy legacy references without rewriting all graphs during upgrade. Small legacy/native history remains compatible. History stores only a collection reference and the preferred graph, rather than cloning the full browser collection on every checkpoint. Interrupted reloads recover any newer durable prefix available behind their saved reference, without adding proof or restarting computation. Completed prefixes do not silently expand. Deleting the last referencing history entry deletes its stored collection; other entries referencing it are preserved.

Selected-solution sharing exports the graph actually selected, including when its source row is on another page. History JSON export reads source-ordered pages and omits local collection IDs; existing history JSON remains portable. Large imported histories move their graph records out of Svelte after parsing. Selected-solution sharing is still proofless and self-contained, with no short-link service or input-only format.

### Resource limits and remaining memory costs

Default browser collection guards request interruption at 50,000 unique layouts or 64 MiB of coordinator canonical-key text. These are resource guards, not impossibility bounds. Already received events can finish their bounded batch; accepted witnesses survive and enumeration remains explicitly incomplete. The identity-byte guard is not a measurement or cap of total browser memory.

Canonical key sets still grow with the collection, including independent branch copies. Sorting scans and sorts an O(N) stats index on the document thread; only its returned row window contains UI data, and the sort index contains no full graphs. User graph-layout caches grow with edited/viewed layouts. JSON export accumulates serialized Blob bytes and import initially parses the whole file. Each cvc5 heap also retains its active encoding and has its existing 1 GiB module maximum; that maximum is not a safe per-device allocation promise. The ownership tests demonstrate removed graph duplication, not constant total memory or a speedup.

Collection acknowledgements do not guarantee that the newest history metadata survives sudden tab/process destruction. Normal checkpoints and best-effort pagehide saves still govern discoverability. Temporary collections created before a history entry is attached can remain unreferenced after abrupt termination; no automatic cross-tab garbage collector is claimed. Browser storage remains origin-local and best-effort. Offline caching, version-coherent asset upgrades, public source/relink packaging and authorized Pages delivery remain Step 06. Other-browser, packaged WebView2 and device-pressure qualification remain outstanding.

### Validation coverage

`npm run test:web:solver` runs 39 application-compatible native/reference cases at each of one, two, four and 32 compute workers, for 156 exact comparisons of objectives, proof projection, complete canonical collections and published source indices. Tests set the page's reported count to 32 so the same explicit budgets remain testable on smaller CI runners. A separate test uses the client's unmodified hardware report and verifies Automatic through an actual completed Find operation. UI tests cover reported counts of 12, 24, 64 and 192 without allocating those large budgets. Unit tests cover burst queues through 192 workers and page-owned termination of 64 simulated compute workers.

Additional tests exercise a failed Sparse branch with a healthy Boolean proof owner, explicit Boolean scheduling, collection guards, four-worker blocked-query cancellation, paged graph editing/reload, off-page selected sharing and source-ordered export. The native verifier materializes only small qualification fixtures, not production job snapshots.

`npm run test:web:platform` includes real IndexedDB collection tests for a 2,048-record structural ownership fixture, exact sorting, bounded graph reads, missing records, mismatched retries, noncontiguous batches, late aborts, quota overlays, prefix recovery, schema migration and reference-aware deletion. The fixture is synthetic storage data, not a claim of 2,048 distinct solver witnesses. Frontend tests cover transport/epoch failures, backpressure, cancellation races, coalesced pages and out-of-order selected-graph reads. Existing portable, sharing, native and backend suites remain separate.

### Step 05 validation on 12 September 2026

The completed local runs passed 184 native workspace tests, 33 portable core tests without the native runtime, 220 frontend tests and 45 Python tooling tests. All 60 browser tests passed in installed Chrome: 26 production solver/UI tests, 16 platform/history tests, nine selected-solution sharing tests, five portable tests and four backend tests. The production cases include 156 native/reference comparisons at budgets of 1, 2, 4 and 32 workers. The actual Automatic Find test uses the browser's unmodified thread report. Larger synthetic reports through 192 test configuration and queue bounds, not 192 simultaneous real solver heaps.

The final history suite rerun replaced an obsolete introductory-text assertion with the labeled worker selector. Its storage and graph-edit checks all passed. The two earlier graph-session fixture failures also pass; selecting another test entry no longer inherits the previous entry's cached layout. No production assertion was weakened to turn an incomplete search into a completed one.

Native workspace Clippy and targeted wasm32 Clippy passed with warnings denied. Svelte checks reported no errors or warnings. Browser and desktop-mode frontend builds passed. Desktop output retains the verifier and omits the browser solver distribution. The small verifier remains separate, and the immutable pre-port identity fixture retains SHA-256 `759271f2ba1ff2b647719650806bc1c050fc0c5f36442c7c017e8a540292f214`.

Frontend evidence is in `.tmp/web-step05-frontend-threads.json`; browser evidence is in the five result directories under `target/web-solver`, `target/web-platform`, `target/web-shares`, `target/web-portable` and `target/web-backends`. The full native workspace and final lint/tooling runs also completed successfully in the recorded terminal session. The non-failing graph shortcut warning remains. These are local Windows/Chrome results, not remote CI, other-browser qualification, a benchmark campaign or packaged WebView2 validation. Step 05 is committed as `3b7f24bcda4cc2b48c6abb3786cbd6898917106d` on `feature/web-backend-foundations`. No push, deployment, installer or version change occurred.

## Step 06: offline and publication delivery

### Offline runtime and upgrades

Production browser builds register one service worker beneath the application's current base path. Vite emits a build-specific `release.json`, injects the same build ID into the page and embeds the release manifest into `service-worker.js`. Every manifest-listed runtime file records its exact byte count and SHA-256 hash. The worker verifies both before storing a network response. Downloads are sequential so hashing a large cvc5 Wasm response does not multiply transient buffers.

Initial installation caches the interface shell. Full offline readiness is explicit: **Download for offline use** saves every runtime asset, including the small selected-solution verifier and the browser solver/cvc5 distribution, before the UI reports that offline solving and viewing are ready. A missing, truncated, corrupted or quota-failed download cannot mark that build complete. Cache eviction is visible on the next status check and the user can repair it explicitly. Source archives are publication downloads, not runtime cache entries.

Updates never call `skipWaiting` or `clients.claim`. If the active version was fully prepared for offline use, a candidate version must prepare its complete runtime before it can wait. Existing tabs therefore keep the old service worker and their current solver-worker ownership until every app tab closes. The new version activates on the next open and removes only older `sfs-offline-v1` caches for the same service-worker scope. It never clears IndexedDB history, and reload still never resumes a mathematical search.

The static delivery uses relative URLs and a local-only CSP, so the same artifact works beneath the normal GitHub project path without a custom domain or cross-origin isolation. Unknown URLs remain normal 404 responses rather than being rewritten to the application shell.

### Public source and relink package

`npm run build:web:release` packages publication material before the frontend build. `licenses.html` links content-hashed downloads for the exact application source snapshot, installed Rust/JavaScript dependency sources, pinned ELK/elkjs sources and the cvc5 relink bundle. `source-manifest.json` records source-file hashes, archive hashes, the source fingerprint and the cvc5 runtime build record. The release-mode Vite plugin refuses a stale or mismatched source snapshot.

The cvc5 relink bundle includes the pinned cvc5, GMP, CaDiCaL and SymFPU sources, the Emscripten source pin, the application wrapper source/object, every static library used at the final link, notices, recorded build flags and a standalone `relink.py`. `npm run verify:web:relink` extracts that public archive, relinks it with Emscripten 3.1.70 and requires the resulting `cvc5.mjs` and `cvc5.wasm` hashes to match the shipped module exactly. A compatible rebuilt `libgmp.a` can be supplied to the same relink recipe.

### GitHub Pages handoff

`.github/workflows/pages.yml` is intentionally manual (`workflow_dispatch`). Its build job prepares the publication artifact on Ubuntu, verifies the relink bundle, runs frontend checks and the offline-delivery browser suite, then uploads `frontend/dist` with `.nojekyll`. The deploy job uses the `github-pages` environment with only `pages: write` and `id-token: write` permissions. Repository maintainers must set **Settings > Pages > Source** to **GitHub Actions** once before running it. The workflow does not push, merge, tag, bump the application version or choose an automatic branch deployment policy.

See [Browser release and GitHub Pages](web-release.md) for the operator steps. Local validation establishes publication readiness only; it does not establish that remote GitHub Actions has run or that a public site exists.

### Step 06 validation on 12 September 2026

The full native workspace still passes 184 tests. The frontend passes 220 tests and Svelte checking reports no errors or warnings. Python tooling passes 49 tests, including deterministic source-archive packaging, npm license-metadata fallback and exact relink-record checks. Formatting, whitespace and release-metadata checks pass. The immutable pre-port identity fixture remains SHA-256 `759271f2ba1ff2b647719650806bc1c050fc0c5f36442c7c017e8a540292f214`.

All 67 browser tests pass in installed Chrome: 26 production solver/UI tests, 16 platform/history tests, nine selected-solution sharing tests, five portable tests, four backend tests and seven new delivery tests. Delivery tests cover lazy first load, explicit full offline preparation, all three exact scopes after a network-off reload, offline selected-solution viewing, missing/corrupt asset recovery, waiting multi-tab updates, preservation of browser history across activation, failed candidate rollback, visible cache eviction, normal 404 behavior and project-path cache isolation.

`npm run build:web:release` builds the verifier and production solver, reuses the verified cvc5 build, creates the public source downloads and emits the static publication directory. The tested release manifest contains 38 integrity-tracked runtime assets. `npm run verify:web:relink` reproduces the shipped cvc5 JavaScript and Wasm byte-for-byte from the public relink archive; the hashes remain `5de57bc7ebfd73bfc795f6a0fc66e560d48e5f31013b09461a66d3f6c44adbed` and `8b1d89aef4959100ee5c19902906a37fae7634f7efcfb3c6237da1502aca2bc0`. A desktop-mode frontend build still contains the small verifier and omits cvc5/search assets, the service worker and release metadata.

These are local Windows/Chrome results with cvc5 relinking executed through Ubuntu WSL. The repository is public and its default branch is `develop`, but the new Pages workflow is manual and has not been pushed or run remotely. No Pages site, version change, tag, merge, installer, performance claim, Firefox/Safari qualification or packaged WebView2 test is claimed by Step 06.
