# @author kongweiguang

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string] $FixturePath,
    [Parameter(Mandatory = $true)]
    [string] $OutputDirectory,
    [ValidateRange(1, 604800)]
    [int] $DurationSeconds = 28800,
    [string] $CargoTargetDirectory
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$fixture = [IO.Path]::GetFullPath($FixturePath)
if (-not (Test-Path -LiteralPath $fixture -PathType Leaf)) {
    throw "Idle soak fixture not found: $fixture"
}
$output = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $output) {
    throw "Refusing to reuse idle soak output: $output"
}
if ([string]::IsNullOrWhiteSpace($CargoTargetDirectory)) {
    $CargoTargetDirectory = Join-Path $PSScriptRoot '..\target\idle-soak-build'
}
$target = [IO.Path]::GetFullPath($CargoTargetDirectory)
[IO.Directory]::CreateDirectory($target) | Out-Null

$previousCargoTargetDir = $env:CARGO_TARGET_DIR
$env:CARGO_TARGET_DIR = $target
try {
    & cargo build --release --bin gmark
    if ($LASTEXITCODE -ne 0) {
        throw "Release build failed with exit code $LASTEXITCODE"
    }
}
finally {
    if ([string]::IsNullOrWhiteSpace($previousCargoTargetDir)) {
        Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue
    }
    else {
        $env:CARGO_TARGET_DIR = $previousCargoTargetDir
    }
}

$app = Join-Path $target 'release\gmark.exe'
if (-not (Test-Path -LiteralPath $app -PathType Leaf)) {
    throw "Release build did not produce: $app"
}
$monitor = Join-Path $PSScriptRoot 'monitor-soak.ps1'
& $monitor -AppPath $app -FixturePath $fixture -DurationSeconds $DurationSeconds `
    -OutputDirectory $output -StartMinimized
if ($LASTEXITCODE -ne 0) {
    throw "Idle soak monitor failed with exit code $LASTEXITCODE"
}
