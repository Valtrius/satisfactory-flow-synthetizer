param(
    [Parameter(Mandatory)][string]$PreparedSuite,
    [switch]$Run,
    [switch]$NoDialog
)
$ErrorActionPreference = 'Stop'
$suiteRoot = (Resolve-Path -LiteralPath $PreparedSuite).Path
$status = Join-Path $suiteRoot 'BENCHMARK-STATUS.txt'
$settings = Get-Content -LiteralPath (Join-Path $suiteRoot 'optimization-suite.json') -Raw | ConvertFrom-Json
if (-not $Run) {
    if (Test-Path -LiteralPath (Join-Path $suiteRoot 'results')) { throw 'This frozen suite already has results; prepare a fresh run directory.' }
    if (Test-Path -LiteralPath (Join-Path $suiteRoot 'runner.pid')) { throw 'This suite was already launched; inspect its status before preparing another run.' }
}
$hashes = Get-Content -LiteralPath (Join-Path $suiteRoot 'frozen-hashes.json') -Raw | ConvertFrom-Json -AsHashtable
foreach ($entry in $hashes.GetEnumerator()) {
        $path = [IO.Path]::GetFullPath((Join-Path $suiteRoot $entry.Key))
        if (-not $path.StartsWith($suiteRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Frozen path escaped the suite' }
        if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $entry.Value) { throw "Frozen file changed: $($entry.Key)" }
}
if (-not $Run) {
    'STARTING: frozen identities verified. A completion dialog will appear after both verifiers finish.' | Set-Content -LiteralPath $status
    $shell = (Get-Process -Id $PID).Path
    $arguments = @('-NoProfile','-STA','-File',('"'+(Join-Path $suiteRoot 'start-optimization-suite.ps1')+'"'),'-PreparedSuite',('"'+$suiteRoot+'"'),'-Run')
    $child = Start-Process -FilePath $shell -ArgumentList $arguments -WorkingDirectory $settings.repository -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $suiteRoot 'runner.log') -RedirectStandardError (Join-Path $suiteRoot 'runner.stderr.log')
    $child.Id | Set-Content -LiteralPath (Join-Path $suiteRoot 'runner.pid')
    Write-Output "Started $($settings.name), PID $($child.Id). Status: $status"
    return
}
$failed = $false
try {
    'RUNNING: see results/BENCHMARK-STATUS.txt for the current job.' | Set-Content -LiteralPath $status
    & (Join-Path $suiteRoot 'run-benchmark-screen.ps1') -RepositoryRoot $settings.repository -JobManifest (Join-Path $suiteRoot 'input/job-manifest.json') -BinaryDirectory (Join-Path $suiteRoot 'bin') -VariantBinaryMap (Join-Path $suiteRoot 'variant-binaries.json') -OutputDirectory (Join-Path $suiteRoot 'results') -CancellationGraceSeconds 15
    if (-not $?) { throw 'Benchmark runner failed' }
    'VERIFYING: exact results, frozen identities and independent proof ownership.' | Set-Content -LiteralPath $status
    & python (Join-Path $suiteRoot 'analyze-benchmarks.py') (Join-Path $suiteRoot 'results') --allow-incomplete --output (Join-Path $suiteRoot 'results/summary.json') *> (Join-Path $suiteRoot 'verification.log')
    if ($LASTEXITCODE -ne 0) { throw 'Exact-result verification failed; see verification.log' }
    & python (Join-Path $suiteRoot 'audit-optimization-results.py') (Join-Path $suiteRoot 'results') *> (Join-Path $suiteRoot 'root-verification.log')
    if ($LASTEXITCODE -ne 0) { throw 'Proof-owner verification failed; see root-verification.log' }
    $message = @('FINISHED: all jobs attempted and both verifiers passed.', 'Capped solves remain explicitly incomplete; this is not a promotion verdict.', "Results: $(Join-Path $suiteRoot 'results')", 'Message the Codex task to analyze the results.')
    $message | Set-Content -LiteralPath (Join-Path $suiteRoot 'BENCHMARK-FINISHED.txt')
    $title = 'Solver optimization suite finished'
} catch {
    $failed = $true
    $message = @('FINISHED WITH FAILURES', $_.Exception.Message, "Inspect: $suiteRoot")
    $message | Set-Content -LiteralPath (Join-Path $suiteRoot 'BENCHMARK-FAILED.txt')
    $title = 'Solver optimization suite needs attention'
}
$message | Set-Content -LiteralPath $status
if (-not $NoDialog) { try {
    Add-Type -AssemblyName System.Windows.Forms
    [System.Media.SystemSounds]::Asterisk.Play()
    [System.Windows.Forms.MessageBox]::Show(($message -join [Environment]::NewLine), $title) | Out-Null
} catch { Write-Warning 'Completion dialog unavailable; status files remain authoritative.' } }
if ($failed) { exit 1 }
