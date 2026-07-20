# @author kongweiguang

[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Assert-True {
    param([bool] $Condition, [string] $Message)
    if (-not $Condition) { throw "Assertion failed: $Message" }
}

function Invoke-ExpectedFailure {
    param([scriptblock] $Action, [string] $Message)
    $failed = $false
    try { $null = & $Action }
    catch { $failed = $true }
    Assert-True $failed $Message
}

$sandbox = Join-Path $env:TEMP ("gmark-soak-test-{0}" -f [Guid]::NewGuid().ToString('N'))
$sandbox = [IO.Path]::GetFullPath($sandbox)
$tempRoot = [IO.Path]::GetFullPath($env:TEMP).TrimEnd('\') + '\'
$sentinel = $null
if (-not $sandbox.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Refusing soak test sandbox outside temporary directory'
}

try {
    New-Item -ItemType Directory -Path $sandbox -Force | Out-Null
    $app = (Get-Process -Id $PID).Path
    $sleepFixture = Join-Path $sandbox 'sleep.ps1'
    $exitFixture = Join-Path $sandbox 'exit.ps1'
    $mutatingFixture = Join-Path $sandbox 'mutating.ps1'
    Set-Content -LiteralPath $sleepFixture -Value 'Start-Sleep -Seconds 30' -Encoding utf8
    Set-Content -LiteralPath $exitFixture -Value 'exit 7' -Encoding utf8
    Set-Content -LiteralPath $mutatingFixture -Encoding utf8 -Value @'
$path = $MyInvocation.MyCommand.Path
Start-Sleep -Milliseconds 500
[IO.File]::AppendAllText($path, "`n# changed during soak")
Start-Sleep -Seconds 30
'@
    # 同名哨兵进程证明 harness 只终止 Start 返回的 PID，不按可执行文件名清理。
    $sentinel = Start-Process -FilePath $app `
        -ArgumentList @('-NoProfile', '-Command', 'Start-Sleep -Seconds 30') `
        -PassThru -WindowStyle Hidden

    $successOutput = Join-Path $sandbox 'success'
    $null = & (Join-Path $PSScriptRoot 'monitor-soak.ps1') -AppPath $app `
        -FixturePath $sleepFixture `
        -DurationSeconds 2 -SampleIntervalSeconds 0.25 -WarmupSeconds 0.25 `
        -OutputDirectory $successOutput -MaxRssBytes 1GB -MaxRssGrowthBytes 1GB `
        -MaxHandleCount 10000 -MaxHandleGrowth 10000 -MaxThreadCount 1000 `
        -MaxThreadGrowth 1000 -MaxCpuPercent 100 -RequireReadyMarker:$false
    $summary = Get-Content -LiteralPath (Join-Path $successOutput 'summary.json') -Raw |
        ConvertFrom-Json
    Assert-True $summary.success 'short idle soak must succeed'
    Assert-True ($summary.mode -eq 'idle-stability') 'summary must identify idle soak'
    Assert-True ($summary.sample_count -ge 4) 'short soak must stream several samples'
    Assert-True ($summary.schema_version -eq 3) 'summary schema must include fixture integrity'
    Assert-True $summary.fixture_unchanged 'short soak must prove fixture integrity'
    Assert-True ($summary.fixture.sha256 -eq $summary.fixture_after.sha256) `
        'fixture hashes before and after the soak must match'
    Assert-True ($summary.budgets.max_handle_growth -eq 10000) `
        'handle growth budget must not be overwritten by observed growth'
    Assert-True ($summary.budgets.max_thread_growth -eq 1000) `
        'thread growth budget must not be overwritten by observed growth'
    Assert-True (-not (Get-Process -Id $summary.process_id -ErrorAction SilentlyContinue)) `
        'harness must terminate the exact child process it started'
    Assert-True ($null -ne (Get-Process -Id $sentinel.Id -ErrorAction SilentlyContinue)) `
        'harness must not terminate an unrelated process with the same executable'
    $sampleLines = @(Get-Content -LiteralPath (Join-Path $successOutput 'samples.jsonl'))
    Assert-True ($sampleLines.Count -eq $summary.sample_count) `
        'JSONL line count must equal summary sample count'
    $firstSample = $sampleLines[0] | ConvertFrom-Json
    Assert-True ($firstSample.rss_bytes -gt 0) 'sample must contain RSS'
    Assert-True ($null -ne $firstSample.cpu_percent) 'sample must contain derived CPU percent'

    $earlyOutput = Join-Path $sandbox 'early-exit'
    Invoke-ExpectedFailure {
        & (Join-Path $PSScriptRoot 'monitor-soak.ps1') -AppPath $app `
            -FixturePath $exitFixture -DurationSeconds 2 -SampleIntervalSeconds 0.25 `
            -WarmupSeconds 0 -OutputDirectory $earlyOutput -RequireReadyMarker:$false
    } 'early process exit must fail closed'
    $earlySummary = Get-Content -LiteralPath (Join-Path $earlyOutput 'summary.json') -Raw |
        ConvertFrom-Json
    Assert-True (-not $earlySummary.success) 'early-exit summary must fail'
    Assert-True ($earlySummary.failure_reason -eq 'process_exited_early') `
        'early-exit reason must be explicit'
    Assert-True ($earlySummary.exit_code -eq 7) 'real child exit code must be preserved'

    $budgetOutput = Join-Path $sandbox 'budget'
    Invoke-ExpectedFailure {
        & (Join-Path $PSScriptRoot 'monitor-soak.ps1') -AppPath $app `
            -FixturePath $sleepFixture -DurationSeconds 2 -SampleIntervalSeconds 0.25 `
            -WarmupSeconds 0 -OutputDirectory $budgetOutput -MaxRssBytes 1 `
            -RequireReadyMarker:$false
    } 'RSS over budget must fail closed'
    $budgetSummary = Get-Content -LiteralPath (Join-Path $budgetOutput 'summary.json') -Raw |
        ConvertFrom-Json
    Assert-True ($budgetSummary.failure_reason -eq 'rss_budget_exceeded') `
        'resource failure reason must be explicit'

    $mutationOutput = Join-Path $sandbox 'fixture-mutation'
    Invoke-ExpectedFailure {
        & (Join-Path $PSScriptRoot 'monitor-soak.ps1') -AppPath $app `
            -FixturePath $mutatingFixture -DurationSeconds 2 -SampleIntervalSeconds 0.25 `
            -WarmupSeconds 0 -OutputDirectory $mutationOutput -RequireReadyMarker:$false
    } 'fixture mutation must fail closed'
    $mutationSummary = Get-Content -LiteralPath (Join-Path $mutationOutput 'summary.json') -Raw |
        ConvertFrom-Json
    Assert-True (-not $mutationSummary.success) 'fixture-mutation summary must fail'
    Assert-True ($mutationSummary.failure_reason -eq 'fixture_changed') `
        'fixture mutation reason must be explicit'
    Assert-True (-not $mutationSummary.fixture_unchanged) `
        'fixture mutation must be recorded in the summary'
    Assert-True ($mutationSummary.fixture.sha256 -ne $mutationSummary.fixture_after.sha256) `
        'fixture mutation must preserve distinct before and after hashes'

    Invoke-ExpectedFailure {
        & (Join-Path $PSScriptRoot 'monitor-soak.ps1') -AppPath $app `
            -FixturePath (Join-Path $sandbox 'missing.md') -DurationSeconds 1
    } 'missing fixture must fail before process launch'
    Invoke-ExpectedFailure {
        & (Join-Path $PSScriptRoot 'monitor-soak.ps1') -AppPath $app `
            -FixturePath $sleepFixture -DurationSeconds 0
    } 'zero duration must be rejected'

    Write-Host 'Real-process soak parser, lifecycle, and fail-closed tests passed'
}
finally {
    if ($null -ne $sentinel -and -not $sentinel.HasExited) {
        Stop-Process -Id $sentinel.Id -Force -ErrorAction SilentlyContinue
        $sentinel.WaitForExit(5000) | Out-Null
    }
    if ($null -ne $sentinel) { $sentinel.Dispose() }
    if (Test-Path -LiteralPath $sandbox) {
        $resolved = [IO.Path]::GetFullPath($sandbox)
        if (-not $resolved.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase)) {
            throw 'Refusing to remove soak test sandbox outside temporary directory'
        }
        Remove-Item -LiteralPath $resolved -Recurse -Force -ErrorAction SilentlyContinue
    }
}
