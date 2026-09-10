param(
    [Parameter(Mandatory)][string]$MatrixDirectory,
    [Parameter(Mandatory)][string]$VariantBinaryMap,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [string]$ValidationEvidence
)
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path $PSScriptRoot -Parent
$matrix = (Resolve-Path -LiteralPath $MatrixDirectory).Path
$binaryMap = Get-Content -LiteralPath $VariantBinaryMap -Raw | ConvertFrom-Json -AsHashtable
$campaign = Get-Content -LiteralPath (Join-Path $matrix 'campaign.json') -Raw | ConvertFrom-Json
foreach ($suite in $campaign.suites) {
    $manifest = Get-Content -LiteralPath (Join-Path $matrix $suite.manifest) -Raw | ConvertFrom-Json
    if (@($manifest.jobs | Where-Object { $_.Workers -lt 8 }).Count) { throw 'Performance campaigns require at least eight total workers' }
    $allowance = ($manifest.jobs | Measure-Object TimeoutSeconds -Sum).Sum + 15 * $manifest.jobs.Count
    if ($suite.max_scheduled_seconds -ne $allowance -or $manifest.MaxScheduledSeconds -ne $allowance) { throw "Suite allowance differs from its jobs: $($suite.name)" }
    if ($allowance + 120 -gt 10800) { throw "Suite exceeds the three-hour budget: $($suite.name)" }
}
$backend = Join-Path $repoRoot 'src-tauri/binaries/cvc5-x86_64-pc-windows-msvc.exe'
$backendHash = (Get-FileHash -LiteralPath $backend -Algorithm SHA256).Hash.ToLowerInvariant()
if ($ValidationEvidence) {
    $evidenceRoot = (Resolve-Path -LiteralPath $ValidationEvidence).Path
    $qualification = Get-Content -LiteralPath (Join-Path $evidenceRoot 'qualification.json') -Raw | ConvertFrom-Json
    if (-not $qualification.passed -or $qualification.benchmark -or $qualification.backend_sha256 -ne $backendHash) { throw 'Release evidence qualification is missing, failed, or uses another backend' }
    foreach ($variant in $binaryMap.Keys) {
        $actualHash = (Get-FileHash -LiteralPath (Join-Path $binaryMap[$variant] 'profile_solver.exe') -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($qualification.runner_hashes.$variant -ne $actualHash) { throw "Release evidence uses another binary: $variant" }
    }
}
$output = [IO.Path]::GetFullPath($OutputDirectory, $repoRoot)
if (Test-Path -LiteralPath $output) { throw 'Frozen campaign output already exists' }
New-Item -ItemType Directory -Path $output | Out-Null
foreach ($suite in $campaign.suites) {
    $suiteRoot = Join-Path $output $suite.name
    $map = [ordered]@{}
    foreach ($variant in @('baseline', $suite.candidate)) {
        $bin = $binaryMap[$variant]
        if (-not $bin) { throw "Missing prepared variant: $variant" }
        $variantRoot = Split-Path $bin -Parent
        $metadata = Get-Content -LiteralPath (Join-Path $variantRoot 'metadata.json') -Raw | ConvertFrom-Json
        if ($metadata.backend_sha256 -ne $backendHash) { throw "Prepared backend differs from variant validation: $variant" }
        foreach ($file in $metadata.files.PSObject.Properties) {
            if ((Get-FileHash -LiteralPath (Join-Path $variantRoot $file.Name) -Algorithm SHA256).Hash.ToLowerInvariant() -ne $file.Value) { throw "Changed prepared artifact: $variant/$($file.Name)" }
        }
        $map[$variant] = $bin
    }
    $mapPath = Join-Path $output "$($suite.name)-binaries.json"
    $map | ConvertTo-Json | Set-Content -LiteralPath $mapPath
    $savedBackend = $env:SOLVER_CVC5
    try {
        $env:SOLVER_CVC5 = $backend
        & (Join-Path $PSScriptRoot 'start-benchmark-screen.ps1') -JobManifest (Join-Path $matrix $suite.manifest) -VariantBinaryMap $mapPath -OutputDirectory $suiteRoot -CancellationGraceSeconds 15 -PlanOnly
    } finally { $env:SOLVER_CVC5 = $savedBackend }
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'start-optimization-suite.ps1'), (Join-Path $PSScriptRoot 'audit-optimization-results.py') -Destination $suiteRoot
    @{name=$suite.name;repository=$repoRoot;max_scheduled_seconds=$suite.max_scheduled_seconds;prepared_only=$true} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $suiteRoot 'optimization-suite.json')
    $hashes = [ordered]@{}
    foreach ($file in (Get-ChildItem -LiteralPath $suiteRoot -Recurse -File | Sort-Object FullName)) {
        if ($file.Name -in @('frozen-hashes.json','BENCHMARK-STATUS.txt')) { continue }
        $relative = [IO.Path]::GetRelativePath($suiteRoot, $file.FullName).Replace('\','/')
        $hashes[$relative] = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    $hashes | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $suiteRoot 'frozen-hashes.json')
}
$campaign | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath (Join-Path $output 'campaign.json')
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'start-optimization-campaign.ps1') -Destination $output
if ($ValidationEvidence) { Copy-Item -LiteralPath $evidenceRoot -Destination (Join-Path $output 'qualification') -Recurse }
$campaignHashes = [ordered]@{}
foreach ($relative in @('campaign.json','start-optimization-campaign.ps1') + @($campaign.suites | ForEach-Object { "$($_.name)/frozen-hashes.json" })) {
    $campaignHashes[$relative] = (Get-FileHash -LiteralPath (Join-Path $output $relative) -Algorithm SHA256).Hash.ToLowerInvariant()
}
if ($ValidationEvidence) {
    foreach ($file in (Get-ChildItem -LiteralPath (Join-Path $output 'qualification') -Recurse -File)) {
        $relative = [IO.Path]::GetRelativePath($output, $file.FullName).Replace('\','/')
        $campaignHashes[$relative] = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    }
}
$campaignHashes | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $output 'campaign-hashes.json')
'PREPARED ONLY: no timing suite launched. Start one suite at a time with scripts/start-optimization-suite.ps1.' | Set-Content -LiteralPath (Join-Path $output 'CAMPAIGN-STATUS.txt')
Write-Output "Frozen campaign: $output"
