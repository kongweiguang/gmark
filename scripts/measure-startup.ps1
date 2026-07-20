# @author kongweiguang

[CmdletBinding()]
param(
    [string]$Executable = (Join-Path $PSScriptRoot "..\target\release\gmark.exe"),
    [ValidateRange(1, 1000)]
    [int]$Samples = 30,
    [ValidateRange(1000, 120000)]
    [int]$TimeoutMs = 15000,
    [string]$OutputPath = (Join-Path $PSScriptRoot "..\target\startup-samples.json")
)

$ErrorActionPreference = "Stop"
$Executable = [System.IO.Path]::GetFullPath($Executable)
if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) {
    throw "Release executable not found: $Executable"
}

function Get-Percentile([double[]]$Values, [double]$Percentile) {
    $sorted = @($Values | Sort-Object)
    $index = [Math]::Ceiling($Percentile * $sorted.Count) - 1
    return $sorted[[Math]::Max(0, [Math]::Min($index, $sorted.Count - 1))]
}

$measurements = @()
for ($sample = 1; $sample -le $Samples; $sample++) {
    $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $Executable
    $startInfo.UseShellExecute = $false
    $startInfo.RedirectStandardError = $true
    $startInfo.Environment["GMARK_PERF_TRACE"] = "1"

    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
    if (-not $process.Start()) {
        throw "Failed to start $Executable"
    }

    try {
        $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMs)
        $ready = $false
        $read = $process.StandardError.ReadLineAsync()
        while ([DateTime]::UtcNow -lt $deadline -and -not $process.HasExited) {
            if (-not $read.Wait(250)) {
                continue
            }
            $line = $read.Result
            if ($null -eq $line) {
                break
            }
            if ($line -match '^gmark_perf (.+)$') {
                $record = $Matches[1] | ConvertFrom-Json
                if ($record.event -eq "editor_first_render") {
                    $ready = $true
                    break
                }
            }
            $read = $process.StandardError.ReadLineAsync()
        }
        if (-not $ready) {
            throw "Sample $sample did not reach editor_first_render within ${TimeoutMs}ms"
        }
        $stopwatch.Stop()
        $measurements += [Math]::Round($stopwatch.Elapsed.TotalMilliseconds, 3)
    }
    finally {
        if (-not $process.HasExited) {
            $process.Kill($true)
            $process.WaitForExit()
        }
        $process.Dispose()
    }
}

$result = [ordered]@{
    schema_version = 1
    executable = $Executable
    measured_at_utc = [DateTime]::UtcNow.ToString("o")
    boundary = "process launch to GPUI editor_first_render; not platform present"
    samples_ms = $measurements
    p50_ms = [Math]::Round((Get-Percentile $measurements 0.50), 3)
    p95_ms = [Math]::Round((Get-Percentile $measurements 0.95), 3)
    p99_ms = [Math]::Round((Get-Percentile $measurements 0.99), 3)
}
$OutputPath = [System.IO.Path]::GetFullPath($OutputPath)
[System.IO.Directory]::CreateDirectory([System.IO.Path]::GetDirectoryName($OutputPath)) | Out-Null
$result | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $OutputPath -Encoding utf8
$result | ConvertTo-Json -Depth 4
