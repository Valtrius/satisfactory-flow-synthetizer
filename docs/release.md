# Prepare a Windows release

The repository currently declares 0.2.0, and that tag already exists. Select the
next version before publishing; a local build with that number is only a
verification artifact. Release preparation does not create or push a tag.

1. Update all version files with `npm run release:version -- <version>`.
2. Review [UNRELEASED.md](release-notes/UNRELEASED.md), then save the final highlights
   as `docs/release-notes/<version>.md`. The release workflow prepends that exact
   version's file to the generated Conventional Commit changelog.
3. Run the checks and package commands below. Commit the version and final notes
   before creating the release tag. Publish only the intended reviewed commit.

```powershell
npm run format
npm test
npm run check
python -m unittest discover -s scripts -p "test_*.py"
npm run prepare:cvc5
$env:SOLVER_CVC5 = Join-Path (Get-Location) 'src-tauri/binaries/cvc5-x86_64-pc-windows-msvc.exe'
cargo test --workspace --locked -- --test-threads=1
cargo clippy --workspace --all-targets --locked -- -D warnings
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
