param(
    [string]$JobManifest = 'benchmarks/custom/hard-obligations.json',
    [string]$OutputDirectory = "target/parallelism-ladder/hard-obligations-$(Get-Date -Format 'yyyyMMdd-HHmmss')",
    [string]$RepositoryRoot = '',
    [string]$RegressionManifest = '',
    [switch]$PlanOnly,
    [switch]$NoNotification,
    [switch]$Run
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'benchmark-affinity.ps1')
$repoRoot = if ($RepositoryRoot) { [IO.Path]::GetFullPath($RepositoryRoot) } else { Split-Path $PSScriptRoot -Parent }
$runRoot = [IO.Path]::GetFullPath($OutputDirectory, $repoRoot)
$status = Join-Path $runRoot 'BENCHMARK-STATUS.txt'
if (-not $Run) {
    if (Test-Path -LiteralPath $runRoot) { throw "Output exists: $runRoot" }
    $binary = Join-Path $repoRoot 'target/release/examples/profile_obligation.exe'
    if (-not (Test-Path -LiteralPath $binary)) { throw 'Build profile_obligation with bench-internals first.' }
    New-Item -ItemType Directory -Path (Join-Path $runRoot 'bin'),(Join-Path $runRoot 'results') | Out-Null
    Copy-Item -LiteralPath $binary -Destination (Join-Path $runRoot 'bin')
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'hard-profile.py'),$PSCommandPath -Destination $runRoot
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'benchmark-affinity.ps1') -Destination $runRoot
    & python (Join-Path $runRoot 'hard-profile.py') prepare $runRoot --manifest ([IO.Path]::GetFullPath($JobManifest, $repoRoot))
    if ($LASTEXITCODE -ne 0) { throw 'Fixed-work plan validation failed' }
    if ($RegressionManifest) {
        Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'parallelism-ladder.ps1'),(Join-Path $PSScriptRoot 'analyze-parallelism.py'),(Join-Path $PSScriptRoot 'benchmark_policy.py') -Destination $runRoot
        $wholeInput = Join-Path $runRoot 'whole-input'
        & (Join-Path $runRoot 'parallelism-ladder.ps1') -RepositoryRoot $repoRoot -JobManifest ([IO.Path]::GetFullPath($RegressionManifest, $repoRoot)) -BinaryDirectory (Join-Path $runRoot 'variants/basis/bin') -ReferenceBinaryDirectory (Join-Path $runRoot 'variants/reference/bin') -OutputDirectory $wholeInput -Hotspots off -PlanOnly
        $wholeManifestPath = Join-Path $wholeInput 'job-manifest.json'
        $wholeManifest = Get-Content -LiteralPath $wholeManifestPath -Raw | ConvertFrom-Json
        foreach ($job in $wholeManifest.jobs) { $job.CaseFile = 'cases/' + (Split-Path $job.CaseFile -Leaf) }
        $wholeManifest | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $wholeManifestPath
    }
    $source = New-Item -ItemType Directory -Path (Join-Path $runRoot 'solver-source')
    Copy-Item -LiteralPath (Join-Path $repoRoot 'crates/solver-core/src'),(Join-Path $repoRoot 'crates/solver-core/examples') -Destination $source.FullName -Recurse
    Copy-Item -LiteralPath (Join-Path $repoRoot 'crates/solver-core/Cargo.toml') -Destination $source.FullName
    $workspace = New-Item -ItemType Directory -Path (Join-Path $runRoot 'workspace-build')
    Save-BenchmarkTopology (Join-Path $runRoot 'topology.json')
    Copy-Item -LiteralPath (Join-Path $repoRoot 'Cargo.toml'),(Join-Path $repoRoot 'Cargo.lock') -Destination $workspace.FullName
    git -C $repoRoot rev-parse HEAD | Set-Content -LiteralPath (Join-Path $runRoot 'revision.txt')
    git -C $repoRoot diff --binary | Set-Content -LiteralPath (Join-Path $runRoot 'working-tree.diff')
    git -C $repoRoot status --short | Set-Content -LiteralPath (Join-Path $runRoot 'working-tree.txt')
    rustc -Vv | Set-Content -LiteralPath (Join-Path $runRoot 'rustc.txt')
    Get-CimInstance Win32_Processor | Select-Object Name,NumberOfCores,NumberOfLogicalProcessors | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $runRoot 'machine.json')
    if ($PlanOnly) {
        'PREPARED ONLY: no benchmark processes launched.' | Set-Content -LiteralPath $status
        Write-Output "Frozen plan ready: $runRoot"
        return
    }
    'STARTING. Fixed-work profiling; completion/failure dialog enabled.' | Set-Content -LiteralPath $status
    $shell = (Get-Process -Id $PID).Path
    $arguments = @('-NoProfile','-STA','-File',('"'+(Join-Path $runRoot 'start-hard-profile.ps1')+'"'),'-Run','-RepositoryRoot',('"'+$repoRoot+'"'),'-OutputDirectory',('"'+$runRoot+'"'))
    if ($NoNotification) { $arguments += '-NoNotification' }
    $child = Start-Process -FilePath $shell -ArgumentList $arguments -WorkingDirectory $repoRoot -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $runRoot 'runner.log') -RedirectStandardError (Join-Path $runRoot 'runner.stderr.log')
    $child.Id | Set-Content -LiteralPath (Join-Path $runRoot 'runner.pid')
    Write-Output "Started fixed-work profiler PID $($child.Id). Status: $status"
    return
}

