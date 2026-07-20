# @author kongweiguang

[CmdletBinding()]
param(
    [string] $FixturePath,
    [ValidateRange(1, 604800)]
    [int] $DurationSeconds = 28800,
    [string] $OutputDirectory,
    [string] $CargoTargetDirectory,
    [switch] $DebugBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($FixturePath)) {
    $FixturePath = Join-Path $PSScriptRoot '..\target\large-fixtures\mixed-64m.md'
}
if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
    $OutputDirectory = Join-Path $PSScriptRoot `
        ("..\target\soak\interactive-{0}-{1}" -f `
            [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ'), [Guid]::NewGuid().ToString('N'))
}
$fixture = [IO.Path]::GetFullPath($FixturePath)
if (-not (Test-Path -LiteralPath $fixture -PathType Leaf)) {
    throw "Interactive soak fixture not found: $fixture"
}

function Get-FileEvidence {
    param([Parameter(Mandatory = $true)][string] $Path)
    $item = Get-Item -LiteralPath $Path
    $stream = [IO.File]::OpenRead($Path)
    $sha256 = [Security.Cryptography.SHA256]::Create()
    try {
        $hash = $sha256.ComputeHash($stream)
        [pscustomobject]@{
            path = $Path
            size_bytes = [long]$item.Length
            modified_at_utc = $item.LastWriteTimeUtc.ToString('o')
            sha256 = ([BitConverter]::ToString($hash) -replace '-', '').ToLowerInvariant()
        }
    }
    finally {
        $sha256.Dispose()
        $stream.Dispose()
    }
}

$fixtureBefore = Get-FileEvidence $fixture
$output = [IO.Path]::GetFullPath($OutputDirectory)
[IO.Directory]::CreateDirectory($output) | Out-Null
$progress = Join-Path $output 'progress.json'
$stdout = Join-Path $output 'stdout.log'
$stderr = Join-Path $output 'stderr.log'
foreach ($path in @($progress, $stdout, $stderr)) {
    if (Test-Path -LiteralPath $path) {
        throw "Refusing to mix an interactive soak with existing output: $path"
    }
}

