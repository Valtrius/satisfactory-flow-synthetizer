# Assemble exactly the application, verified backend and notices shipped by Tauri.
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path $PSScriptRoot -Parent
$version = (Get-Content -LiteralPath (Join-Path $repoRoot 'src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json).version
$releaseRoot = [IO.Path]::GetFullPath((Join-Path $repoRoot 'target/release'))
$binary = Join-Path $releaseRoot 'satisfactory-flow-synthetizer.exe'
$backend = Join-Path $releaseRoot 'cvc5.exe'
$notices = Join-Path $repoRoot 'src-tauri/resources/cvc5'
$metadata = Get-Content -LiteralPath (Join-Path $notices 'package.json') -Raw | ConvertFrom-Json
$package = Get-Content -LiteralPath (Join-Path $repoRoot 'src-tauri/cvc5-package.json') -Raw | ConvertFrom-Json
if ($metadata.version -ne $package.version -or $metadata.archiveSha256 -ne $package.sha256) { throw 'Built backend metadata differs from the pinned package; rebuild the app.' }
if ((Get-FileHash -LiteralPath $backend -Algorithm SHA256).Hash.ToLowerInvariant() -ne $metadata.executableSha256) { throw 'Built cvc5 checksum differs from verified package' }
$installers = @(Get-ChildItem -LiteralPath (Join-Path $releaseRoot 'bundle/nsis') -Filter "*_${version}_x64-setup.exe" -File)
if ($installers.Count -ne 1) { throw 'Expected exactly one matching Windows x64 NSIS installer' }
$output = Join-Path $releaseRoot 'bundle/distribution'
New-Item -ItemType Directory -Path $output -Force | Out-Null
$stage = Join-Path $releaseRoot "portable-$([guid]::NewGuid())"
try {
    New-Item -ItemType Directory -Path (Join-Path $stage 'licenses') -Force | Out-Null
    Copy-Item -LiteralPath $binary, $backend -Destination $stage
    Copy-Item -LiteralPath (Join-Path $repoRoot 'LICENSE') -Destination (Join-Path $stage 'LICENSE.txt')
    Copy-Item -LiteralPath $notices -Destination (Join-Path $stage 'licenses/cvc5') -Recurse
    @'
Extract the entire ZIP into a folder, then run satisfactory-flow-synthetizer.exe.
Keep cvc5.exe and the licenses folder beside the application. No cvc5 installation is required.
The app uses Microsoft Edge WebView2. The installer can install that runtime when necessary.
'@ | Set-Content -LiteralPath (Join-Path $stage 'README.txt') -Encoding utf8
    $portable = Join-Path $output "satisfactory-flow-synthetizer_${version}_portable.zip"
    if (Test-Path -LiteralPath $portable) { Remove-Item -LiteralPath $portable }
    [IO.Compression.ZipFile]::CreateFromDirectory($stage, $portable)
    $installer = Join-Path $output "satisfactory-flow-synthetizer_${version}_setup.exe"
    Copy-Item -LiteralPath $installers[0].FullName -Destination $installer -Force
    foreach ($artifact in @($portable, $installer)) {
        $hash = (Get-FileHash -LiteralPath $artifact -Algorithm SHA256).Hash.ToLowerInvariant()
        "$hash  $([IO.Path]::GetFileName($artifact))" | Set-Content -LiteralPath "$artifact.sha256" -Encoding utf8
        Write-Output $artifact
    }
} finally {
    if (Test-Path -LiteralPath $stage) {
        $resolvedStage = (Resolve-Path -LiteralPath $stage).Path
        if (-not $resolvedStage.StartsWith($releaseRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Portable cleanup escaped the release directory' }
        Remove-Item -LiteralPath $resolvedStage -Recurse -Force
    }
}
