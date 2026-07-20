# @author kongweiguang

[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$sandbox = Join-Path $env:TEMP ("gmark-release-script-test-{0}" -f [Guid]::NewGuid().ToString('N'))
$sandbox = [IO.Path]::GetFullPath($sandbox)
$tempRoot = [IO.Path]::GetFullPath($env:TEMP).TrimEnd('\') + '\'
if (-not $sandbox.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Refusing test sandbox outside temporary directory'
}
$package = Join-Path $sandbox 'package'
$archive = Join-Path $sandbox 'gmark-v0.1.0-windows-x86_64.zip'
try {
    New-Item -ItemType Directory -Path $package -Force | Out-Null
    Set-Content -LiteralPath (Join-Path $package 'gmark.exe') -Value 'unsigned-dev-binary' `
        -NoNewline
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot '..\resources\windows\install.ps1') `
        -Destination $package
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot '..\resources\windows\uninstall.ps1') `
        -Destination $package
    $fakeModule = @'
function Register-GmarkFileAssociations { param($ExecutablePath, $InstallLocation, $Version, $UninstallCommand) }
function Unregister-GmarkFileAssociations { param($ExecutablePath) }
Export-ModuleMember -Function Register-GmarkFileAssociations, Unregister-GmarkFileAssociations
'@
    Set-Content -LiteralPath (Join-Path $package 'gmark.Install.psm1') -Value $fakeModule
    Set-Content -LiteralPath (Join-Path $package 'version.txt') -Value '0.1.0' -NoNewline

    & (Join-Path $PSScriptRoot 'sign-windows-package.ps1') -PackageDir $package -DryRun
    & (Join-Path $PSScriptRoot 'sign-windows-package.ps1') -PackageDir $package -UnsignedDev
    Compress-Archive -Path (Join-Path $package '*') -DestinationPath $archive -Force
    & (Join-Path $PSScriptRoot 'smoke-windows-package.ps1') -Archive $archive -DryRun
    & (Join-Path $PSScriptRoot 'smoke-windows-package.ps1') -Archive $archive -UnsignedDev

    $previousMode = $env:GMARK_RELEASE_MODE
    try {
        $env:GMARK_RELEASE_MODE = 'production'
        $failed = $false
        try {
            & (Join-Path $PSScriptRoot 'sign-windows-package.ps1') -PackageDir $package `
                -UnsignedDev
        }
        catch {
            $failed = $true
        }
        if (-not $failed) {
            throw 'Production mode accepted an unsigned Windows package'
        }
    }
    finally {
        $env:GMARK_RELEASE_MODE = $previousMode
    }
    Write-Host 'Windows release signing and clean-smoke dry-run tests passed'
}
finally {
    if (Test-Path -LiteralPath $sandbox) {
        $resolved = [IO.Path]::GetFullPath($sandbox)
        if (-not $resolved.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase)) {
            throw 'Refusing to remove test sandbox outside temporary directory'
        }
        Remove-Item -LiteralPath $resolved -Recurse -Force -ErrorAction SilentlyContinue
    }
}
