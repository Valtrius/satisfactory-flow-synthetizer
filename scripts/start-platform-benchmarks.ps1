param(
    [Parameter(Mandatory)][string]$PreparedDirectory,
    [switch]$Run
)
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath($PreparedDirectory)
$metadata = Get-Content -LiteralPath (Join-Path $root 'metadata.json') -Raw | ConvertFrom-Json
$sessionSeconds = $metadata.suite.session_seconds
if ($sessionSeconds -isnot [ValueType] -or $sessionSeconds -lt 1 -or $sessionSeconds -gt 10800 -or [Math]::Floor($sessionSeconds) -ne $sessionSeconds) { throw 'Session budget must be an integer between 1 and 10800 seconds.' }
$status = Join-Path $root 'BENCHMARK-STATUS.txt'
if (-not $Run) {
    if ((Get-Content -LiteralPath $status -Raw) -notmatch '^PREPARED:') { throw 'Expected an unused prepared suite.' }
    $arguments = @('-NoProfile', '-STA', '-File', ('"' + (Join-Path $root 'start-platform-benchmarks.ps1') + '"'), '-PreparedDirectory', ('"' + $root + '"'), '-Run')
    'STARTING: the suite will write progress here and show a completion dialog.' | Set-Content -LiteralPath $status
    $process = Start-Process -FilePath (Get-Process -Id $PID).Path -ArgumentList $arguments -WorkingDirectory $root -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $root 'supervisor.log') -RedirectStandardError (Join-Path $root 'supervisor.stderr.log')
    $process.Id | Set-Content -LiteralPath (Join-Path $root 'supervisor.pid')
    Write-Output "Started benchmark supervisor PID $($process.Id)."
    Write-Output "Status: $status"
    return
}
try {
    $runner = Start-Process -FilePath (Get-Command node).Source -ArgumentList @(('"' + (Join-Path $root 'run-platform-benchmarks.mjs') + '"'), '--run', ('"' + $root + '"')) -WorkingDirectory $root -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $root 'runner.log') -RedirectStandardError (Join-Path $root 'runner.stderr.log')
    $runner.Id | Set-Content -LiteralPath (Join-Path $root 'runner.pid')
    if (-not $runner.WaitForExit([int]$sessionSeconds * 1000)) {
        $runner.Kill($true)
        $runner.WaitForExit(10000) | Out-Null
        "STOPPED: $sessionSeconds-second supervisor deadline; owned runner and descendants terminated. Inspect partial REPORT.md and raw records." | Set-Content -LiteralPath $status
    } elseif ($runner.ExitCode -ne 0 -and (Get-Content -LiteralPath $status -Raw) -notmatch '^FAILED:') {
        "FAILED: runner exited with code $($runner.ExitCode). See runner.stderr.log and partial results." | Set-Content -LiteralPath $status
    }
} catch {
    "FAILED: $($_.Exception.Message)" | Set-Content -LiteralPath $status
}
$message = (Get-Content -LiteralPath $status -Raw) + "`nReport: $(Join-Path $root 'REPORT.md')"
try {
    Add-Type -AssemblyName System.Windows.Forms
    [System.Media.SystemSounds]::Asterisk.Play()
    [System.Windows.Forms.MessageBox]::Show($message, 'Native and web solver benchmarks finished') | Out-Null
} catch { Write-Warning 'Completion dialog unavailable; use BENCHMARK-STATUS.txt.' }
