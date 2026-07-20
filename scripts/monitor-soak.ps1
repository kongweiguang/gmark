# @author kongweiguang

[CmdletBinding()]
param(
    [Alias('Executable')]
    [string] $AppPath = (Join-Path $PSScriptRoot '..\target\release\gmark.exe'),
    [string] $FixturePath,
    [ValidateRange(1, 604800)]
    [int] $DurationSeconds = 28800,
    [ValidateRange(0, 10080)]
    [int] $DurationMinutes = 0,
    [Alias('IntervalSeconds')]
    [ValidateRange(0.1, 300.0)]
    [double] $SampleIntervalSeconds = 5,
    [ValidateRange(0.0, 86400.0)]
    [double] $WarmupSeconds = 30,
    [string] $OutputDirectory = (Join-Path $PSScriptRoot `
        ("..\target\soak\run-{0}-{1}" -f [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ'), `
            [Guid]::NewGuid().ToString('N'))),
    [ValidateRange(1, [long]::MaxValue)]
    [long] $MaxRssBytes = 512MB,
    [ValidateRange(0, [long]::MaxValue)]
    [long] $MaxRssGrowthBytes = 128MB,
    [ValidateRange(1, [long]::MaxValue)]
    [long] $MaxPrivateBytes = 768MB,
    [ValidateRange(0, [long]::MaxValue)]
    [long] $MaxPrivateGrowthBytes = 192MB,
    [ValidateRange(1, [int]::MaxValue)]
    [int] $MaxHandleCount = 4096,
    [ValidateRange(0, [int]::MaxValue)]
    [int] $MaxHandleGrowth = 512,
    [ValidateRange(1, [int]::MaxValue)]
    [int] $MaxThreadCount = 256,
    [ValidateRange(0, [int]::MaxValue)]
    [int] $MaxThreadGrowth = 64,
    [ValidateRange(0.1, 100.0)]
    [double] $MaxCpuPercent = 100,
    [ValidateRange(1, 120)]
    [int] $BudgetBreachSamples = 3,
    [ValidateRange(0, 60)]
    [int] $GracefulShutdownSeconds = 5,
    [bool] $RequireReadyMarker = $true,
    [switch] $StartMinimized
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if ($PSBoundParameters.ContainsKey('DurationMinutes')) {
    if ($PSBoundParameters.ContainsKey('DurationSeconds')) {
        throw 'Specify only DurationSeconds or legacy DurationMinutes, not both'
    }
    if ($DurationMinutes -eq 0) {
        throw 'DurationMinutes must be positive when specified'
    }
    $DurationSeconds = $DurationMinutes * 60
}
if ($WarmupSeconds -ge $DurationSeconds) {
    throw 'WarmupSeconds must be smaller than DurationSeconds'
}

$app = [IO.Path]::GetFullPath($AppPath)
if (-not (Test-Path -LiteralPath $app -PathType Leaf)) {
    throw "Application executable not found: $app"
}
$fixture = $null
if (-not [string]::IsNullOrWhiteSpace($FixturePath)) {
    $fixture = [IO.Path]::GetFullPath($FixturePath)
    if (-not (Test-Path -LiteralPath $fixture -PathType Leaf)) {
        throw "Soak fixture not found: $fixture"
    }
}
$output = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $output -PathType Leaf) {
    throw "OutputDirectory points to a file: $output"
}
[IO.Directory]::CreateDirectory($output) | Out-Null
$samplesPath = Join-Path $output 'samples.jsonl'
$summaryPath = Join-Path $output 'summary.json'
$stdoutPath = Join-Path $output 'stdout.log'
$stderrPath = Join-Path $output 'stderr.log'
$readyPath = Join-Path $output 'ready.json'
foreach ($path in @($samplesPath, $summaryPath, $stdoutPath, $stderrPath, $readyPath)) {
    if (Test-Path -LiteralPath $path) {
        throw "Refusing to mix a soak run with existing output: $path"
    }
}

