# Download and verify the pinned Windows backend before Tauri copies its sidecar/resources.
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path $PSScriptRoot -Parent
$package = Get-Content -LiteralPath (Join-Path $repoRoot 'src-tauri/cvc5-package.json') -Raw | ConvertFrom-Json
$target = if ($env:TAURI_ENV_TARGET_TRIPLE) { $env:TAURI_ENV_TARGET_TRIPLE } else { (& rustc --print host-tuple).Trim() }
if (-not $IsWindows -or $target -ne $package.target) { throw "The bundled cvc5 package supports Windows x64 ($($package.target)); requested $target." }
$cacheRoot = [IO.Path]::GetFullPath((Join-Path $repoRoot 'target/cvc5-package'))
New-Item -ItemType Directory -Path $cacheRoot -Force | Out-Null
$archive = Join-Path $cacheRoot "cvc5-$($package.version)-$($package.sha256).zip"
function Get-Sha256([string]$Path) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
if (-not (Test-Path -LiteralPath $archive)) {
    $download = Join-Path $cacheRoot "$([guid]::NewGuid()).download"
    try {
        Invoke-WebRequest -Uri $package.url -OutFile $download
        if ((Get-Sha256 $download) -ne $package.sha256) { throw 'cvc5 archive checksum mismatch' }
        Move-Item -LiteralPath $download -Destination $archive
    } finally {
        if (Test-Path -LiteralPath $download) { Remove-Item -LiteralPath $download }
    }
}
if ((Get-Sha256 $archive) -ne $package.sha256) { throw "Cached cvc5 archive checksum mismatch: $archive" }

function Copy-VerifiedFile([string]$Source, [string]$Destination) {
    New-Item -ItemType Directory -Path (Split-Path $Destination -Parent) -Force | Out-Null
    if (-not (Test-Path -LiteralPath $Destination) -or (Get-Sha256 $Source) -ne (Get-Sha256 $Destination)) {
        Copy-Item -LiteralPath $Source -Destination $Destination -Force
    }
}
$stage = Join-Path $cacheRoot "extract-$([guid]::NewGuid())"
try {
    Expand-Archive -LiteralPath $archive -DestinationPath $stage
    $executables = @(Get-ChildItem -LiteralPath $stage -Recurse -Filter cvc5.exe -File)
    if ($executables.Count -ne 1) { throw 'Expected exactly one cvc5 executable in the pinned archive' }
    $executable = $executables[0].FullName
    $packageRoot = Split-Path (Split-Path $executable -Parent) -Parent
    $version = & $executable --version
    if ($LASTEXITCODE -ne 0 -or "$version" -notmatch "\bcvc5\s+$([regex]::Escape($package.version))\b") { throw 'cvc5 version verification failed' }
    $answer = '(set-logic QF_LRA)(declare-fun x () Real)(assert (= x 1))(check-sat)' | & $executable --lang=smt2
    if ($LASTEXITCODE -ne 0 -or "$answer".Trim() -ne 'sat') { throw 'cvc5 SMT-LIB verification failed' }
    $sidecar = Join-Path $repoRoot "src-tauri/binaries/cvc5-$($package.target).exe"
    $resources = Join-Path $repoRoot 'src-tauri/resources/cvc5'
    foreach ($name in @('COPYING', 'AUTHORS')) {
        Copy-VerifiedFile (Join-Path $packageRoot $name) (Join-Path $resources $name)
    }
    $licenseRoot = Join-Path $packageRoot 'licenses'
    $licenses = @(Get-ChildItem -LiteralPath $licenseRoot -File -Recurse)
    if (-not $licenses.Count) { throw 'cvc5 license files are missing' }
    foreach ($license in $licenses) {
        $relative = [IO.Path]::GetRelativePath($licenseRoot, $license.FullName)
        Copy-VerifiedFile $license.FullName (Join-Path $resources "licenses/$relative")
    }
    foreach ($notice in $package.notices) {
        $cachedNotice = Join-Path $cacheRoot $notice.sha256
        if (-not (Test-Path -LiteralPath $cachedNotice)) {
            $noticeDownload = Join-Path $cacheRoot "$([guid]::NewGuid()).download"
            try {
                Invoke-WebRequest -Uri $notice.url -OutFile $noticeDownload
                if ((Get-Sha256 $noticeDownload) -ne $notice.sha256) { throw "Dependency notice checksum mismatch: $($notice.name)" }
                Move-Item -LiteralPath $noticeDownload -Destination $cachedNotice
            } finally {
                if (Test-Path -LiteralPath $noticeDownload) { Remove-Item -LiteralPath $noticeDownload }
            }
        }
        if ((Get-Sha256 $cachedNotice) -ne $notice.sha256) { throw "Dependency notice checksum mismatch: $($notice.name)" }
        Copy-VerifiedFile $cachedNotice (Join-Path $resources "licenses/$($notice.name)")
    }
    Copy-VerifiedFile (Join-Path $repoRoot 'src-tauri/cvc5-SOURCES.txt') (Join-Path $resources 'SOURCES.txt')
    $metadata = [ordered]@{
        version = $package.version
        target = $package.target
        archiveUrl = $package.url
        archiveSha256 = $package.sha256
        executableSha256 = Get-Sha256 $executable
        sourceUrl = $package.sourceUrl
    } | ConvertTo-Json
    $metadataPath = Join-Path $resources 'package.json'
    if (-not (Test-Path -LiteralPath $metadataPath) -or (Get-Content -LiteralPath $metadataPath -Raw).Trim() -ne $metadata.Trim()) {
        $metadata | Set-Content -LiteralPath $metadataPath -Encoding utf8
    }
    Copy-VerifiedFile $executable $sidecar
    Write-Output "Prepared cvc5 $($package.version) ($target), verified SHA256 and SMT-LIB sat."
} finally {
    if (Test-Path -LiteralPath $stage) {
        $resolvedStage = (Resolve-Path -LiteralPath $stage).Path
        if (-not $resolvedStage.StartsWith($cacheRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Extraction cleanup escaped its cache directory' }
        Remove-Item -LiteralPath $resolvedStage -Recurse -Force
    }
}
