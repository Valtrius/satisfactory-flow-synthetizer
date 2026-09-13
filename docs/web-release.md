# Browser release and GitHub Pages

The browser build is a static application. It has no link-storage service or solver backend. Search, selected-solution verification and history stay in the browser.

## Build a publication artifact

Run from the repository root:

```powershell
npm ci
npm run build:web:release
npm run verify:web:relink
$env:WEB_TEST_CHANNEL='chrome'; npm run test:web:delivery
```

`build:web:release` prepares the pinned verifier and browser solver, packages the corresponding source and relink material, then writes the static site to `frontend/dist`. A normal `npm run build:web` remains a development build and emits a warning page instead of publication source downloads.

The publication directory contains `release.json`, `service-worker.js`, `.nojekyll`, `licenses.html`, `source-manifest.json`, content-versioned runtime assets and source archives under `sources/`. The service worker does not cache source archives.

## Offline behavior

The service worker automatically downloads the complete runtime on the first visit (currently about 35 MiB). The UI reports full offline readiness only after every runtime asset in `release.json`, including the selected-solution verifier and browser solver/cvc5 files, has passed its recorded byte-count and SHA-256 check and entered Cache Storage. Solver and verifier code still execute only when needed. Before mounting the interface on an initial visit, bootstrap waits for installation and reloads to obtain the active worker's document and assets together. Controlled documents also check their build ID before mounting. If installation fails, the app can run online. A later successful retry saves the files and asks the user to finish work and reopen to use them, preserving any running work. Browser eviction can require a repair download.

The browser checks for updates when the app registers its worker on a visit, when connectivity returns, or when the user chooses **Check for updates**. An update installs beside the active version and must download the complete new runtime before it can wait. It never calls `skipWaiting` or `clients.claim`. Open tabs continue using their old cached build, including solver files they have not used yet, and keep ownership of running solver workers. Close every app tab, then reopen the site to activate the waiting build. Activation deletes only caches carrying this application's scoped `sfs-offline-v1` prefix. IndexedDB history is separate and is not cleared by service-worker activation.

Closing or reloading a tab never resumes a search. Browser storage remains best-effort and can be evicted by the browser, so export history that must be retained.

## Publish with GitHub Pages

The repository includes `.github/workflows/pages.yml`. It is manual and does not deploy automatically on branch changes.

One-time repository setup requires admin or maintainer access:

1. Open **Settings > Pages**.
2. Under **Build and deployment**, set **Source** to **GitHub Actions**.
3. Review the `github-pages` environment protection rules if the repository uses them.

For a reviewed ref already available on GitHub, open **Actions > Publish browser application**, choose **Run workflow**, and select the reviewed ref. The workflow builds the publication artifact on Ubuntu, runs frontend and offline-delivery tests, uploads `frontend/dist`, then deploys that exact artifact through the `github-pages` environment.

No custom domain is required. Relative asset URLs and the scoped service worker support the normal GitHub project path. The browser itself creates selected-solution links from its current project URL. A separately built desktop application can use the Pages address through its existing `VITE_PUBLIC_APP_URL` setting.

The workflow does not enable Pages automatically and does not push, merge, tag or change the application version. GitHub Pages must already use GitHub Actions as its publishing source.

## Source and relinking material

`scripts/package-web-sources.py` creates the files linked by `licenses.html`. The package includes the application source snapshot and build scripts, source for shipped Rust and JavaScript dependencies, pinned ELK/elkjs source, and a cvc5 relink archive.

The cvc5 archive contains the exact cvc5, GMP, CaDiCaL and SymFPU sources used by the browser module, the Emscripten source pin, the linked static libraries, `session.o`, its C++ source, build metadata, notices and `relink.py`. The included recipe can relink the original module or substitute a compatible Emscripten-built `libgmp.a`.

`npm run verify:web:relink` extracts that public archive in a temporary directory, relinks it with the pinned Emscripten toolchain, and requires the resulting JavaScript and Wasm byte counts and SHA-256 hashes to equal the shipped cvc5 module. The Pages workflow runs this gate before deployment.

`source-manifest.json` records byte counts and SHA-256 hashes for every downloadable source archive, the application source-file hashes and the cvc5 runtime artifact record. The frontend release build rejects a stale source snapshot.
