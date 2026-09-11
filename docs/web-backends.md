# Browser backend foundations

This phase adds testable backend modules. It does not enable browser solving in the Svelte application or deploy a GitHub Pages site. The native scheduler, cvc5 process transport, SQLite history and desktop interface are unchanged.

## Agreed product scope

The browser release must support exact single-solution optimization and complete minimum-N,L enumeration. Keep all minimum-N enumeration in the implementation plan too. A viewer-only build does not meet the browser release requirements.

Use one Svelte interface with host-specific adapters where needed. Sharing publishes one selected physical solution with its input/output rates, belt capacity and topology. It does not initially include graph coordinates, solve mode or the full result collection. Do not add input-only sharing. Generate the default display from the shared topology.

Short revocable links and local history with offline use are required. Short-link storage is a separate service from static GitHub Pages hosting. Offline use covers locally available data and solving after the required assets have been cached, not guaranteed offline retrieval of an unseen short link. Do not promise revocation of copies someone has already downloaded.

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

## Local validation on 11 September 2026

The first backend proof gate passed on Windows with Chrome, with the cvc5 module built in Ubuntu WSL. All four browser tests passed, including 28 native/Wasm verification fixtures, persistent session isolation, coexistence of both Wasm modules and termination/recreation during a blocked query. The session test enumerated two exact models to UNSAT, returned `unknown` with `resourceout` on a resource-limited query, and disposed/recreated 16 sessions.

The full native workspace passed 155 tests. The frontend passed 150 tests, and Python tooling passed 40 tests. Native workspace Clippy, verifier wasm32 Clippy, Svelte checks, formatting, release metadata and the frontend production build also passed. `npm run build:web:backends` completed using the checked-in Windows-to-WSL build path.

The verifier Wasm file is 484,367 bytes, or 170,188 bytes under local gzip compression. The proof cvc5 Wasm file is 32,908,722 bytes, or 6,454,698 gzip bytes. These exclude JavaScript and other assets. Compression sizes are file measurements, not measurements of HTTP delivery, browser memory or search performance.

Browser evidence is written to `target/web-backends/browser-results.json`; source pins and cvc5 artifact checksums are in `target/web-backends/cvc5/build.json`. The added GitHub workflow has not run remotely. Firefox, Safari, mobile behavior and full browser search parity have not been qualified in this phase.

## Canonicalization audit and next gates

The published `canonaut` 1.0.0 dependency is not wasm32-ready. The ordinary validator build with identity enabled first fails in its default `rand`/`getrandom` dependency. In a detached source probe with only seeded `SmallRng` enabled, wasm32 compilation reaches the Canonaut code and reports eleven overflowing `usize` literals. These are in `src/rng.rs` and `src/utilities.rs`.

The audit also found hardcoded 64-bit operations in `src/structs/schreier_arena.rs`, including division/remainders by 64 on `usize` bitsets, plus shifts by 58 and 43 in the RNG. Fixing only entropy or truncating constants would not establish a correct port. No Canonaut source or native identity encoding changes are included in this phase.

Before browser solving is released, port or replace canonicalization with parity tests around 31/32/33 and 63/64/65 vertices, relabeled graphs, duplicate terminals and the existing `layout-v1` corpus. The native canonical byte contract must not change silently. Complete minimum-N,L enumeration remains blocked until this gate passes.

The following phases separate mathematical search transitions from native thread/process ownership, implement worker scheduling with unchanged proof obligations, and connect the shared job contract to Svelte. Then add the selected-solution schema and short-link service, browser persistence, offline assets and Pages deployment. Browser exactness tests must compare canonical result sets and proof completion against native runs, including interrupted enumeration. The current backend tests do not establish full search parity.