$runId = [Guid]::NewGuid().ToString('N')
$configRoot = Join-Path $output "isolated-config-$runId"
# Isolate preferences, recovery state, and the single-instance lock from the user's app.
[IO.Directory]::CreateDirectory($configRoot) | Out-Null
$utf8NoBom = [Text.UTF8Encoding]::new($false)
$writer = [IO.StreamWriter]::new($samplesPath, $false, $utf8NoBom)
# Flush every sample so a crash still leaves the last complete JSONL record.
$writer.AutoFlush = $true
$startInfo = $null
$process = $null
$processStarted = $false
$trackedProcessId = $null
$stdoutTask = $null
$stderrTask = $null
$startedAt = $null
$deadline = $null
$sampleCount = 0
$success = $false
$failureReason = $null
$failureDetail = $null
$termination = 'not_started'
$baseline = $null
$lastSample = $null
$ready = $null
$maxRss = 0L
$maxPrivate = $null
$maxHandles = $null
$maxThreads = $null
$maxCpu = 0.0
$maxRssGrowth = 0L
$maxPrivateGrowth = $null
$observedMaxHandleGrowth = $null
$observedMaxThreadGrowth = $null
$previousCpuSeconds = $null
$previousCpuSampleAt = $null
$consecutiveBudgetBreaches = 0
$maxConsecutiveBudgetBreaches = 0
$logicalProcessors = [Math]::Max(1, [Environment]::ProcessorCount)

function Get-Sha256Hex {
    param([Parameter(Mandatory = $true)][string] $Path)
    $stream = [IO.File]::OpenRead($Path)
    $sha256 = [Security.Cryptography.SHA256]::Create()
    try {
        $hash = $sha256.ComputeHash($stream)
        return ([BitConverter]::ToString($hash) -replace '-', '').ToLowerInvariant()
    }
    finally {
        $sha256.Dispose()
        $stream.Dispose()
    }
}

$appSha256 = Get-Sha256Hex $app
$fixtureInfo = if ($null -ne $fixture) {
    $item = Get-Item -LiteralPath $fixture
    [ordered]@{
        path = $fixture
        size_bytes = $item.Length
        modified_at_utc = $item.LastWriteTimeUtc.ToString('o')
        sha256 = Get-Sha256Hex $fixture
    }
}
else {
    $null
}
$fixtureAfter = $null
$fixtureUnchanged = $null

function Get-OptionalLongMetric {
    param([scriptblock] $Read)
    try { return [long](& $Read) }
    catch { return $null }
}

function Set-Failure {
    param([string] $Reason, [string] $Detail)
    if ($null -eq $script:failureReason) {
        $script:failureReason = $Reason
        $script:failureDetail = $Detail
    }
}

