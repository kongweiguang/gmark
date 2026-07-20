# @author kongweiguang

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateRange(1, [int]::MaxValue)]
    [int] $WaitForProcessId,
    [Parameter(Mandatory = $true)]
    [string] $FixturePath,
    [Parameter(Mandatory = $true)]
    [string] $OutputDirectory,
    [ValidateRange(1, 604800)]
    [int] $DurationSeconds = 28800
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# Serialize the two release soaks: cargo test cannot replace gmark.exe while idle soak owns it.
Wait-Process -Id $WaitForProcessId -ErrorAction SilentlyContinue
$runner = Join-Path $PSScriptRoot 'run-large-file-interactive-soak.ps1'
& $runner -FixturePath $FixturePath -DurationSeconds $DurationSeconds `
    -OutputDirectory $OutputDirectory
