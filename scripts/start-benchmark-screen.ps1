param(
    [Parameter(Mandatory)][string]$JobManifest,
    [string]$OutputDirectory = "target/solver-screen-$(Get-Date -Format 'yyyyMMdd-HHmmss')",
    [string]$VariantBinaryMap = '',
    [string]$RepositoryRoot = '',
    [ValidateRange(1, 600)][int]$CancellationGraceSeconds = 60,
    [switch]$PlanOnly,
    [switch]$Run
)
$ErrorActionPreference = 'Stop'
$repoRoot = if ($RepositoryRoot) { [IO.Path]::GetFullPath($RepositoryRoot) } else { Split-Path $PSScriptRoot -Parent }
$runRoot = [IO.Path]::GetFullPath($OutputDirectory, $repoRoot)
$manifestPath = [IO.Path]::GetFullPath($JobManifest, $repoRoot)
$suite = Join-Path $runRoot 'results'
$status = Join-Path $runRoot 'BENCHMARK-STATUS.txt'
if (-not $Run) {
    if (Test-Path -LiteralPath $runRoot) { throw "Output already exists: $runRoot" }
    $profileName = 'profile_solver.exe'
    $binary = Join-Path $repoRoot ('target/release/examples/' + $profileName)
    if (-not $VariantBinaryMap -and -not (Test-Path -LiteralPath $binary -PathType Leaf)) { throw 'Build profile_solver in release mode first.' }
    New-Item -ItemType Directory -Path $runRoot | Out-Null
    $bin = New-Item -ItemType Directory -Path (Join-Path $runRoot 'bin')
    $frozenVariantMap = ''
    if ($VariantBinaryMap) {
        $variantMapPath = [IO.Path]::GetFullPath($VariantBinaryMap, $repoRoot)
        $variantMapData = Get-Content -LiteralPath $variantMapPath -Raw | ConvertFrom-Json -AsHashtable
        if (-not $variantMapData.Count) { throw 'Empty variant binary map' }
        $frozenVariantData = [ordered]@{}
        foreach ($entry in $variantMapData.GetEnumerator()) {
            if ($entry.Key -notmatch '^[a-zA-Z0-9_-]+$' -or -not $entry.Value) { throw 'Invalid binary variant map' }
            $sourceDirectory = [IO.Path]::GetFullPath([string]$entry.Value, (Split-Path $variantMapPath -Parent))
            $sourceBinary = Join-Path $sourceDirectory $profileName
            if (-not (Test-Path -LiteralPath $sourceBinary -PathType Leaf)) { throw "Variant binary is missing: $($entry.Key)" }
            $frozenDirectory = New-Item -ItemType Directory -Path (Join-Path $runRoot "variant-bin/$($entry.Key)")
            Copy-Item -LiteralPath $sourceBinary -Destination (Join-Path $frozenDirectory.FullName 'profile_solver.exe')
            $frozenVariantData[$entry.Key] = $frozenDirectory.FullName

            $variantRoot = Split-Path $sourceDirectory -Parent
            $variantSource = Join-Path $variantRoot 'solver-source'
            if (Test-Path -LiteralPath $variantSource -PathType Container) {
                $frozenSourceRoot = New-Item -ItemType Directory -Path (Join-Path $runRoot 'variant-source') -Force
                Copy-Item -LiteralPath $variantSource -Destination (Join-Path $frozenSourceRoot.FullName $entry.Key) -Recurse
            }
            $variantMetadata = Join-Path $variantRoot 'metadata.json'
            if (Test-Path -LiteralPath $variantMetadata -PathType Leaf) {
                $metadataDirectory = New-Item -ItemType Directory -Path (Join-Path $runRoot 'variant-metadata') -Force
                Copy-Item -LiteralPath $variantMetadata -Destination (Join-Path $metadataDirectory.FullName "$($entry.Key).json")
            }
        }
        $frozenVariantMap = Join-Path $runRoot 'variant-binaries.json'
        $frozenVariantData | ConvertTo-Json | Set-Content -LiteralPath $frozenVariantMap
    } else {
        Copy-Item -LiteralPath $binary -Destination (Join-Path $bin.FullName 'profile_solver.exe')
    }
    & {
        $backend = if ($env:SOLVER_CVC5) { $env:SOLVER_CVC5 } else { (Get-Command cvc5 -ErrorAction SilentlyContinue).Source }
        if (-not $backend) { $backend = Join-Path $env:LOCALAPPDATA 'Programs/cvc5/bin/cvc5.exe' }
        if (-not (Test-Path -LiteralPath $backend -PathType Leaf)) { throw 'cvc5 is required for solver screening' }
        $destinations = if ($frozenVariantMap) { @($frozenVariantData.Values) } else { @($bin.FullName) }
        foreach ($destination in $destinations) {
            Copy-Item -LiteralPath $backend -Destination (Join-Path $destination 'cvc5.exe')
            Get-ChildItem -LiteralPath (Split-Path $backend -Parent) -Filter '*.dll' | Copy-Item -Destination $destination
        }
        & $backend --version | Set-Content -LiteralPath (Join-Path $runRoot 'cvc5-version.txt')
    }
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'analyze-benchmarks.py') -Destination $runRoot
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'run-benchmark-screen.ps1') -Destination $runRoot
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'benchmark-affinity.ps1'),(Join-Path $PSScriptRoot 'benchmark_policy.py') -Destination $runRoot
    Copy-Item -LiteralPath $PSCommandPath -Destination $runRoot
    # Validate and freeze the manifest and cases before detaching. The child uses
    # these copies and the copied scripts even if the working tree changes later.
    $planRoot = Join-Path $runRoot 'input'
    & (Join-Path $runRoot 'run-benchmark-screen.ps1') -RepositoryRoot $repoRoot -JobManifest $manifestPath -BinaryDirectory $bin.FullName -VariantBinaryMap $frozenVariantMap -OutputDirectory $planRoot -CancellationGraceSeconds $CancellationGraceSeconds -PlanOnly
    $manifestPath = Join-Path $planRoot 'job-manifest.json'
    $frozenManifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    foreach ($job in $frozenManifest.jobs) { $job.CaseFile = 'cases/' + (Split-Path $job.CaseFile -Leaf) }
    $frozenManifest | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $manifestPath
    $source = New-Item -ItemType Directory -Path (Join-Path $runRoot 'solver-source')
    foreach ($crate in @('solver-core','solver-api','solver-validation','solver-reference','synthetizer-app')) {
        Copy-Item -LiteralPath (Join-Path $repoRoot "crates/$crate") -Destination (Join-Path $source.FullName $crate) -Recurse
    }
    Copy-Item -LiteralPath (Join-Path $repoRoot 'Cargo.lock'),(Join-Path $repoRoot 'Cargo.toml') -Destination $source.FullName
    git -C $repoRoot diff --binary | Set-Content -LiteralPath (Join-Path $runRoot 'working-tree.diff')
    $frozenHashes = [ordered]@{}
    foreach ($file in (Get-ChildItem -LiteralPath $runRoot -Recurse -File | Sort-Object FullName)) {
        $relative = [IO.Path]::GetRelativePath($runRoot, $file.FullName).Replace('\', '/')
        $frozenHashes[$relative] = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    $frozenHashes | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $runRoot 'frozen-hashes.json')
    if ($PlanOnly) { 'PREPARED ONLY: no benchmark launched.' | Set-Content -LiteralPath $status; return }
    'STARTING. See results/BENCHMARK-STATUS.txt for the current job. A completion dialog will appear after verification.' | Set-Content -LiteralPath $status
    $shell = (Get-Process -Id $PID).Path
    $arguments = @('-NoProfile','-STA','-File',('"'+(Join-Path $runRoot 'start-benchmark-screen.ps1')+'"'),'-Run','-RepositoryRoot',('"'+$repoRoot+'"'),'-JobManifest',('"'+$manifestPath+'"'),'-OutputDirectory',('"'+$runRoot+'"'))
    if ($frozenVariantMap) { $arguments += @('-VariantBinaryMap', ('"'+$frozenVariantMap+'"')) }
    $arguments += @('-CancellationGraceSeconds', $CancellationGraceSeconds)
    $child = Start-Process -FilePath $shell -ArgumentList $arguments -WorkingDirectory $repoRoot -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $runRoot 'runner.log') -RedirectStandardError (Join-Path $runRoot 'runner.stderr.log')
    $child.Id | Set-Content -LiteralPath (Join-Path $runRoot 'runner.pid')
    Write-Output "Started benchmark runner PID $($child.Id)."
    Write-Output "Status: $status"
    Write-Output "Progress: $(Join-Path $suite 'BENCHMARK-STATUS.txt')"
    return
}

