param(
    [string]$JobManifest = 'benchmarks/custom/screening.json',
    [string]$OutputDirectory = "target/parallelism-ladder/screen-$(Get-Date -Format 'yyyyMMdd-HHmmss')",
    [ValidateSet('on', 'off')][string]$Hotspots = 'off',
    [string]$WitnessResult = '',
    [string]$ReferenceReplayBinary = '',
    [string]$ReferenceBinaryDirectory = '',
    [string]$VariantBinaryMap = '',
    [string]$RepositoryRoot = '',
    [ValidateRange(1, 5)][int]$ReplayRepeats = 1,
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
    $engineJobs = @((Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json).jobs | Where-Object { $_.Engine })
    $profileName = if ($engineJobs.Count) { 'profile_engines.exe' } else { 'profile_case.exe' }
    $binary = Join-Path $repoRoot ('target/release/examples/' + $profileName)
    if (-not $VariantBinaryMap -and -not (Test-Path -LiteralPath $binary -PathType Leaf)) { throw 'Build profile_case in release mode first.' }
    if ($VariantBinaryMap -and $ReferenceBinaryDirectory) { throw 'Use either VariantBinaryMap or ReferenceBinaryDirectory, not both.' }
    if ($ReferenceBinaryDirectory) {
        $referenceCaseBinary = Join-Path ([IO.Path]::GetFullPath($ReferenceBinaryDirectory, $repoRoot)) 'profile_case.exe'
        if (-not (Test-Path -LiteralPath $referenceCaseBinary -PathType Leaf)) { throw 'Reference profile_case executable is missing.' }
    }
    if ($WitnessResult) {
        $witnessBinary = Join-Path $repoRoot 'target/release/examples/profile_witness.exe'
        $witnessPath = [IO.Path]::GetFullPath($WitnessResult, $repoRoot)
        if (-not (Test-Path -LiteralPath $witnessBinary -PathType Leaf)) { throw 'Build profile_witness in release mode first.' }
        $saved = Get-Content -LiteralPath $witnessPath -Raw | ConvertFrom-Json
        if ($saved.outcome.kind -ne 'optimal' -or $saved.validated -ne $true) { throw 'Replay requires a validated optimal saved result.' }
    }
    if ($ReferenceReplayBinary) {
        if (-not $WitnessResult) { throw 'Reference replay needs a saved witness.' }
        $referenceReplayPath = [IO.Path]::GetFullPath($ReferenceReplayBinary, $repoRoot)
        if (-not (Test-Path -LiteralPath $referenceReplayPath -PathType Leaf)) { throw 'Reference replay executable is missing.' }
    }
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
            Copy-Item -LiteralPath $sourceBinary -Destination (Join-Path $frozenDirectory.FullName 'profile_case.exe')
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
        Copy-Item -LiteralPath $binary -Destination (Join-Path $bin.FullName 'profile_case.exe')
    }
    if (@($engineJobs | Where-Object { $_.Engine -eq 'astra' }).Count) {
        $backend = if ($env:ASTRA_CVC5) { $env:ASTRA_CVC5 } else { (Get-Command cvc5 -ErrorAction SilentlyContinue).Source }
        if (-not $backend) { $backend = Join-Path $env:LOCALAPPDATA 'Programs/cvc5/bin/cvc5.exe' }
        if (-not (Test-Path -LiteralPath $backend -PathType Leaf)) { throw 'cvc5 is required for Astra screening' }
        $destinations = if ($frozenVariantMap) { @($frozenVariantData.Values) } else { @($bin.FullName) }
        foreach ($destination in $destinations) {
            Copy-Item -LiteralPath $backend -Destination (Join-Path $destination 'cvc5.exe')
            Get-ChildItem -LiteralPath (Split-Path $backend -Parent) -Filter '*.dll' | Copy-Item -Destination $destination
        }
        & $backend --version | Set-Content -LiteralPath (Join-Path $runRoot 'cvc5-version.txt')
    }
    if ($ReferenceBinaryDirectory) {
        $referenceBin = New-Item -ItemType Directory -Path (Join-Path $runRoot 'reference-bin')
        Copy-Item -LiteralPath $referenceCaseBinary -Destination $referenceBin.FullName
        Get-FileHash -LiteralPath $referenceCaseBinary | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $runRoot 'reference-origin-hash.json')
    }
    if ($WitnessResult) {
        Copy-Item -LiteralPath $witnessBinary -Destination $bin.FullName
        Copy-Item -LiteralPath $witnessPath -Destination (Join-Path $runRoot 'witness-input.json')
        Get-FileHash -LiteralPath (Join-Path $bin.FullName 'profile_witness.exe'), (Join-Path $runRoot 'witness-input.json') | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $runRoot 'replay-hashes.json')
    }
    if ($ReferenceReplayBinary) {
        Copy-Item -LiteralPath $referenceReplayPath -Destination (Join-Path $bin.FullName 'profile_witness-before.exe')
        Get-FileHash -LiteralPath (Join-Path $bin.FullName 'profile_witness-before.exe') | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $runRoot 'reference-replay-hash.json')
    }
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'analyze-parallelism.py') -Destination $runRoot
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'parallelism-ladder.ps1') -Destination $runRoot
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'benchmark-affinity.ps1'),(Join-Path $PSScriptRoot 'benchmark_policy.py') -Destination $runRoot
    Copy-Item -LiteralPath $PSCommandPath -Destination $runRoot
    # Validate and freeze the manifest and cases before detaching. The child uses
    # these copies and the copied scripts even if the working tree changes later.
    $planRoot = Join-Path $runRoot 'input'
    & (Join-Path $runRoot 'parallelism-ladder.ps1') -RepositoryRoot $repoRoot -JobManifest $manifestPath -BinaryDirectory $bin.FullName -ReferenceBinaryDirectory $ReferenceBinaryDirectory -VariantBinaryMap $frozenVariantMap -OutputDirectory $planRoot -Hotspots $Hotspots -CancellationGraceSeconds $CancellationGraceSeconds -PlanOnly
    $manifestPath = Join-Path $planRoot 'job-manifest.json'
    $frozenManifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    foreach ($job in $frozenManifest.jobs) { $job.CaseFile = 'cases/' + (Split-Path $job.CaseFile -Leaf) }
    $frozenManifest | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $manifestPath
    $source = New-Item -ItemType Directory -Path (Join-Path $runRoot 'solver-source')
    Copy-Item -LiteralPath (Join-Path $repoRoot 'crates/solver-core/src') -Destination $source.FullName -Recurse
    Copy-Item -LiteralPath (Join-Path $repoRoot 'crates/solver-core/examples') -Destination $source.FullName -Recurse
    if ($engineJobs.Count) {
        foreach ($crate in @('solver-astra','solver-api','solver-validation','solver-z3','synthetizer-app')) {
            Copy-Item -LiteralPath (Join-Path $repoRoot "crates/$crate") -Destination (Join-Path $source.FullName $crate) -Recurse
        }
        Copy-Item -LiteralPath (Join-Path $repoRoot 'Cargo.lock'),(Join-Path $repoRoot 'Cargo.toml') -Destination $source.FullName
    }
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
    $arguments = @('-NoProfile','-STA','-File',('"'+(Join-Path $runRoot 'start-benchmark-screen.ps1')+'"'),'-Run','-RepositoryRoot',('"'+$repoRoot+'"'),'-JobManifest',('"'+$manifestPath+'"'),'-OutputDirectory',('"'+$runRoot+'"'),'-Hotspots',$Hotspots)
    if ($ReferenceBinaryDirectory) { $arguments += @('-ReferenceBinaryDirectory', ('"'+$referenceBin.FullName+'"')) }
    if ($frozenVariantMap) { $arguments += @('-VariantBinaryMap', ('"'+$frozenVariantMap+'"')) }
    if ($WitnessResult) { $arguments += @('-WitnessResult', ('"'+(Join-Path $runRoot 'witness-input.json')+'"')) }
    $arguments += @('-ReplayRepeats', $ReplayRepeats)
    $arguments += @('-CancellationGraceSeconds', $CancellationGraceSeconds)
    if ($ReferenceReplayBinary) { $arguments += @('-ReferenceReplayBinary', ('"'+(Join-Path $bin.FullName 'profile_witness-before.exe')+'"')) }
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
    if ($WitnessResult) {
        $afterBinary = Join-Path $runRoot 'bin/profile_witness.exe'
        $replays = @(
            foreach ($repeat in 1..$ReplayRepeats) {
                if ($ReferenceReplayBinary) { @{ Name="witness-before-r$repeat"; Binary=$ReferenceReplayBinary; CancelMs=0; WatchdogSeconds=180 } }
                @{ Name="witness-after-r$repeat"; Binary=$afterBinary; CancelMs=0; WatchdogSeconds=180 }
            }
            @{ Name='witness-cancel'; Binary=$afterBinary; CancelMs=200; WatchdogSeconds=20 }
        )
        $replayResults = @()
        foreach ($replay in $replays) {
            "REPLAY: $($replay.Name), watchdog $($replay.WatchdogSeconds) seconds. Solver screen follows." | Set-Content -LiteralPath $status
            $resultFile = Join-Path $runRoot "$($replay.Name).json"
            $process = Start-Process -FilePath $replay.Binary -ArgumentList @(('"'+$WitnessResult+'"'), ('"'+$resultFile+'"'), $replay.CancelMs) -WorkingDirectory $repoRoot -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $runRoot "$($replay.Name).log") -RedirectStandardError (Join-Path $runRoot "$($replay.Name).stderr.log")
            $null = $process.Handle
            $process.Id | Set-Content -LiteralPath (Join-Path $runRoot "$($replay.Name).pid")
            if (-not $process.WaitForExit($replay.WatchdogSeconds * 1000)) {
                Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
                throw "$($replay.Name) exceeded its process watchdog; no completion claimed."
            }
            $process.WaitForExit()
            if ($process.ExitCode -ne 0) { throw "$($replay.Name) failed with exit $($process.ExitCode). See its stderr log." }
            $result = Get-Content -LiteralPath $resultFile -Raw | ConvertFrom-Json
            if ($result.validated -ne $true) { throw 'Replay validation failed.' }
            if ($replay.CancelMs -eq 0 -and ($result.completed -ne $true -or $result.key_equal -ne $true)) { throw 'Full replay changed or lost the authoritative witness.' }
            if ($replay.CancelMs -ne 0 -and ($result.cancelled -ne $true -or $result.completed -ne $false -or $null -ne $result.key_equal)) { throw 'Cancellation replay did not discard the interrupted canonical result.' }
            if ($replay.CancelMs -eq 0) {
                $replayResults += [pscustomobject]@{ name=$replay.Name; canonical_s=$result.canonical_s; branches=$result.hotspots.witness_branches; leaves=$result.hotspots.witness_leaves; key_equal=$result.key_equal; file=$resultFile }
            }
        }
        if (@($replayResults.branches | Sort-Object -Unique).Count -ne 1 -or @($replayResults.leaves | Sort-Object -Unique).Count -ne 1) { throw 'Full replay permutation counts changed.' }
        $replayResults | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $runRoot 'replay-summary.json')
    }
    'RUNNING: see results/BENCHMARK-STATUS.txt for the current solver job.' | Set-Content -LiteralPath $status
    & (Join-Path $runRoot 'parallelism-ladder.ps1') -RepositoryRoot $repoRoot -JobManifest $manifestPath -BinaryDirectory (Join-Path $runRoot 'bin') -ReferenceBinaryDirectory $ReferenceBinaryDirectory -VariantBinaryMap $VariantBinaryMap -OutputDirectory $suite -Hotspots $Hotspots -CancellationGraceSeconds $CancellationGraceSeconds
    if (-not $?) { throw 'Benchmark runner failed.' }
    $allJobsAttempted = $true
    'VERIFYING: solver runs finished; checking identities and proof status.' | Set-Content -LiteralPath $status
    & python (Join-Path $runRoot 'analyze-parallelism.py') $suite --allow-incomplete --output (Join-Path $suite 'summary.json') > (Join-Path $runRoot 'verification.log') 2>&1
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