$env:GMARK_INTERACTIVE_SOAK_FIXTURE = $fixture
$env:GMARK_INTERACTIVE_SOAK_SECONDS = [string]$DurationSeconds
$env:GMARK_INTERACTIVE_SOAK_PROGRESS = $progress
$previousCargoTargetDir = $env:CARGO_TARGET_DIR
if ([string]::IsNullOrWhiteSpace($CargoTargetDirectory)) {
    $CargoTargetDirectory = Join-Path $PSScriptRoot '..\target\interactive-soak-build'
}
$soakCargoTargetDir = [IO.Path]::GetFullPath($CargoTargetDirectory)
[IO.Directory]::CreateDirectory($soakCargoTargetDir) | Out-Null
$env:CARGO_TARGET_DIR = $soakCargoTargetDir
try {
    $cargoArguments = @('test')
    if (-not $DebugBuild) { $cargoArguments += '--release' }
    $cargoArguments += @(
        '-q',
        'large_document_interactive_soak',
        '--',
        '--ignored',
        '--nocapture',
        '--test-threads=1'
    )
    # Keep the rustup proxy in front of cargo. Launching the toolchain-internal cargo.exe
    # directly loses rustup's child environment and makes every rustc proxy re-sync targets.
    $cargoPath = (Get-Command cargo -ErrorAction Stop).Source
    $startInfo = [Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $cargoPath
    $startInfo.Arguments = $cargoArguments -join ' '
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    $cargoProcess = [Diagnostics.Process]::Start($startInfo)
    $stdoutTask = $cargoProcess.StandardOutput.ReadToEndAsync()
    $stderrTask = $cargoProcess.StandardError.ReadToEndAsync()
    $resourceBaseline = $null
    $maximumRssBytes = [long]0
    $maximumPrivateBytes = [long]0
    $maximumHandleCount = 0
    $maximumThreadCount = 0
    while (-not $cargoProcess.HasExited) {
        # The dedicated target directory identifies this run without relying on the rustup
        # proxy's unstable Windows parent chain or sampling another cargo session.
        foreach ($process in @(Get-Process -Name 'gmark-*' -ErrorAction SilentlyContinue)) {
            try {
                $belongsToCargo = $process.Path.StartsWith(
                    $soakCargoTargetDir,
                    [StringComparison]::OrdinalIgnoreCase
                )
            }
            catch {
                continue
            }
            if (-not $belongsToCargo) {
                continue
            }
            $sample = [pscustomobject]@{
                process_id = $process.Id
                rss_bytes = [long]$process.WorkingSet64
                private_bytes = [long]$process.PrivateMemorySize64
                handle_count = [int]$process.HandleCount
                thread_count = [int]$process.Threads.Count
            }
            if ($null -eq $resourceBaseline) {
                $resourceBaseline = $sample
            }
            $maximumRssBytes = [Math]::Max($maximumRssBytes, $sample.rss_bytes)
            $maximumPrivateBytes = [Math]::Max($maximumPrivateBytes, $sample.private_bytes)
            $maximumHandleCount = [Math]::Max($maximumHandleCount, $sample.handle_count)
            $maximumThreadCount = [Math]::Max($maximumThreadCount, $sample.thread_count)
        }
        # Poll quickly until the short-lived test binary is found, then sample every five
        # seconds so the eight-hour run is not distorted by its own monitor.
        if ($null -eq $resourceBaseline) {
            Start-Sleep -Milliseconds 100
        }
        else {
            Start-Sleep -Seconds 5
        }
        $cargoProcess.Refresh()
    }
    $cargoProcess.WaitForExit()
    $exitCode = $cargoProcess.ExitCode
    $utf8NoBom = [Text.UTF8Encoding]::new($false)
    [IO.File]::WriteAllText($stdout, $stdoutTask.GetAwaiter().GetResult(), $utf8NoBom)
    [IO.File]::WriteAllText($stderr, $stderrTask.GetAwaiter().GetResult(), $utf8NoBom)
    if ($exitCode -ne 0) {
        throw "Interactive soak test failed with exit code $exitCode"
    }
    if ($null -eq $resourceBaseline) {
        throw 'Interactive soak did not expose a child test process for resource sampling'
    }
}
finally {
    Remove-Item Env:GMARK_INTERACTIVE_SOAK_FIXTURE -ErrorAction SilentlyContinue
    Remove-Item Env:GMARK_INTERACTIVE_SOAK_SECONDS -ErrorAction SilentlyContinue
    Remove-Item Env:GMARK_INTERACTIVE_SOAK_PROGRESS -ErrorAction SilentlyContinue
    if ([string]::IsNullOrWhiteSpace($previousCargoTargetDir)) {
        Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue
    }
    else {
        $env:CARGO_TARGET_DIR = $previousCargoTargetDir
    }
}

if (-not (Test-Path -LiteralPath $progress -PathType Leaf)) {
    throw "Interactive soak did not publish progress: $progress"
}
$result = Get-Content -LiteralPath $progress -Raw | ConvertFrom-Json
if (-not [bool]$result.completed) {
    throw "Interactive soak ended without a completed progress record: $progress"
}
$resourceResult = [pscustomobject]@{
    process_id = $resourceBaseline.process_id
    baseline_rss_bytes = $resourceBaseline.rss_bytes
    maximum_rss_bytes = $maximumRssBytes
    rss_growth_bytes = [Math]::Max([long]0, $maximumRssBytes - $resourceBaseline.rss_bytes)
    baseline_private_bytes = $resourceBaseline.private_bytes
    maximum_private_bytes = $maximumPrivateBytes
    private_growth_bytes = [Math]::Max(
        [long]0,
        $maximumPrivateBytes - $resourceBaseline.private_bytes
    )
    baseline_handle_count = $resourceBaseline.handle_count
    maximum_handle_count = $maximumHandleCount
    handle_growth = [Math]::Max(0, $maximumHandleCount - $resourceBaseline.handle_count)
    baseline_thread_count = $resourceBaseline.thread_count
    maximum_thread_count = $maximumThreadCount
    thread_growth = [Math]::Max(0, $maximumThreadCount - $resourceBaseline.thread_count)
}
$resourceExceeded = $resourceResult.rss_growth_bytes -gt 128MB -or
    $resourceResult.private_growth_bytes -gt 192MB -or
    $resourceResult.handle_growth -gt 512 -or
    $resourceResult.thread_growth -gt 64
$result | Add-Member -NotePropertyName process_resources -NotePropertyValue $resourceResult
$fixtureAfter = if (Test-Path -LiteralPath $fixture -PathType Leaf) {
    Get-FileEvidence $fixture
}
else {
    $null
}
$fixtureUnchanged = $null -ne $fixtureAfter -and
    $fixtureAfter.size_bytes -eq $fixtureBefore.size_bytes -and
    $fixtureAfter.modified_at_utc -eq $fixtureBefore.modified_at_utc -and
    $fixtureAfter.sha256 -eq $fixtureBefore.sha256
$result | Add-Member -NotePropertyName fixture_before -NotePropertyValue $fixtureBefore
$result | Add-Member -NotePropertyName fixture_after -NotePropertyValue $fixtureAfter
$result | Add-Member -NotePropertyName fixture_unchanged -NotePropertyValue $fixtureUnchanged
$result | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $progress -Encoding UTF8
$result | ConvertTo-Json -Depth 6
if (-not $fixtureUnchanged) {
    throw 'Interactive soak fixture identity or content changed during the run'
}
if ($resourceExceeded) {
    throw "Interactive soak resource growth exceeded its production budget: $($resourceResult | ConvertTo-Json -Compress)"
}