$allJobsAttempted = $false
try {
    Set-Location -LiteralPath $repoRoot
    'RUNNING: see results/BENCHMARK-STATUS.txt for the current solver job.' | Set-Content -LiteralPath $status
    & (Join-Path $runRoot 'run-benchmark-screen.ps1') -RepositoryRoot $repoRoot -JobManifest $manifestPath -BinaryDirectory (Join-Path $runRoot 'bin') -VariantBinaryMap $VariantBinaryMap -OutputDirectory $suite -CancellationGraceSeconds $CancellationGraceSeconds
    if (-not $?) { throw 'Benchmark runner failed.' }
    $allJobsAttempted = $true
    'VERIFYING: solver runs finished; checking identities and proof status.' | Set-Content -LiteralPath $status
    & python (Join-Path $runRoot 'analyze-benchmarks.py') $suite --allow-incomplete --output (Join-Path $suite 'summary.json') > (Join-Path $runRoot 'verification.log') 2>&1
    if ($LASTEXITCODE -ne 0) { throw 'Verification failed; see verification.log and results/summary.json.' }
    $message = @('FINISHED: screening and verification completed.', "Finished: $(Get-Date -Format o)", 'Capped runs may be incomplete. This does not mean every solve finished.', 'Message the Codex task to analyze the results.', "Results: $suite")
    $message | Set-Content -LiteralPath (Join-Path $runRoot 'BENCHMARK-FINISHED.txt')
    $message | Set-Content -LiteralPath $status
    $message | Set-Content -LiteralPath (Join-Path $suite 'BENCHMARK-STATUS.txt')
    $title = 'Solver benchmark screening finished'
} catch {
    $headline = if ($allJobsAttempted) { 'FINISHED WITH FAILURES: all solver jobs attempted; verification did not pass.' } else { 'FAILED OR INTERRUPTED' }
    $message = @($headline, $_.Exception.Message, "Updated: $(Get-Date -Format o)", 'Message the Codex task with this status file.')
    $message | Set-Content -LiteralPath (Join-Path $runRoot 'BENCHMARK-FAILED.txt')
    $message | Set-Content -LiteralPath $status
    $title = 'Solver benchmark screening needs attention'
}
# Status files are authoritative even if Windows cannot display the dialog.
try {
    Add-Type -AssemblyName System.Windows.Forms
    [System.Media.SystemSounds]::Asterisk.Play()
    [System.Windows.Forms.MessageBox]::Show(($message -join [Environment]::NewLine), $title) | Out-Null
} catch {
    Write-Warning 'Completion dialog unavailable; use BENCHMARK-STATUS.txt.'
}
