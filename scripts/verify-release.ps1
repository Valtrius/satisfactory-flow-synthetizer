# Inspect both distributables without installing the app or touching user history.
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path $PSScriptRoot -Parent
$version = (Get-Content -LiteralPath (Join-Path $repoRoot 'src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json).version
$releaseRoot = Join-Path $repoRoot 'target/release'
$distribution = Join-Path $releaseRoot 'bundle/distribution'
$sevenZip = (Get-Command 7z -ErrorAction SilentlyContinue).Source
if (-not $sevenZip) { $sevenZip = Join-Path $env:ProgramFiles '7-Zip/7z.exe' }
if (-not (Test-Path -LiteralPath $sevenZip -PathType Leaf)) { throw '7-Zip is required to inspect the NSIS payload' }
& cargo build --release -p synthetizer-app --example verify_bundle --locked
if ($LASTEXITCODE -ne 0) { throw 'Cannot build the packaged solver check' }
$checkRoot = Join-Path $repoRoot "target/package-check-$([guid]::NewGuid())"
New-Item -ItemType Directory -Path $checkRoot | Out-Null
$portableRoot = Join-Path $checkRoot 'portable'
$installerRoot = Join-Path $checkRoot 'installer'
Expand-Archive -LiteralPath (Join-Path $distribution "satisfactory-flow-synthetizer_${version}_portable.zip") -DestinationPath $portableRoot
& $sevenZip x '-y' "-o$installerRoot" (Join-Path $distribution "satisfactory-flow-synthetizer_${version}_setup.exe") | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Cannot extract the NSIS payload' }

function Invoke-BundleCheck([string]$Directory, [bool]$ExpectSuccess) {
    $start = [Diagnostics.ProcessStartInfo]::new((Join-Path $Directory 'verify_bundle.exe'))
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $start.WorkingDirectory = $Directory
    $start.Environment.Remove('SOLVER_CVC5') | Out-Null
    $start.Environment.Remove('SOLVER_DIAGNOSTICS') | Out-Null
    $start.Environment['PATH'] = ''
    $start.Environment['LOCALAPPDATA'] = Join-Path $checkRoot 'empty-app-data'
    $process = [Diagnostics.Process]::Start($start)
    try {
        $stdout = $process.StandardOutput.ReadToEndAsync()
        $stderr = $process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit(30000)) { $process.Kill($true); $process.WaitForExit(); throw 'Packaged solver check timed out' }
        $output = $stdout.GetAwaiter().GetResult()
        $errors = $stderr.GetAwaiter().GetResult()
        if ($ExpectSuccess -and ($process.ExitCode -ne 0 -or $output.Trim() -ne 'Packaged solver OK: N=1 L=0')) { throw "Packaged solver check failed: $output $errors" }
        if (-not $ExpectSuccess -and $process.ExitCode -eq 0) { throw 'Missing-backend control unexpectedly succeeded via an external installation' }
    } finally { $process.Dispose() }
}
$metadata = Get-Content -LiteralPath (Join-Path $repoRoot 'src-tauri/resources/cvc5/package.json') -Raw | ConvertFrom-Json
$appBytes = [IO.File]::ReadAllBytes((Join-Path $releaseRoot 'satisfactory-flow-synthetizer.exe'))
$portableAppHash = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($appBytes))
# Tauri changes only this marker while bundling NSIS, then restores the build output.
$marker = '__TAURI_BUNDLE_TYPE_VAR_UNK'
$appText = [Text.Encoding]::Latin1.GetString($appBytes)
$markerOffset = $appText.IndexOf($marker, [StringComparison]::Ordinal)
if ($markerOffset -lt 0 -or $appText.LastIndexOf($marker, [StringComparison]::Ordinal) -ne $markerOffset) { throw 'Expected exactly one Tauri bundle type marker' }
[Text.Encoding]::ASCII.GetBytes('__TAURI_BUNDLE_TYPE_VAR_NSS').CopyTo($appBytes, $markerOffset)
$installerAppHash = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($appBytes))
foreach ($root in @($portableRoot, $installerRoot)) {
    $applications = @(Get-ChildItem -LiteralPath $root -Recurse -Filter satisfactory-flow-synthetizer.exe -File)
    if ($applications.Count -ne 1) { throw "Expected one application in $root" }
    $expectedAppHash = if ($root -eq $portableRoot) { $portableAppHash } else { $installerAppHash }
    if ((Get-FileHash -LiteralPath $applications[0].FullName).Hash -ne $expectedAppHash) { throw "Application checksum differs from the release build in $root" }
    $directory = $applications[0].DirectoryName
    $backend = Join-Path $directory 'cvc5.exe'
    if ((Get-FileHash -LiteralPath $backend -Algorithm SHA256).Hash.ToLowerInvariant() -ne $metadata.executableSha256) { throw "Backend checksum mismatch in $root" }
    $sourceResources = Join-Path $repoRoot 'src-tauri/resources/cvc5'
    foreach ($notice in (Get-ChildItem -LiteralPath $sourceResources -Recurse -File)) {
        $relative = [IO.Path]::GetRelativePath($sourceResources, $notice.FullName)
        $distributed = Join-Path $directory "licenses/cvc5/$relative"
        if ((Get-FileHash -LiteralPath $notice.FullName).Hash -ne (Get-FileHash -LiteralPath $distributed).Hash) { throw "Notice differs in package: $relative" }
    }
    Copy-Item -LiteralPath (Join-Path $releaseRoot 'examples/verify_bundle.exe') -Destination $directory
    Invoke-BundleCheck $directory $true
    Move-Item -LiteralPath $backend -Destination "$backend.disabled"
    try { Invoke-BundleCheck $directory $false } finally { Move-Item -LiteralPath "$backend.disabled" -Destination $backend }
    Remove-Item -LiteralPath (Join-Path $directory 'verify_bundle.exe')
    Write-Output "Verified $(Split-Path $root -Leaf): exact solve with no external backend, missing-backend control, checksums and notices."
}
Write-Output "Extracted verification payloads: $checkRoot"
