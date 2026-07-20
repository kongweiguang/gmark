# @author kongweiguang

[CmdletBinding()]
param(
    [switch] $ForceClose
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$install = [IO.Path]::GetFullPath((Split-Path -Parent $PSCommandPath))
$markerPath = Join-Path $install '.gmark-install'
if (-not (Test-Path -LiteralPath $markerPath -PathType Leaf)) {
    throw 'Refusing to uninstall: .gmark-install marker is missing.'
}
$marker = Get-Content -LiteralPath $markerPath -Raw | ConvertFrom-Json
if ($marker.schema -ne 1 -or $marker.product -ne 'gmark' -or `
    [IO.Path]::GetFullPath([string]$marker.install_location) -ne $install) {
    throw 'Refusing to uninstall: installation marker is invalid or belongs to another directory.'
}

$executable = Join-Path $install 'gmark.exe'
$running = @(Get-Process -Name 'gmark' -ErrorAction SilentlyContinue | Where-Object {
    try { [IO.Path]::GetFullPath($_.Path) -eq $executable }
    catch { $false }
})
if ($running.Count -gt 0) {
    if (-not $ForceClose) {
        throw 'gmark is running. Close it or pass -ForceClose.'
    }
    $running | Stop-Process -Force
    $running | Wait-Process -Timeout 10 -ErrorAction Stop
}

Import-Module (Join-Path $install 'gmark.Install.psm1') -Force
Unregister-GmarkFileAssociations -ExecutablePath $executable

$shortcutPath = Join-Path ([Environment]::GetFolderPath('Programs')) 'gmark.lnk'
if (Test-Path -LiteralPath $shortcutPath -PathType Leaf) {
    $shell = New-Object -ComObject WScript.Shell
    $shortcut = $shell.CreateShortcut($shortcutPath)
    if ([IO.Path]::GetFullPath($shortcut.TargetPath) -eq $executable) {
        Remove-Item -LiteralPath $shortcutPath -Force
    }
}

# 在安装目录外启动清理进程，避免当前 PowerShell 对脚本或模块的文件锁影响目录删除。
$cleanup = Join-Path $env:TEMP ("gmark-uninstall-{0}.ps1" -f [Guid]::NewGuid().ToString('N'))
$cleanupBody = @'
param([int] $ParentPid, [string] $InstallDir, [string] $CleanupPath)
try { Wait-Process -Id $ParentPid -Timeout 30 -ErrorAction SilentlyContinue } catch {}
Remove-Item -LiteralPath $InstallDir -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item -LiteralPath $CleanupPath -Force -ErrorAction SilentlyContinue
'@
Set-Content -LiteralPath $cleanup -Value $cleanupBody -Encoding UTF8
$cleanupArguments = '-NoProfile -ExecutionPolicy Bypass -File "{0}" -ParentPid {1} ' +
    '-InstallDir "{2}" -CleanupPath "{0}"'
$cleanupArguments = $cleanupArguments -f $cleanup, $PID, $install
Start-Process -FilePath 'powershell.exe' -WindowStyle Hidden -ArgumentList $cleanupArguments
Write-Host "gmark has been unregistered and $install is scheduled for removal."
