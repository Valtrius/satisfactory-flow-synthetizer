param(
    [string[]]$Stages = @('baseline', 'p1', 'p12', 'p123', 'p1234', 'shared', 'groups', 'p14', 'p124'),
    [ValidateSet('all', 'minimum_links', 'optimal')][string[]]$Modes = @('all', 'optimal'),
    [ValidateRange(1, 4096)][int[]]$Workers = @(1, 4, 16, 32),
    [ValidateRange(1, 100)][int]$Repeats = 3,
    [ValidateRange(1, 86400)][int]$TimeoutSeconds = 600,
    [ValidateRange(1, 600)][int]$CancellationGraceSeconds = 60,
    [string[]]$Cases = @('profile_40_25', 'profile_7_6_5_4_2'),
    [string]$JobManifest = '',
    [switch]$PlanOnly,
    [string]$BinaryDirectory = 'target/release/examples',
    [string]$ReferenceBinaryDirectory = '',
    [string]$VariantBinaryMap = '',
    [string[]]$ReferenceStages = @('baseline', 'p1234'),
    [ValidateSet('on', 'off')][string]$Hotspots = 'off',
    [int]$Seed = 270826,
    [string]$RepositoryRoot = '',
    [string]$OutputDirectory = "target/parallelism-ladder/runs-$(Get-Date -Format 'yyyyMMdd-HHmmss')"
)
$ErrorActionPreference = 'Stop'
$repoRoot = if ($RepositoryRoot) { [IO.Path]::GetFullPath($RepositoryRoot) } else { Split-Path $PSScriptRoot -Parent }
$outputRoot = [IO.Path]::GetFullPath($OutputDirectory, $repoRoot)
if (Test-Path -LiteralPath $outputRoot) { throw "Output already exists: $outputRoot" }
New-Item -ItemType Directory -Path $outputRoot | Out-Null
$variants = if ($VariantBinaryMap) {
    $variantMapPath = [IO.Path]::GetFullPath($VariantBinaryMap, $repoRoot)
    $variantMapRoot = Split-Path $variantMapPath -Parent
    $variantMapData = Get-Content -LiteralPath $variantMapPath -Raw | ConvertFrom-Json -AsHashtable
    if (-not $variantMapData.Count) { throw 'Empty variant binary map' }
    @($variantMapData.GetEnumerator() | ForEach-Object {
        if ($_.Key -notmatch '^[a-zA-Z0-9_-]+$' -or -not $_.Value) { throw 'Invalid binary variant map' }
        [pscustomobject]@{
            Name = $_.Key
            Directory = [IO.Path]::GetFullPath([string]$_.Value, $variantMapRoot)
            Stages = $Stages
        }
    })
} else {
    $legacyVariants = @([pscustomobject]@{ Name='after'; Directory=$BinaryDirectory; Stages=$Stages })
    if ($ReferenceBinaryDirectory) {
        $legacyVariants += [pscustomobject]@{ Name='before'; Directory=$ReferenceBinaryDirectory; Stages=$ReferenceStages }
    }
    $legacyVariants
}
$jobs = foreach ($variant in $variants) {
foreach ($case in $Cases) {
foreach ($mode in $Modes) {
    foreach ($worker in $Workers) {
        foreach ($stage in $variant.Stages) {
            foreach ($repeat in 1..$Repeats) {
                [pscustomobject]@{ Case=$case; Mode=$mode; Workers=$worker; Stage=$stage; Repeat=$repeat; Variant=$variant.Name; BinaryDirectory=$variant.Directory }
            }
        }
    }
}
}
}
$manifestRoot = $null
if ($JobManifest) {
    $manifestPath = [IO.Path]::GetFullPath($JobManifest, $repoRoot)
    $manifestRoot = Split-Path $manifestPath -Parent
    $jobs = @((Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json).jobs)
    if (-not $jobs.Count) { throw 'Empty job manifest' }
    foreach ($job in $jobs) {
        if ($job.Case -notmatch '^[a-zA-Z0-9_-]+$' -or $job.Stage -notin $Stages -or $job.Mode -notin @('all','minimum_links','optimal') -or $job.Cohort -notin @('reference','stress')) { throw 'Invalid job identity or mode' }
        if ($job.Workers -lt 1 -or $job.Workers -gt 4096 -or $job.Repeat -lt 1 -or $job.MaxNodes -lt 0 -or $job.TimeoutSeconds -lt 1 -or $job.TimeoutSeconds -gt 86400) { throw 'Invalid job budget' }
        if ($null -ne $job.Hotspots -and $job.Hotspots -notin @('on','off')) { throw 'Invalid job hotspot recording setting' }
        $casePath = [IO.Path]::GetFullPath($job.CaseFile, $manifestRoot)
        $caseData = Get-Content -LiteralPath $casePath -Raw | ConvertFrom-Json
        if ($caseData.problem.maxLinkRate -ne '1200') { throw 'File benchmark cases must use maxLinkRate 1200' }
        if ($null -eq $job.Variant) { $job | Add-Member NoteProperty Variant 'after' }
        $matchedVariant = @($variants | Where-Object { $_.Name -eq $job.Variant })
        if ($matchedVariant.Count -ne 1) {
            if ($job.Variant -eq 'before' -and -not $VariantBinaryMap) { throw 'Before jobs require ReferenceBinaryDirectory with a compatible profile_case executable' }
            throw 'Invalid binary variant'
        }
        $directory = $matchedVariant[0].Directory
        $job | Add-Member NoteProperty BinaryDirectory $directory
        $job | Add-Member NoteProperty CasePath $casePath
    }
    $identities = @($jobs | ForEach-Object { "$($_.Case)-$($_.Mode)-$($_.Variant)-$($_.Stage)-$($_.Workers)-$($_.Repeat)" })
    if (@($identities | Sort-Object -Unique).Count -ne $jobs.Count) { throw 'Duplicate job identities' }
    Copy-Item -LiteralPath $manifestPath -Destination (Join-Path $outputRoot 'job-manifest.json')
    $caseOutput = New-Item -ItemType Directory -Path (Join-Path $outputRoot 'cases')
    foreach ($casePath in ($jobs.CasePath | Sort-Object -Unique)) {
        Copy-Item -LiteralPath $casePath -Destination $caseOutput.FullName
    }
    foreach ($job in $jobs) { $job.CasePath = Join-Path $caseOutput.FullName (Split-Path $job.CasePath -Leaf) }
    $variants = @($variants | Where-Object { $_.Name -in $jobs.Variant })
}
$randomOrder = [Random]::new($Seed)
$jobs = @($jobs | Sort-Object { $randomOrder.Next() })
$jobs | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $outputRoot 'schedule.json')
if ($PlanOnly) { Write-Output "Validated $($jobs.Count) jobs; plan written to $outputRoot"; return }
@{ seed=$Seed; hotspots=$Hotspots; timeout_s=$TimeoutSeconds; cancellation_grace_s=$CancellationGraceSeconds; repeats=$Repeats } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $outputRoot 'settings.json')
git -C $repoRoot rev-parse HEAD | Set-Content -LiteralPath (Join-Path $outputRoot 'revision.txt')
git -C $repoRoot status --short | Set-Content -LiteralPath (Join-Path $outputRoot 'working-tree.txt')
rustc -Vv | Set-Content -LiteralPath (Join-Path $outputRoot 'rustc.txt')
Get-CimInstance Win32_Processor | Select-Object Name,NumberOfCores,NumberOfLogicalProcessors | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $outputRoot 'machine.json')
$binaryHashes = foreach ($variant in $variants) {
    $executables = if ($JobManifest) { @('profile_case') } else { $Cases }
    foreach ($case in $executables) {
        $binary = Join-Path ([IO.Path]::GetFullPath($variant.Directory, $repoRoot)) "$case.exe"
        $hash = Get-FileHash -LiteralPath $binary
        [pscustomobject]@{ variant=$variant.Name; case=$case; path=$hash.Path; sha256=$hash.Hash }
    }
}
$binaryHashes | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $outputRoot 'binaries.json')
$rows = [Collections.Generic.List[object]]::new()
$failedJobs = [Collections.Generic.List[object]]::new()
$index = 0
foreach ($job in $jobs) {
    $index++
    $name = "$($job.Case)-$($job.Mode)-$($job.Variant)-$($job.Stage)-w$($job.Workers)-r$($job.Repeat)"
    $executable = if ($JobManifest) { 'profile_case' } else { $job.Case }
    $exe = Join-Path ([IO.Path]::GetFullPath($job.BinaryDirectory, $repoRoot)) "$executable.exe"
    $json = Join-Path $outputRoot "$name.json"
    $maxNodes = if ($JobManifest) { $job.MaxNodes } else { switch ($job.Case) { 'profile_40_25' { 6 } 'profile_tiny' { 2 } default { 12 } } }
    $timeout = if ($JobManifest) { $job.TimeoutSeconds } else { $TimeoutSeconds }
    $jobHotspots = if ($JobManifest -and $null -ne $job.Hotspots) { $job.Hotspots } else { $Hotspots }
    $arguments = @($timeout, $job.Workers, $maxNodes, 'custom', $job.Stage, ('"' + $json + '"'), $job.Mode, $jobHotspots)
    if ($JobManifest) { $arguments += ('"' + $job.CasePath + '"') }
    Write-Output "Starting $index/$($jobs.Count) $name"
    @("RUNNING: $index / $($jobs.Count)", "Current: $name", "Time cap: $timeout seconds; N <= $maxNodes; hotspots $jobHotspots", "Cancellation cleanup watchdog: $CancellationGraceSeconds additional seconds", "Updated: $(Get-Date -Format o)") | Set-Content -LiteralPath (Join-Path $outputRoot 'BENCHMARK-STATUS.txt')
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $process = Start-Process -FilePath $exe -ArgumentList $arguments -WorkingDirectory $repoRoot -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $outputRoot "$name.log") -RedirectStandardError (Join-Path $outputRoot "$name.stderr.log")
    $null = $process.Handle
    $cpu = 0.0
    $peakWorkingSet = 0L
    $watchdogKilled = $false
    $processSamples = [Collections.Generic.List[object]]::new()
    $nextProcessSample = 0.0
    $lastWorkingSet = 0L
    $lastThreadCount = 0
    while (-not $process.WaitForExit(25)) {
        $process.Refresh()
        $cpu = [Math]::Max($cpu, $process.TotalProcessorTime.TotalSeconds)
        $peakWorkingSet = [Math]::Max($peakWorkingSet, $process.PeakWorkingSet64)
        $lastWorkingSet = $process.WorkingSet64
        $lastThreadCount = $process.Threads.Count
        if ($watch.Elapsed.TotalSeconds -ge $nextProcessSample) {
            $processSamples.Add([pscustomobject]@{
                elapsed_s = $watch.Elapsed.TotalSeconds
                cpu_s = $process.TotalProcessorTime.TotalSeconds
                working_set_bytes = $lastWorkingSet
                thread_count = $lastThreadCount
            })
            $nextProcessSample += 1.0
        }
        if ($watch.Elapsed.TotalSeconds -gt ($timeout + $CancellationGraceSeconds)) {
            $process.Kill()
            $process.WaitForExit()
            $watchdogKilled = $true
            break
        }
    }
    $process.WaitForExit()
    $watch.Stop()
    $cpu = [Math]::Max($cpu, $process.TotalProcessorTime.TotalSeconds)
    if ($watchdogKilled) {
        $failedJobs.Add([pscustomobject]@{
            name=$name; reason='cancellation_watchdog'; case=$job.Case; mode=$job.Mode
            variant=$job.Variant; stage=$job.Stage; workers=$job.Workers; timeout_s=$timeout
            hotspots=$jobHotspots
            cancellation_grace_s=$CancellationGraceSeconds; process_wall_s=$watch.Elapsed.TotalSeconds
            process_cpu_s=$cpu; sampled_peak_working_set_bytes=$peakWorkingSet
            diagnostic_only=$true; solver_result_available=$false
        })
        ConvertTo-Json -InputObject @($failedJobs.ToArray()) | Set-Content -LiteralPath (Join-Path $outputRoot 'failed-jobs.json')
        Write-Warning "Watchdog terminated $name. Failure preserved; continuing the remaining jobs."
        $process.Dispose()
        continue
    }
    $processSamples.Add([pscustomobject]@{
        elapsed_s = $watch.Elapsed.TotalSeconds
        cpu_s = $process.TotalProcessorTime.TotalSeconds
        working_set_bytes = $lastWorkingSet
        thread_count = $lastThreadCount
    })
    $processSamples | Export-Csv -LiteralPath (Join-Path $outputRoot "$name.process-samples.csv") -NoTypeInformation
    if ($process.ExitCode -ne 0 -or -not (Test-Path -LiteralPath $json)) { throw "Benchmark failed: $name" }
    $data = Get-Content -LiteralPath $json -Raw | ConvertFrom-Json
    if ($data.mode -ne $job.Mode -or $data.stage -ne $job.Stage -or $data.workers -ne $job.Workers) { throw "Executable ignored benchmark settings: $name" }
    if ($data.max_nodes -ne $maxNodes -or $data.timeout_s -ne $timeout) { throw "Executable ignored benchmark budgets: $name" }
    if ($data.hotspot_recording -ne ($jobHotspots -eq 'on')) { throw "Executable ignored hotspot recording setting: $name" }
    if ($JobManifest) {
        $expected = Get-Content -LiteralPath $job.CasePath -Raw | ConvertFrom-Json
        # Compare exact Rational parsing in the executable; JSON decimals may normalize to fractions.
        if ($data.case -ne $expected.name -or $data.problem.maxLinkRate -ne '1200') { throw "Executable ignored benchmark case: $name" }
    }
    $row = [pscustomobject]@{
        case=$job.Case; mode=$job.Mode; variant=$job.Variant; stage=$job.Stage; workers=$job.Workers; repeat=$job.Repeat
        status=$data.status; layouts=$data.layouts; wall_s=$data.wall_s
        max_nodes=$maxNodes; timeout_s=$timeout; cohort=$(if ($JobManifest) { $job.Cohort } else { 'reference' })
        first_valid_s=$data.first_valid_s; hotspot_recording=$data.hotspot_recording
        process_cpu_s=$cpu; cpu_utilization=($cpu / $watch.Elapsed.TotalSeconds / $job.Workers)
        accounted_timer_s=$data.accounted_timer_s; sampled_peak_working_set_bytes=$peakWorkingSet
        process_wall_s=$watch.Elapsed.TotalSeconds; result_file=$json
    }
    $rows.Add($row)
    $rows | Export-Csv -LiteralPath (Join-Path $outputRoot 'results.csv') -NoTypeInformation
    Write-Output "$name status=$($data.status) layouts=$($data.layouts) wall=$($data.wall_s)s cpu=$cpu"
    $process.Dispose()
}
Write-Output "Results: $outputRoot"
