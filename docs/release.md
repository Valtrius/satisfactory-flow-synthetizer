# Prepare a Windows release

Select the next unused version before publishing. Preparation and validation do not create or push a tag. Local verification builds may still carry the previous version.

1. Update all version files with `npm run release:version -- <version>`.
2. Review [UNRELEASED.md](release-notes/UNRELEASED.md), then save the final highlights
   as `docs/release-notes/<version>.md`. The release workflow prepends that exact
   version's file to the generated Conventional Commit changelog.
3. Run the checks and package commands below. Commit the version and final notes
   before creating the release tag. Publish only the intended reviewed commit.

```powershell
npm run format:check
npm test
npm run check
python scripts/check-release.py
python -m unittest discover -s scripts -p "test_*.py"
npm run prepare:cvc5
$env:SOLVER_CVC5 = Join-Path (Get-Location) 'src-tauri/binaries/cvc5-x86_64-pc-windows-msvc.exe'
cargo test --workspace --locked -j 2 -- --test-threads=1
cargo clippy --workspace --all-targets --locked -j 2 -- -D warnings
npm run build
npm run package:release
npm run verify:release
```

The installer, portable ZIP and SHA256 files are written to
`target/release/bundle/distribution`. Package verification uses 7-Zip to extract
both formats, checks application/backend/notices hashes, runs an exact production
API solve with external backend lookup disabled, and confirms that removing the
bundled backend fails. It does not install the app or open user history.

The cvc5 archive and checksum are pinned in `src-tauri/cvc5-package.json`.
The first preparation downloads that archive; solving itself is offline.
WebView2 is required; the installer can install it when missing. An unsigned
build may show Windows SmartScreen prompts.

Keep failed benchmark samples with their records. The accepted hybrid tradeoff
and source identity are documented in
[the promotion record](../benchmarks/optimization/results-hybrid-20260910.md).
Do not turn the release check into another performance campaign.

Before tagging, run `python scripts/check-release.py --tag <version>`. The release workflow runs the shared validation job against that exact commit before packaging or publishing. It rejects missing notes or inconsistent versions.

Package verification does not replace an installer upgrade or interactive desktop test. In an isolated app-data profile, check startup, import/export outside the home directory, graph placement, cancellation, and closing during a hard solve. Check the failed-save choices and confirm that cvc5 children exit after the window disappears. Do not use a release test to migrate the user's real history.

For 1.0.0, complete the five performance-opportunity experiments before the historical release comparison. Keep the old release notes and SVGs unchanged. Generate the new three-version wall-seconds chart and table from one provenance-backed dataset; caps are not completion times.
