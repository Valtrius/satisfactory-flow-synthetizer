param(
    [string[]]$Stages = @('baseline', 'p1', 'p12', 'p123', 'p1234', 'shared', 'groups', 'p14', 'p124'),
    [ValidateSet('all', 'optimal')][string[]]$Modes = @('all', 'optimal'),
    [ValidateRange(1, 4096)][int[]]$Workers = @(1, 4, 16, 32),
    [ValidateRange(1, 100)][int]$Repeats = 3,
    [ValidateRange(1, 86400)][int]$TimeoutSeconds = 600,
    [string[]]$Cases = @('profile_40_25', 'profile_7_6_5_4_2'),
    [string]$BinaryDirectory = 'target/release/examples',
    [string]$ReferenceBinaryDirectory = '',
    [string[]]$ReferenceStages = @('baseline', 'p1234'),
    [ValidateSet('on', 'off')][string]$Hotspots = 'off',
    [int]$Seed = 270826,
    [string]$OutputDirectory = "target/parallelism-ladder/runs-$(Get-Date -Format 'yyyyMMdd-HHmmss')"
)
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path $PSScriptRoot -Parent
$outputRoot = [IO.Path]::GetFullPath((Join-Path $repoRoot $OutputDirectory))
if (Test-Path -LiteralPath $outputRoot) { throw "Output already exists: $outputRoot" }
New-Item -ItemType Directory -Path $outputRoot | Out-Null
$variants = @([pscustomobject]@{ Name='after'; Directory=$BinaryDirectory; Stages=$Stages })
if ($ReferenceBinaryDirectory) {
    $variants += [pscustomobject]@{ Name='before'; Directory=$ReferenceBinaryDirectory; Stages=$ReferenceStages }
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
$randomOrder = [Random]::new($Seed)
$jobs = @($jobs | Sort-Object { $randomOrder.Next() })
$jobs | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $outputRoot 'schedule.json')
@{ seed=$Seed; hotspots=$Hotspots; timeout_s=$TimeoutSeconds; repeats=$Repeats } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $outputRoot 'settings.json')
git -C $repoRoot rev-parse HEAD | Set-Content -LiteralPath (Join-Path $outputRoot 'revision.txt')
git -C $repoRoot status --short | Set-Content -LiteralPath (Join-Path $outputRoot 'working-tree.txt')
rustc -Vv | Set-Content -LiteralPath (Join-Path $outputRoot 'rustc.txt')
Get-CimInstance Win32_Processor | Select-Object Name,NumberOfCores,NumberOfLogicalProcessors | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $outputRoot 'machine.json')
$binaryHashes = foreach ($variant in $variants) {
    foreach ($case in $Cases) {
        $binary = Join-Path $repoRoot "$($variant.Directory)/$case.exe"
        $hash = Get-FileHash -LiteralPath $binary
        [pscustomobject]@{ variant=$variant.Name; case=$case; path=$hash.Path; sha256=$hash.Hash }
    }
}
$binaryHashes | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $outputRoot 'binaries.json')
$rows = [Collections.Generic.List[object]]::new()
$index = 0
foreach ($job in $jobs) {
    $index++
    $name = "$($job.Case)-$($job.Mode)-$($job.Variant)-$($job.Stage)-w$($job.Workers)-r$($job.Repeat)"
    $exe = Join-Path $repoRoot "$($job.BinaryDirectory)/$($job.Case).exe"
    $json = Join-Path $outputRoot "$name.json"
    $maxNodes = switch ($job.Case) { 'profile_40_25' { 6 } 'profile_tiny' { 2 } default { 12 } }
    $arguments = @($TimeoutSeconds, $job.Workers, $maxNodes, 'custom', $job.Stage, ('"' + $json + '"'), $job.Mode, $Hotspots)
    Write-Output "Starting $index/$($jobs.Count) $name"
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $process = Start-Process -FilePath $exe -ArgumentList $arguments -WorkingDirectory $repoRoot -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $outputRoot "$name.log") -RedirectStandardError (Join-Path $outputRoot "$name.stderr.log")
    $null = $process.Handle
    $cpu = 0.0
    $peakWorkingSet = 0L
    while (-not $process.WaitForExit(25)) {
        $process.Refresh()
        $cpu = [Math]::Max($cpu, $process.TotalProcessorTime.TotalSeconds)
        $peakWorkingSet = [Math]::Max($peakWorkingSet, $process.PeakWorkingSet64)
        if ($watch.Elapsed.TotalSeconds -gt ($TimeoutSeconds + 60)) {
            $process.Kill()
            throw "Watchdog terminated $name after cancellation failed to return"
        }
    }
    $process.WaitForExit()
    $watch.Stop()
    $cpu = [Math]::Max($cpu, $process.TotalProcessorTime.TotalSeconds)
    if ($process.ExitCode -ne 0 -or -not (Test-Path -LiteralPath $json)) { throw "Benchmark failed: $name" }
    $data = Get-Content -LiteralPath $json -Raw | ConvertFrom-Json
    if ($data.mode -ne $job.Mode -or $data.stage -ne $job.Stage -or $data.workers -ne $job.Workers) { throw "Executable ignored benchmark settings: $name" }
    $row = [pscustomobject]@{
        case=$job.Case; mode=$job.Mode; variant=$job.Variant; stage=$job.Stage; workers=$job.Workers; repeat=$job.Repeat
        status=$data.status; layouts=$data.layouts; wall_s=$data.wall_s
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