try {
    $schedule = @(Get-Content -LiteralPath (Join-Path $runRoot 'schedule.json') -Raw | ConvertFrom-Json)
    $metrics = [Collections.Generic.List[object]]::new()
    $failures = [Collections.Generic.List[object]]::new()
    $index = 0
    foreach ($job in $schedule) {
        $index++
        $timeout = [int]$job.request.timeout_s
        @("RUNNING $index/$($schedule.Count): $($job.id)","$timeout s cap + 60 s cleanup grace; variant $($job.variant)", "Updated: $(Get-Date -Format o)") | Set-Content -LiteralPath $status
        $result = Join-Path $runRoot "results/$($job.id).json"
        $watch = [Diagnostics.Stopwatch]::StartNew()
        $executable = if ($job.binary) { $job.binary } else { Join-Path $runRoot 'bin/profile_obligation.exe' }
        $jobArguments = @($job.request_file, $result)
        if ($job.kind -eq 'witness_replay') { $jobArguments += [string][int]$job.request.cancel_after_ms }
        $launch = Start-BenchmarkProcess -Executable $executable -Arguments $jobArguments -WorkingDirectory $repoRoot -StandardOutput (Join-Path $runRoot "results/$($job.id).log") -StandardError (Join-Path $runRoot "results/$($job.id).stderr.log") -Affinity $job.processor_affinity
        $process = $launch.Process
        $null = $process.Handle
        $cpu = 0.0
        $peak = 0L
        $killed = $false
        while (-not $process.WaitForExit(25)) {
            $process.Refresh()
            $cpu = [Math]::Max($cpu, $process.TotalProcessorTime.TotalSeconds)
            $peak = [Math]::Max($peak, $process.PeakWorkingSet64)
            if ($watch.Elapsed.TotalSeconds -gt ($timeout + 60)) {
                $process.Kill()
                $process.WaitForExit()
                $killed = $true
                break
            }
        }
        $process.WaitForExit()
        $watch.Stop()
        $cpu = [Math]::Max($cpu, $process.TotalProcessorTime.TotalSeconds)
        $metrics.Add([pscustomobject]@{ id=$job.id; process_wall_s=$watch.Elapsed.TotalSeconds; process_cpu_s=$cpu; peak_working_set_bytes=$peak; watchdog_killed=$killed; exit_code=$process.ExitCode; affinity_requested=$job.processor_affinity; affinity_observed=$launch.ObservedMask; affinity_applied_s=$launch.AppliedSeconds; affinity_before_resume=$launch.BeforeResume })
        if ($killed -or $process.ExitCode -ne 0 -or -not (Test-Path -LiteralPath $result)) {
            $failures.Add([pscustomobject]@{id=$job.id; watchdog_killed=$killed; exit_code=$process.ExitCode})
            ConvertTo-Json -InputObject @($failures.ToArray()) | Set-Content -LiteralPath (Join-Path $runRoot 'failed-jobs.json')
        }
        $metrics | Export-Csv -LiteralPath (Join-Path $runRoot 'process-metrics.csv') -NoTypeInformation
        $process.Dispose()
    }
    'VERIFYING fixed-work identities and results.' | Set-Content -LiteralPath $status
    & python (Join-Path $runRoot 'hard-profile.py') verify $runRoot *> (Join-Path $runRoot 'verification.log')
    if ($LASTEXITCODE -ne 0) { throw 'Fixed-work verification failed; inspect summary.json and failed-jobs.json.' }
    $wholeManifestPath = Join-Path $runRoot 'whole-input/job-manifest.json'
    if (Test-Path -LiteralPath $wholeManifestPath) {
        'RUNNING whole optimal/all regression. See whole-results/BENCHMARK-STATUS.txt.' | Set-Content -LiteralPath $status
        $wholeResults = Join-Path $runRoot 'whole-results'
        & (Join-Path $runRoot 'parallelism-ladder.ps1') -RepositoryRoot $repoRoot -JobManifest $wholeManifestPath -BinaryDirectory (Join-Path $runRoot 'variants/basis/bin') -ReferenceBinaryDirectory (Join-Path $runRoot 'variants/reference/bin') -OutputDirectory $wholeResults -Hotspots off
        & python (Join-Path $runRoot 'analyze-parallelism.py') $wholeResults --allow-incomplete --output (Join-Path $wholeResults 'summary.json') *> (Join-Path $runRoot 'whole-verification.log')
        if ($LASTEXITCODE -ne 0) { throw 'Whole-solve verification failed; inspect whole-verification.log.' }
    }
    $message = @('FINISHED: Custom benchmarks and verification completed.',"Finished: $(Get-Date -Format o)",'Fixed-work exhaustion applies only to its recorded profile/prefix/root scope. Capped work remains incomplete.',"Results: $runRoot",'Message the Codex task to analyze results.')
    $message | Set-Content -LiteralPath (Join-Path $runRoot 'BENCHMARK-FINISHED.txt')
    $title = 'Custom hard profiling finished'
} catch {
    $message = @('FAILED OR INTERRUPTED', $_.Exception.Message, "Updated: $(Get-Date -Format o)", "Results: $runRoot")
    $message | Set-Content -LiteralPath (Join-Path $runRoot 'BENCHMARK-FAILED.txt')
    $title = 'Custom hard profiling needs attention'
}
$message | Set-Content -LiteralPath $status
if ($NoNotification) { return }
try {
    Add-Type -AssemblyName System.Windows.Forms
    [System.Media.SystemSounds]::Asterisk.Play()
    [System.Windows.Forms.MessageBox]::Show(($message -join [Environment]::NewLine), $title) | Out-Null
} catch { Write-Warning 'Dialog unavailable; use BENCHMARK-STATUS.txt.' }