try {
    $startInfo = [Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $app
    $startInfo.UseShellExecute = $false
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    if ($StartMinimized) {
        $startInfo.WindowStyle = [Diagnostics.ProcessWindowStyle]::Minimized
    }
    # Windows PowerShell 5.1 lacks ProcessStartInfo.ArgumentList and Environment.
    # Use APIs shared by Windows PowerShell and PowerShell 7.
    $startInfo.EnvironmentVariables['GMARK_UI_CHECK_CONFIG_ROOT'] = $configRoot
    $startInfo.EnvironmentVariables['GMARK_SOAK_MODE'] = 'idle-stability'
    if ($RequireReadyMarker) {
        $startInfo.EnvironmentVariables['GMARK_SOAK_READY_PATH'] = $readyPath
    }
    if ($null -ne $fixtureInfo) {
        # Windows file names cannot contain quotes; quote the absolute path to preserve spaces.
        $startInfo.Arguments = '"' + [string]$fixtureInfo.path + '"'
    }

    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    if (-not $process.Start()) {
        throw "Failed to start $app"
    }
    $trackedProcessId = $process.Id
    $processStarted = $true
    $termination = 'running'
    $stdoutTask = $process.StandardOutput.ReadToEndAsync()
    $stderrTask = $process.StandardError.ReadToEndAsync()
    $startedAt = [DateTime]::UtcNow
    $deadline = $startedAt.AddSeconds($DurationSeconds)

    while ($true) {
        $sampledAt = [DateTime]::UtcNow
        if ($sampledAt -ge $deadline) {
            if ($process.HasExited) {
                Set-Failure 'process_exited_early' "child exited with code $($process.ExitCode)"
            }
            elseif ($RequireReadyMarker -and $null -ne $fixtureInfo -and $null -eq $ready) {
                Set-Failure 'fixture_not_ready' `
                    'large-file Source viewport did not publish a readiness marker'
            }
            else {
                $success = $true
            }
            break
        }
        if ($process.HasExited) {
            Set-Failure 'process_exited_early' "child exited with code $($process.ExitCode)"
            break
        }

        if ($RequireReadyMarker -and $null -ne $fixtureInfo -and $null -eq $ready -and
            (Test-Path -LiteralPath $readyPath -PathType Leaf)) {
            try {
                $ready = Get-Content -LiteralPath $readyPath -Raw | ConvertFrom-Json
                $samePath = [StringComparer]::OrdinalIgnoreCase.Equals(
                    [IO.Path]::GetFullPath([string]$ready.path),
                    [string]$fixtureInfo.path
                )
                if (-not $samePath -or [string]$ready.mode -ne 'source' -or
                    [long]$ready.visible_rows -le 0 -or
                    [long]$ready.file_len -ne [long]$fixtureInfo.size_bytes -or
                    [int]$ready.process_id -ne $trackedProcessId) {
                    Set-Failure 'fixture_ready_mismatch' `
                        'ready marker does not describe the monitored Source viewport'
                }
            }
            catch {
                Set-Failure 'fixture_ready_invalid' $_.Exception.Message
            }
        }

        try {
            $process.Refresh()
            $rss = [long]$process.WorkingSet64
            $privateBytes = Get-OptionalLongMetric { $process.PrivateMemorySize64 }
            $handles = Get-OptionalLongMetric { $process.HandleCount }
            $threads = Get-OptionalLongMetric { $process.Threads.Count }
            $cpuSeconds = $process.TotalProcessorTime.TotalSeconds
            $cpuPercent = 0.0
            if ($null -ne $previousCpuSeconds -and $null -ne $previousCpuSampleAt) {
                $wallSeconds = ($sampledAt - $previousCpuSampleAt).TotalSeconds
                if ($wallSeconds -gt 0) {
                    $cpuPercent = (($cpuSeconds - $previousCpuSeconds) / $wallSeconds) *
                        100.0 / $logicalProcessors
                }
            }
            $previousCpuSeconds = $cpuSeconds
            $previousCpuSampleAt = $sampledAt
            $elapsed = ($sampledAt - $startedAt).TotalSeconds
            $sample = [ordered]@{
                schema_version = 1
                sequence = $sampleCount + 1
                sampled_at_utc = $sampledAt.ToString('o')
                elapsed_seconds = [Math]::Round($elapsed, 3)
                process_alive = $true
                rss_bytes = $rss
                private_bytes = $privateBytes
                handle_count = $handles
                thread_count = $threads
                cpu_total_seconds = [Math]::Round($cpuSeconds, 6)
                cpu_percent = [Math]::Round([Math]::Max(0, $cpuPercent), 3)
            }
            $writer.WriteLine(($sample | ConvertTo-Json -Compress -Depth 4))
            $sampleCount++
            $lastSample = $sample
            $maxRss = [Math]::Max($maxRss, $rss)
            if ($null -ne $privateBytes) {
                $maxPrivate = if ($null -eq $maxPrivate) {
                    $privateBytes
                }
                else {
                    [Math]::Max($maxPrivate, $privateBytes)
                }
            }
            if ($null -ne $handles) {
                $maxHandles = if ($null -eq $maxHandles) { $handles } else { [Math]::Max($maxHandles, $handles) }
            }
            if ($null -ne $threads) {
                $maxThreads = if ($null -eq $maxThreads) { $threads } else { [Math]::Max($maxThreads, $threads) }
            }
            $maxCpu = [Math]::Max($maxCpu, $cpuPercent)

            $rssGrowth = $null
            $privateGrowth = $null
            $handleGrowth = $null
            $threadGrowth = $null
            $budgetBreachReason = $null
            $budgetBreachDetail = $null
            if ($rss -gt $MaxRssBytes) {
                $budgetBreachReason = 'rss_budget_exceeded'
                $budgetBreachDetail = "$rss > $MaxRssBytes"
            }
            elseif ($null -ne $privateBytes -and $privateBytes -gt $MaxPrivateBytes) {
                $budgetBreachReason = 'private_bytes_budget_exceeded'
                $budgetBreachDetail = "$privateBytes > $MaxPrivateBytes"
            }
            elseif ($null -ne $handles -and $handles -gt $MaxHandleCount) {
                $budgetBreachReason = 'handle_budget_exceeded'
                $budgetBreachDetail = "$handles > $MaxHandleCount"
            }
            elseif ($null -ne $threads -and $threads -gt $MaxThreadCount) {
                $budgetBreachReason = 'thread_budget_exceeded'
                $budgetBreachDetail = "$threads > $MaxThreadCount"
            }

            if ($elapsed -ge $WarmupSeconds) {
                if ($null -eq $baseline) {
                    $baseline = $sample
                }
                else {
                    $rssGrowth = [Math]::Max(0L, $rss - [long]$baseline.rss_bytes)
                    $maxRssGrowth = [Math]::Max($maxRssGrowth, $rssGrowth)
                    if ($null -ne $privateBytes -and $null -ne $baseline.private_bytes) {
                        $privateGrowth = [Math]::Max(
                            0L,
                            $privateBytes - [long]$baseline.private_bytes
                        )
                        $maxPrivateGrowth = if ($null -eq $maxPrivateGrowth) {
                            $privateGrowth
                        }
                        else {
                            [Math]::Max($maxPrivateGrowth, $privateGrowth)
                        }
                    }
                    if ($null -ne $handles -and $null -ne $baseline.handle_count) {
                        $handleGrowth = [Math]::Max(0L, $handles - [long]$baseline.handle_count)
                        $observedMaxHandleGrowth = if ($null -eq $observedMaxHandleGrowth) {
                            $handleGrowth
                        }
                        else {
                            [Math]::Max($observedMaxHandleGrowth, $handleGrowth)
                        }
                    }
                    if ($null -ne $threads -and $null -ne $baseline.thread_count) {
                        $threadGrowth = [Math]::Max(0L, $threads - [long]$baseline.thread_count)
                        $observedMaxThreadGrowth = if ($null -eq $observedMaxThreadGrowth) {
                            $threadGrowth
                        }
                        else {
                            [Math]::Max($observedMaxThreadGrowth, $threadGrowth)
                        }
                    }
                }
                if ($null -eq $budgetBreachReason -and $cpuPercent -gt $MaxCpuPercent) {
                    $budgetBreachReason = 'cpu_budget_exceeded'
                    $budgetBreachDetail = "$cpuPercent > $MaxCpuPercent"
                }
                elseif ($null -eq $budgetBreachReason -and
                    $null -ne $rssGrowth -and $rssGrowth -gt $MaxRssGrowthBytes) {
                    $budgetBreachReason = 'rss_growth_budget_exceeded'
                    $budgetBreachDetail = "$rssGrowth > $MaxRssGrowthBytes"
                }
                elseif ($null -eq $budgetBreachReason -and
                    $null -ne $privateGrowth -and $privateGrowth -gt $MaxPrivateGrowthBytes) {
                    $budgetBreachReason = 'private_growth_budget_exceeded'
                    $budgetBreachDetail = "$privateGrowth > $MaxPrivateGrowthBytes"
                }
                elseif ($null -eq $budgetBreachReason -and
                    $null -ne $handleGrowth -and $handleGrowth -gt $MaxHandleGrowth) {
                    $budgetBreachReason = 'handle_growth_budget_exceeded'
                    $budgetBreachDetail = "$handleGrowth > $MaxHandleGrowth"
                }
                elseif ($null -eq $budgetBreachReason -and
                    $null -ne $threadGrowth -and $threadGrowth -gt $MaxThreadGrowth) {
                    $budgetBreachReason = 'thread_growth_budget_exceeded'
                    $budgetBreachDetail = "$threadGrowth > $MaxThreadGrowth"
                }
            }
            # A display-topology or graphics reset can transiently rebuild GPUI/driver resources.
            # Keep recording the absolute maxima, but only classify a budget violation as a leak
            # when current samples remain over budget for a configured consecutive window.
            if ($null -ne $budgetBreachReason) {
                $consecutiveBudgetBreaches++
                $maxConsecutiveBudgetBreaches = [Math]::Max(
                    $maxConsecutiveBudgetBreaches,
                    $consecutiveBudgetBreaches
                )
                if ($consecutiveBudgetBreaches -ge $BudgetBreachSamples) {
                    Set-Failure $budgetBreachReason $budgetBreachDetail
                }
            }
            else {
                $consecutiveBudgetBreaches = 0
            }
            if ($null -ne $failureReason) { break }
        }
        catch {
            Set-Failure 'sample_failed' $_.Exception.Message
            break
        }

        $remainingMs = [Math]::Max(0, ($deadline - [DateTime]::UtcNow).TotalMilliseconds)
        if ($remainingMs -gt 0) {
            $sleepMs = [Math]::Max(
                1,
                [Math]::Min($remainingMs, $SampleIntervalSeconds * 1000)
            )
            Start-Sleep -Milliseconds ([int][Math]::Ceiling($sleepMs))
        }
    }
}
catch {
    Set-Failure 'harness_error' $_.Exception.Message
}
finally {
    $writer.Dispose()
    $exitCode = $null
    if ($null -ne $process -and $processStarted) {
        # Only stop the exact Process returned by this harness; never kill by executable name.
        if (-not $process.HasExited) {
            $termination = 'harness_terminated'
            $closed = $false
            try { $closed = $process.CloseMainWindow() }
            catch { $closed = $false }
            if ($closed -and $GracefulShutdownSeconds -gt 0) {
                $process.WaitForExit($GracefulShutdownSeconds * 1000) | Out-Null
            }
            if (-not $process.HasExited) {
                $process.Kill($true)
                $process.WaitForExit()
            }
        }
        else {
            $termination = 'process_exited'
        }
        $exitCode = $process.ExitCode
    }

    $stdout = if ($null -ne $stdoutTask) { $stdoutTask.GetAwaiter().GetResult() } else { '' }
    $stderr = if ($null -ne $stderrTask) { $stderrTask.GetAwaiter().GetResult() } else { '' }
    [IO.File]::WriteAllText($stdoutPath, $stdout, $utf8NoBom)
    [IO.File]::WriteAllText($stderrPath, $stderr, $utf8NoBom)
    if ($success -and $null -ne $fixtureInfo -and
        $stderr -match "(?m)(failed to read '.+'|file was not opened)") {
        $success = $false
        Set-Failure 'fixture_open_failed' 'application reported that the fixture was not opened'
    }
    $endedAt = [DateTime]::UtcNow
    if ($null -ne $fixtureInfo) {
        if (Test-Path -LiteralPath $fixtureInfo.path -PathType Leaf) {
            $item = Get-Item -LiteralPath $fixtureInfo.path
            $fixtureAfter = [ordered]@{
                path = $fixtureInfo.path
                size_bytes = $item.Length
                modified_at_utc = $item.LastWriteTimeUtc.ToString('o')
                sha256 = Get-Sha256Hex $fixtureInfo.path
            }
            $fixtureUnchanged =
                $fixtureAfter.size_bytes -eq $fixtureInfo.size_bytes -and
                $fixtureAfter.modified_at_utc -eq $fixtureInfo.modified_at_utc -and
                $fixtureAfter.sha256 -eq $fixtureInfo.sha256
        }
        else {
            $fixtureUnchanged = $false
        }
        if (-not $fixtureUnchanged) {
            Set-Failure 'fixture_changed' 'fixture identity or content changed during the soak'
        }
    }
    if ($null -ne $failureReason) { $success = $false }

    $summary = [ordered]@{
        schema_version = 3
        mode = 'idle-stability'
        interactive_driver = $false
        success = $success
        failure_reason = $failureReason
        failure_detail = $failureDetail
        run_id = $runId
        platform = [Environment]::OSVersion.ToString()
        app_path = $app
        app_sha256 = $appSha256
        fixture = $fixtureInfo
        fixture_after = $fixtureAfter
        fixture_unchanged = $fixtureUnchanged
        isolated_config_root = $configRoot
        process_id = $trackedProcessId
        start_arguments = if ($null -ne $startInfo) { $startInfo.Arguments } else { $null }
        started_at_utc = if ($null -ne $startedAt) { $startedAt.ToString('o') } else { $null }
        ended_at_utc = $endedAt.ToString('o')
        requested_duration_seconds = $DurationSeconds
        actual_duration_seconds = if ($null -ne $startedAt) {
            [Math]::Round(($endedAt - $startedAt).TotalSeconds, 3)
        }
        else { 0 }
        sample_interval_seconds = $SampleIntervalSeconds
        warmup_seconds = $WarmupSeconds
        require_ready_marker = $RequireReadyMarker
        start_minimized = [bool]$StartMinimized
        sample_count = $sampleCount
        termination = $termination
        exit_code = $exitCode
        readiness = $ready
        baseline = $baseline
        final_sample = $lastSample
        maxima = [ordered]@{
            rss_bytes = $maxRss
            private_bytes = $maxPrivate
            handle_count = $maxHandles
            thread_count = $maxThreads
            cpu_percent = [Math]::Round($maxCpu, 3)
            rss_growth_bytes = $maxRssGrowth
            private_growth_bytes = $maxPrivateGrowth
            handle_growth = $observedMaxHandleGrowth
            thread_growth = $observedMaxThreadGrowth
            consecutive_budget_breaches = $maxConsecutiveBudgetBreaches
        }
        budgets = [ordered]@{
            max_rss_bytes = $MaxRssBytes
            max_rss_growth_bytes = $MaxRssGrowthBytes
            max_private_bytes = $MaxPrivateBytes
            max_private_growth_bytes = $MaxPrivateGrowthBytes
            max_handle_count = $MaxHandleCount
            max_handle_growth = $MaxHandleGrowth
            max_thread_count = $MaxThreadCount
            max_thread_growth = $MaxThreadGrowth
            max_cpu_percent = $MaxCpuPercent
            required_consecutive_breach_samples = $BudgetBreachSamples
        }
        artifacts = [ordered]@{
            samples_jsonl = $samplesPath
            stdout_log = $stdoutPath
            stderr_log = $stderrPath
            ready_json = $readyPath
        }
    }
    $summaryTemporary = "$summaryPath.tmp-$runId"
    [IO.File]::WriteAllText(
        $summaryTemporary,
        ($summary | ConvertTo-Json -Depth 8) + [Environment]::NewLine,
        $utf8NoBom
    )
    Move-Item -LiteralPath $summaryTemporary -Destination $summaryPath
    if ($null -ne $process) { $process.Dispose() }
}

$summary | ConvertTo-Json -Depth 8
if (-not $success) {
    throw "Idle soak failed: $failureReason ($failureDetail). Summary: $summaryPath"
}
