param(
    [Parameter(Mandatory)][string]$JobManifest,
    [string]$BinaryDirectory = 'target/release/examples',
    [string]$VariantBinaryMap = '',
    [ValidateRange(1, 600)][int]$CancellationGraceSeconds = 60,
    [int]$Seed = 270826,
    [string]$RepositoryRoot = '',
    [Parameter(Mandatory)][string]$OutputDirectory,
    [switch]$PlanOnly
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'benchmark-affinity.ps1')
$repoRoot = if ($RepositoryRoot) { [IO.Path]::GetFullPath($RepositoryRoot) } else { Split-Path $PSScriptRoot -Parent }
$outputRoot = [IO.Path]::GetFullPath($OutputDirectory, $repoRoot)
if (Test-Path -LiteralPath $outputRoot) { throw "Output already exists: $outputRoot" }
New-Item -ItemType Directory -Path $outputRoot | Out-Null
Save-BenchmarkTopology (Join-Path $outputRoot 'topology.json')
$topology = Get-Content -LiteralPath (Join-Path $outputRoot 'topology.json') -Raw | ConvertFrom-Json
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
        }
    })
} else { @([pscustomobject]@{ Name='after'; Directory=$BinaryDirectory }) }
$manifestRoot = $null
if ($JobManifest) {
    $manifestPath = [IO.Path]::GetFullPath($JobManifest, $repoRoot)
    $manifestRoot = Split-Path $manifestPath -Parent
    $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    if ($manifest.RunnerProtocol -ne 'layout-v1') { throw 'RunnerProtocol layout-v1 is required; recorded manifests must use their matching frozen harness.' }
    $jobs = @($manifest.jobs)
    if (-not $jobs.Count) { throw 'Empty job manifest' }
    foreach ($job in $jobs) {
        if ($job.Case -notmatch '^[a-zA-Z0-9_-]+$' -or $job.Stage -ne 'baseline' -or $job.Mode -notin @('all_min_n','all_min_nl','one_min_nl') -or $job.Cohort -notin @('reference','stress')) { throw 'Invalid job identity or mode' }
        if ($job.Workers -lt 1 -or $job.Workers -gt 4096 -or $job.Repeat -lt 1 -or $job.MaxNodes -lt 0 -or $job.TimeoutSeconds -lt 1 -or $job.TimeoutSeconds -gt 86400) { throw 'Invalid job budget' }
        if ($null -ne $job.Hotspots -and $job.Hotspots -ne 'off') { throw 'Invalid job hotspot recording setting' }
        if ($null -ne $job.ProcessorAffinity -and $job.ProcessorAffinity -isnot [string]) { throw 'Affinity must be a string' }
        $mask = ConvertTo-BenchmarkAffinityMask $job.ProcessorAffinity
        $job | Add-Member NoteProperty ProcessorAffinity $mask -Force
        $selected = if ($mask) { [Convert]::ToInt64($mask, 16) } else { [Convert]::ToInt64($topology.available_mask, 16) }
        if (($mask -or $job.PairId) -and $job.Workers -gt [System.Numerics.BitOperations]::PopCount([ulong]$selected)) { throw 'Worker count exceeds selected logical CPUs' }
        if ($null -ne $job.CacheBytes) {
            $matchedCache = @($topology.relationships | Where-Object { $_.relation -eq 2 -and $_.level -eq 3 -and $_.cache_bytes -eq $job.CacheBytes -and $_.groups.Count -eq 1 -and $_.groups[0].group -eq 0 -and $_.groups[0].mask -eq $mask })
            if ($matchedCache.Count -ne 1) { throw 'Requested cache domain does not match this machine' }
        }
        $casePath = [IO.Path]::GetFullPath($job.CaseFile, $manifestRoot)
        $caseData = Get-Content -LiteralPath $casePath -Raw | ConvertFrom-Json
        if (-not $caseData.problem.maxLinkRate) { throw 'Case requires exact belt capacity' }
        if ($null -ne $job.Diagnostics -and $job.Diagnostics -isnot [bool]) { throw 'Invalid Diagnostics flag' }
        if ($null -eq $job.Variant) { $job | Add-Member NoteProperty Variant 'after' }
        $matchedVariant = @($variants | Where-Object { $_.Name -eq $job.Variant })
        if ($matchedVariant.Count -ne 1) {
            throw 'Invalid binary variant'
        }
        $directory = $matchedVariant[0].Directory
        $job | Add-Member NoteProperty BinaryDirectory $directory
        $job | Add-Member NoteProperty CasePath $casePath
    }
    $identities = @($jobs | ForEach-Object { "$($_.Case)-$($_.Mode)-$($_.Variant)-$($_.Stage)-$($_.Workers)-$($_.Repeat)" })
    if (@($identities | Sort-Object -Unique).Count -ne $jobs.Count) { throw 'Duplicate job identities' }
    if ($null -ne $manifest.MaxScheduledSeconds) {
        $worst = ($jobs | Measure-Object TimeoutSeconds -Sum).Sum + $jobs.Count * $CancellationGraceSeconds
        if ($manifest.MaxScheduledSeconds -isnot [long] -and $manifest.MaxScheduledSeconds -isnot [int]) { throw 'Invalid schedule allowance' }
        if ($manifest.MaxScheduledSeconds -le 0 -or $worst -gt $manifest.MaxScheduledSeconds) { throw 'Search and cleanup allowances exceed manifest budget' }
    }
    Copy-Item -LiteralPath $manifestPath -Destination (Join-Path $outputRoot 'job-manifest.json')
    $caseOutput = New-Item -ItemType Directory -Path (Join-Path $outputRoot 'cases')
    foreach ($casePath in ($jobs.CasePath | Sort-Object -Unique)) {
        Copy-Item -LiteralPath $casePath -Destination $caseOutput.FullName
    }
    foreach ($job in $jobs) { $job.CasePath = Join-Path $caseOutput.FullName (Split-Path $job.CasePath -Leaf) }
    $variants = @($variants | Where-Object { $_.Name -in $jobs.Variant })
}
$randomOrder = [Random]::new($Seed)
$pairedJobs = @($jobs | Where-Object { $_.PairId })
if ($pairedJobs.Count) {
    if ($pairedJobs.Count -ne $jobs.Count) { throw 'Do not mix paired and unpaired jobs' }
    $pairs = @($jobs | Group-Object PairId)
    foreach ($pair in $pairs) {
        $members = @($pair.Group)
        if ($members.Count -ne 2 -or @($members.PairRole | Sort-Object -Unique).Count -ne 2 -or 'reference' -notin $members.PairRole -or 'candidate' -notin $members.PairRole -or $members[0].Variant -eq $members[1].Variant) { throw 'Invalid reference/candidate pair' }
        foreach ($field in @('Case','Mode','Stage','Workers','Repeat','MaxNodes','TimeoutSeconds','Hotspots','ProcessorAffinity','CacheBytes','Comparison','Cohort','CaseFile','Diagnostics')) {
            if ($members[0].$field -ne $members[1].$field) { throw "Unmatched pair setting: $field" }
        }
    }
    $firstRole = if ($randomOrder.Next(2)) { 'reference' } else { 'candidate' }
    $jobs = @($pairs | Sort-Object { $randomOrder.Next() } | ForEach-Object {
        $role = if ($_.Group[0].Repeat % 2) { $firstRole } else { if ($firstRole -eq 'reference') { 'candidate' } else { 'reference' } }
        $_.Group | Sort-Object { if ($_.PairRole -eq $role) { 0 } else { 1 } }
    })
} else { $jobs = @($jobs | Sort-Object { $randomOrder.Next() }) }
$jobs | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $outputRoot 'schedule.json')
if ($PlanOnly) { Write-Output "Validated $($jobs.Count) jobs; plan written to $outputRoot"; return }
@{ seed=$Seed; cancellation_grace_s=$CancellationGraceSeconds } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $outputRoot 'settings.json')
git -C $repoRoot rev-parse HEAD | Set-Content -LiteralPath (Join-Path $outputRoot 'revision.txt')
git -C $repoRoot status --short | Set-Content -LiteralPath (Join-Path $outputRoot 'working-tree.txt')
rustc -Vv | Set-Content -LiteralPath (Join-Path $outputRoot 'rustc.txt')
Get-CimInstance Win32_Processor | Select-Object Name,NumberOfCores,NumberOfLogicalProcessors | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $outputRoot 'machine.json')
$binaryHashes = foreach ($variant in $variants) {
    $executables = @('profile_solver')
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
    $executable = 'profile_solver'
    $exe = Join-Path ([IO.Path]::GetFullPath($job.BinaryDirectory, $repoRoot)) "$executable.exe"
    $json = Join-Path $outputRoot "$name.json"
    $maxNodes = $job.MaxNodes
    $timeout = $job.TimeoutSeconds
    $jobHotspots = 'off'
    $arguments = @([string]$timeout, [string]$job.Workers, [string]$maxNodes, $json, $job.Mode, $job.CasePath)
    Write-Output "Starting $index/$($jobs.Count) $name"
    @("RUNNING: $index / $($jobs.Count)", "Current: $name", "Time cap: $timeout seconds; N <= $maxNodes; hotspots $jobHotspots", "Cancellation cleanup watchdog: $CancellationGraceSeconds additional seconds", "Updated: $(Get-Date -Format o)") | Set-Content -LiteralPath (Join-Path $outputRoot 'BENCHMARK-STATUS.txt')
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $originalBackend = $env:SOLVER_CVC5
    $originalDiagnostics = $env:SOLVER_DIAGNOSTICS
    try {
        $env:SOLVER_DIAGNOSTICS = if ($job.Diagnostics -eq $true) { '1' } else { $null }
        $env:SOLVER_CVC5 = Join-Path (Split-Path $exe -Parent) 'cvc5.exe'
    $launch = Start-BenchmarkProcess -Executable $exe -Arguments $arguments -WorkingDirectory $repoRoot -StandardOutput (Join-Path $outputRoot "$name.log") -StandardError (Join-Path $outputRoot "$name.stderr.log") -Affinity $job.ProcessorAffinity
    } finally { $env:SOLVER_CVC5 = $originalBackend; $env:SOLVER_DIAGNOSTICS = $originalDiagnostics }

    $process = $launch.Process
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
            $process.Kill($true)
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
    if ($job.Diagnostics -and $data.diagnostics_enabled -ne $true) { throw "Executable ignored solver diagnostics: $name" }
    if ($data.hotspot_recording -ne ($jobHotspots -eq 'on')) { throw "Executable ignored hotspot recording setting: $name" }
    if ($JobManifest) {
        $expected = Get-Content -LiteralPath $job.CasePath -Raw | ConvertFrom-Json
        # Compare exact Rational parsing in the executable; JSON decimals may normalize to fractions.
        if ($data.case -ne $expected.name -or $data.comparison_protocol -ne 'layout-v1') { throw "Executable ignored benchmark case: $name" }
    }
    $row = [pscustomobject]@{
        case=$job.Case; mode=$job.Mode; variant=$job.Variant; stage=$job.Stage; workers=$job.Workers; repeat=$job.Repeat
        status=$data.status; layouts=$data.layouts; wall_s=$data.wall_s
        max_nodes=$maxNodes; timeout_s=$timeout; cohort=$job.Cohort
        first_valid_s=$data.first_valid_s; hotspot_recording=$data.hotspot_recording
        process_cpu_s=$cpu; cpu_utilization=($cpu / $watch.Elapsed.TotalSeconds / $job.Workers)
        accounted_timer_s=$data.accounted_timer_s; sampled_peak_working_set_bytes=$peakWorkingSet
        process_wall_s=$watch.Elapsed.TotalSeconds; result_file=$json
        processor_affinity=$launch.RequestedMask; affinity_observed=$launch.ObservedMask
        affinity_before_resume=$launch.BeforeResume; affinity_applied_s=$launch.AppliedSeconds
        pair_id=$job.PairId; pair_role=$job.PairRole; comparison=$job.Comparison
    }
    $rows.Add($row)
    $rows | Export-Csv -LiteralPath (Join-Path $outputRoot 'results.csv') -NoTypeInformation
    Write-Output "$name status=$($data.status) layouts=$($data.layouts) wall=$($data.wall_s)s cpu=$cpu"
    $process.Dispose()
}
Write-Output "Results: $outputRoot"
