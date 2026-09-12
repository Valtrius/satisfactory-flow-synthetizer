# Development

Run commands from the repository root. The root README covers installation and basic use.

## Build and validation

`npm run dev` and `npm run build` prepare the pinned cvc5 package automatically. The first preparation downloads the official archive; later builds verify and reuse the cache. Generated executables and notices stay outside Git.

```powershell
npm ci
npm run prepare:cvc5
$env:SOLVER_CVC5 = Join-Path (Get-Location) 'src-tauri/binaries/cvc5-x86_64-pc-windows-msvc.exe'
npm run format:check
npm test
npm run test:vendor
npm run check
npm run build -w frontend
python -m unittest discover -s scripts -p "test_*.py"
python scripts/check-release.py
cargo test --workspace --locked -j 2 -- --test-threads=1
cargo clippy --workspace --all-targets --locked -j 2 -- -D warnings
```

Use `npm run format` before committing. CI and release publishing share `.github/workflows/validate.yml`. Release tags also require matching package versions and nonempty version-specific notes. Tests must not enforce machine-dependent speed thresholds.

`SOLVER_CVC5` is an authoritative development override. Otherwise lookup checks beside the executable, PATH, then `%LOCALAPPDATA%/Programs/cvc5/bin/cvc5.exe`. An invalid override fails rather than silently choosing another backend.

## Code map

| Location                   | Responsibility                                               |
| -------------------------- | ------------------------------------------------------------ |
| `crates/solver-api`        | Exact rates, requests, result scopes, proofs and progress    |
| `crates/solver-core`       | cvc5 encoding, sparse/Boolean portfolio and proof scheduling |
| `crates/solver-validation` | Independent exact graph validation and layout identity       |
| `crates/solver-reference`  | Exhaustive small-case test oracle                            |
| `crates/synthetizer-app`   | Production solve API and graph presentation                  |
| `src-tauri`                | Jobs, owned solver threads, IPC and SQLite history           |
| `frontend`                 | Queue, history, graph placement and export                   |
| `benchmarks`               | Cases, protocols and performance evidence                    |

Keep exact-result and migration fixtures. Presentation tests use fixed validated graphs; solver integration tests exercise the production API separately.

## History and shutdown

SQLite schema 4 migrates the original JSON schema and relational schema 3 transactionally. Older engine labels and solve-mode aliases remain import compatibility, not active backend choices. Unknown or corrupt schemas fail without deleting stored data.

Writes coalesce for 400 ms, with a five-second maximum checkpoint delay during continuous updates. Interrupted running work reloads as incomplete. The frontend releases full terminal snapshots after accepting them; backend retention is bounded as a fallback.

Persistence diffs use detached, acknowledged snapshots. An unchanged result prefix can append its new suffix in the same transaction as metadata and graph-position updates. Corrections, reordering, removals and preferred-proof changes use full replacement. A lost acknowledgement can cause a prefix-count mismatch; an atomic retry replaces affected entries without rewriting unrelated history. Frontend and SQLite tests share the incremental wire fixture.

The collector normalizes identities after restoring caller rates and terminals. Presentation reuses those public identities and any matching live, unproved display objects. Terminal proof application and missing-result conversion remain separate; cached displays cannot confer proof.

Closing hides the window before cancellation and persistence finish. The application keeps ownership of solver threads until they join. Failed saves offer Retry or Exit without saving. Failed solver cleanup offers Retry cleanup or Return to app. Cancellation is accepted until search seals; a later request does not discard a completed mathematical result.

File import/export uses paths selected by native dialogs. Frontend filesystem permissions do not grant recursive access to the home directory.

## Further references

The [browser architecture guide](web-backends.md) covers production exact worker jobs, paged IndexedDB collections, sharing, offline delivery and qualification commands. The `Web backend proof` workflow tests browser contracts separately from desktop packaging. Dated implementation evidence is kept in the [milestone archive](web-backend-history.md).

The [Windows release guide](release.md) covers native distribution checks. The [browser release guide](web-release.md) covers offline delivery, source/relink packaging and GitHub Pages. The [benchmark guide](../benchmarks/README.md) defines timing and exact-result comparisons. The [memory index](../.serena/memories/index.md) links proof contracts and experiment decisions.
