param(
    [Parameter(Mandatory)][string]$PreparedCampaign,
    [switch]$Run,
    [switch]$NoDialog
)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path -LiteralPath $PreparedCampaign).Path
$status = Join-Path $root 'CAMPAIGN-STATUS.txt'
$campaign = Get-Content -LiteralPath (Join-Path $root 'campaign.json') -Raw | ConvertFrom-Json
$hashes = Get-Content -LiteralPath (Join-Path $root 'campaign-hashes.json') -Raw | ConvertFrom-Json -AsHashtable
foreach ($entry in $hashes.GetEnumerator()) {
    $path = [IO.Path]::GetFullPath((Join-Path $root $entry.Key))
    if (-not $path.StartsWith($root + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Campaign path escaped its directory' }
    if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $entry.Value) { throw "Frozen campaign index changed: $($entry.Key)" }
}
if (-not $Run) {
    foreach ($suite in $campaign.suites) {
        if (Test-Path -LiteralPath (Join-Path $root "$($suite.name)/results")) { throw 'This campaign already has results; prepare a fresh campaign directory.' }
        if (Test-Path -LiteralPath (Join-Path $root "$($suite.name)/runner.pid")) { throw 'A suite in this campaign was already launched separately.' }
    }
    New-Item -ItemType File -Path (Join-Path $root 'CAMPAIGN-STARTED') -ErrorAction Stop | Out-Null
    'STARTING: suites will run sequentially. One completion dialog will appear after the queue finishes.' | Set-Content -LiteralPath $status
    $shell = (Get-Process -Id $PID).Path
    $arguments = @('-NoProfile','-STA','-File',('"'+(Join-Path $root 'start-optimization-campaign.ps1')+'"'),'-PreparedCampaign',('"'+$root+'"'),'-Run')
    $child = Start-Process -FilePath $shell -ArgumentList $arguments -WorkingDirectory $root -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $root 'campaign.log') -RedirectStandardError (Join-Path $root 'campaign.stderr.log')
    $child.Id | Set-Content -LiteralPath (Join-Path $root 'campaign.pid')
    Write-Output "Started sequential campaign PID $($child.Id). Status: $status"
    return
}
$outcomes = @()
$started = Get-Date
$campaignFailed = $false
$remaining = @()
ConvertTo-Json -InputObject $outcomes | Set-Content -LiteralPath (Join-Path $root 'suite-outcomes.json')
try {
    $index = 0
    foreach ($suite in $campaign.suites) {
        # Respect the earlier eight-hour allowance without interrupting a pair
        # or leaving a suite's exact-result comparison half-written.
        if (((Get-Date) - $started).TotalSeconds + $suite.max_scheduled_seconds + 120 -gt 28800) {
            $remaining = @($campaign.suites | Select-Object -Skip $index)
            break
        }
        $index++
        $suiteRoot = Join-Path $root $suite.name
        @("RUNNING: suite $index / $($campaign.suites.Count): $($suite.name)", "Progress: $suiteRoot/results/BENCHMARK-STATUS.txt", "Completed suite outcomes: $($outcomes.Count)", "Started: $($started.ToString('o'))") | Set-Content -LiteralPath $status
        & (Get-Process -Id $PID).Path -NoProfile -File (Join-Path $suiteRoot 'start-optimization-suite.ps1') -PreparedSuite $suiteRoot -Run -NoDialog *> (Join-Path $suiteRoot 'runner.log')
        $passed = $LASTEXITCODE -eq 0 -and (Test-Path -LiteralPath (Join-Path $suiteRoot 'BENCHMARK-FINISHED.txt'))
        $outcomes += [pscustomobject]@{suite=$suite.name;verified=$passed;results=(Join-Path $suiteRoot 'results')}
        $outcomes | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $root 'suite-outcomes.json')
    }
    $failed = @($outcomes | Where-Object { -not $_.verified })
    $campaignFailed = $failed.Count -gt 0
    $headline = if ($remaining.Count) { "PAUSED AT EIGHT-HOUR GUARD: $($outcomes.Count) suites attempted; $($remaining.Count) remain." } elseif ($failed.Count) { "FINISHED WITH FAILURES: $($failed.Count) suite(s) need review." } else { 'FINISHED: all suites attempted; exact-result and proof-owner verifiers passed.' }
    $message = @($headline, "Suites failing verification: $($failed.Count)", 'Capped solves remain explicitly incomplete; no candidate is automatically promoted.', "Elapsed: $((Get-Date)-$started)", "Outcomes: $root/suite-outcomes.json", 'Message the Codex task to analyze the campaign.')
    if ($remaining.Count) {
        ConvertTo-Json -InputObject $remaining -Depth 10 | Set-Content -LiteralPath (Join-Path $root 'remaining-suites.json')
        $message | Set-Content -LiteralPath (Join-Path $root 'CAMPAIGN-PAUSED.txt')
    } else { $message | Set-Content -LiteralPath (Join-Path $root 'CAMPAIGN-FINISHED.txt') }
} catch {
    $campaignFailed = $true
    $message = @('CAMPAIGN INTERRUPTED', $_.Exception.Message, "Preserved outcomes: $root/suite-outcomes.json")
    $message | Set-Content -LiteralPath (Join-Path $root 'CAMPAIGN-FAILED.txt')
}
$message | Set-Content -LiteralPath $status
if (-not $NoDialog) { try {
    Add-Type -AssemblyName System.Windows.Forms
    [System.Media.SystemSounds]::Asterisk.Play()
    $title = if ($remaining.Count) { 'Solver optimization campaign paused' } elseif ($campaignFailed) { 'Solver optimization campaign needs attention' } else { 'Solver optimization campaign finished' }
    [System.Windows.Forms.MessageBox]::Show(($message -join [Environment]::NewLine), $title) | Out-Null
} catch { Write-Warning 'Completion dialog unavailable; use CAMPAIGN-STATUS.txt.' } }
if ($campaignFailed) { exit 1 }
