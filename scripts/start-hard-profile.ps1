param(
    [string]$JobManifest = 'benchmarks/custom/hard-obligations.json',
    [string]$OutputDirectory = "target/parallelism-ladder/hard-obligations-$(Get-Date -Format 'yyyyMMdd-HHmmss')",
    [string]$RepositoryRoot = '',
    [switch]$Run
)
$ErrorActionPreference = 'Stop'
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
    & python (Join-Path $runRoot 'hard-profile.py') prepare $runRoot --manifest ([IO.Path]::GetFullPath($JobManifest, $repoRoot))
    if ($LASTEXITCODE -ne 0) { throw 'Fixed-work plan validation failed' }
    $source = New-Item -ItemType Directory -Path (Join-Path $runRoot 'solver-source')
    Copy-Item -LiteralPath (Join-Path $repoRoot 'crates/solver-core/src'),(Join-Path $repoRoot 'crates/solver-core/examples') -Destination $source.FullName -Recurse
    Copy-Item -LiteralPath (Join-Path $repoRoot 'crates/solver-core/Cargo.toml') -Destination $source.FullName
    $workspace = New-Item -ItemType Directory -Path (Join-Path $runRoot 'workspace-build')
    Copy-Item -LiteralPath (Join-Path $repoRoot 'Cargo.toml'),(Join-Path $repoRoot 'Cargo.lock') -Destination $workspace.FullName
    git -C $repoRoot rev-parse HEAD | Set-Content -LiteralPath (Join-Path $runRoot 'revision.txt')
    git -C $repoRoot diff --binary | Set-Content -LiteralPath (Join-Path $runRoot 'working-tree.diff')
    git -C $repoRoot status --short | Set-Content -LiteralPath (Join-Path $runRoot 'working-tree.txt')
    rustc -Vv | Set-Content -LiteralPath (Join-Path $runRoot 'rustc.txt')
    Get-CimInstance Win32_Processor | Select-Object Name,NumberOfCores,NumberOfLogicalProcessors | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $runRoot 'machine.json')
    'STARTING. Fixed-work profiling; completion/failure dialog enabled.' | Set-Content -LiteralPath $status
    $shell = (Get-Process -Id $PID).Path
    $arguments = @('-NoProfile','-STA','-File',('"'+(Join-Path $runRoot 'start-hard-profile.ps1')+'"'),'-Run','-RepositoryRoot',('"'+$repoRoot+'"'),'-OutputDirectory',('"'+$runRoot+'"'))
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
        @("RUNNING $index/$($schedule.Count): $($job.id)","Exact N=$($job.request.node_count) L=$($job.request.link_count); $timeout s cap + 60 s cleanup grace", "Updated: $(Get-Date -Format o)") | Set-Content -LiteralPath $status
        $result = Join-Path $runRoot "results/$($job.id).json"
        $watch = [Diagnostics.Stopwatch]::StartNew()
        $process = Start-Process -FilePath (Join-Path $runRoot 'bin/profile_obligation.exe') -ArgumentList @(('"'+$job.request_file+'"'),('"'+$result+'"')) -WorkingDirectory $repoRoot -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $runRoot "results/$($job.id).log") -RedirectStandardError (Join-Path $runRoot "results/$($job.id).stderr.log")
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
        $metrics.Add([pscustomobject]@{ id=$job.id; process_wall_s=$watch.Elapsed.TotalSeconds; process_cpu_s=$cpu; peak_working_set_bytes=$peak; watchdog_killed=$killed; exit_code=$process.ExitCode })
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
    $message = @('FINISHED: fixed-work profiling and verification completed.',"Finished: $(Get-Date -Format o)",'Exhaustion applies only to selected profiles. Capped work remains incomplete.',"Results: $runRoot",'Message the Codex task to analyze results.')
    $message | Set-Content -LiteralPath (Join-Path $runRoot 'BENCHMARK-FINISHED.txt')
    $title = 'Custom hard profiling finished'
} catch {
    $message = @('FAILED OR INTERRUPTED', $_.Exception.Message, "Updated: $(Get-Date -Format o)", "Results: $runRoot")
    $message | Set-Content -LiteralPath (Join-Path $runRoot 'BENCHMARK-FAILED.txt')
    $title = 'Custom hard profiling needs attention'
}
$message | Set-Content -LiteralPath $status
try {
    Add-Type -AssemblyName System.Windows.Forms
    [System.Media.SystemSounds]::Asterisk.Play()
    [System.Windows.Forms.MessageBox]::Show(($message -join [Environment]::NewLine), $title) | Out-Null
} catch { Write-Warning 'Dialog unavailable; use BENCHMARK-STATUS.txt.' }
