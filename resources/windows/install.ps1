# @author kongweiguang

[CmdletBinding()]
param(
    [string] $InstallDir = (Join-Path $env:LOCALAPPDATA 'Programs\gmark'),
    [switch] $AllowUnsigned,
    [switch] $ForceClose,
    [switch] $NoStartMenuShortcut,
    [switch] $Launch
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Get-GmarkProcessesInDirectory {
    param([Parameter(Mandatory)] [string] $Directory)

    $root = [IO.Path]::GetFullPath($Directory).TrimEnd('\') + '\'
    return @(Get-Process -Name 'gmark' -ErrorAction SilentlyContinue | Where-Object {
        try { $_.Path.StartsWith($root, [StringComparison]::OrdinalIgnoreCase) }
        catch { $false }
    })
}

function Stop-GmarkProcessesInDirectory {
    param(
        [Parameter(Mandatory)] [string] $Directory,
        [switch] $Force
    )

    $processes = @(Get-GmarkProcessesInDirectory -Directory $Directory)
    if ($processes.Count -eq 0) { return }
    if (-not $Force) {
        throw 'gmark is running from the install directory. Close it or pass -ForceClose.'
    }
    $processes | Stop-Process -Force
    $processes | Wait-Process -Timeout 10 -ErrorAction Stop
}

function New-GmarkShortcut {
    param(
        [Parameter(Mandatory)] [string] $ExecutablePath,
        [Parameter(Mandatory)] [string] $ShortcutPath
    )

    New-Item -ItemType Directory -Path (Split-Path -Parent $ShortcutPath) -Force | Out-Null
    $shell = New-Object -ComObject WScript.Shell
    $shortcut = $shell.CreateShortcut($ShortcutPath)
    $shortcut.TargetPath = $ExecutablePath
    $shortcut.WorkingDirectory = Split-Path -Parent $ExecutablePath
    $shortcut.IconLocation = "$ExecutablePath,0"
    $shortcut.Description = 'gmark Markdown Editor'
    $shortcut.Save()
}

$packageRoot = Split-Path -Parent $PSCommandPath
$requiredFiles = @('gmark.exe', 'gmark.Install.psm1', 'uninstall.ps1', 'version.txt')
foreach ($file in $requiredFiles) {
    if (-not (Test-Path -LiteralPath (Join-Path $packageRoot $file) -PathType Leaf)) {
        throw "Package is incomplete: missing $file"
    }
}

$version = (Get-Content -LiteralPath (Join-Path $packageRoot 'version.txt') -Raw).Trim()
if ($version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?$') {
    throw "version.txt does not contain a valid SemVer version: $version"
}

$sourceExecutable = Join-Path $packageRoot 'gmark.exe'
if (-not $AllowUnsigned) {
    $signature = Get-AuthenticodeSignature -LiteralPath $sourceExecutable
    if ($signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid) {
        throw "gmark.exe Authenticode signature is not valid: $($signature.Status)"
    }
}

$install = [IO.Path]::GetFullPath($InstallDir)
$parent = Split-Path -Parent $install
$leaf = Split-Path -Leaf $install
if ([string]::IsNullOrWhiteSpace($leaf) -or [string]::IsNullOrWhiteSpace($parent)) {
    throw "InstallDir must name a child directory: $install"
}

Stop-GmarkProcessesInDirectory -Directory $install -Force:$ForceClose
New-Item -ItemType Directory -Path $parent -Force | Out-Null
$nonce = [Guid]::NewGuid().ToString('N')
$stage = Join-Path $parent "$leaf.stage-$nonce"
$backup = Join-Path $parent "$leaf.backup-$nonce"
$shortcutPath = Join-Path ([Environment]::GetFolderPath('Programs')) 'gmark.lnk'
$swapped = $false
$hadPrevious = Test-Path -LiteralPath $install
$installedExecutable = Join-Path $install 'gmark.exe'
$previousVersion = ''
if ($hadPrevious) {
    $previousMarkerPath = Join-Path $install '.gmark-install'
    if (Test-Path -LiteralPath $previousMarkerPath -PathType Leaf) {
        try {
            $previousMarker = Get-Content -LiteralPath $previousMarkerPath -Raw | ConvertFrom-Json
            if ($previousMarker.product -eq 'gmark') {
                $previousVersion = [string]$previousMarker.version
            }
        }
        catch {
            $previousVersion = ''
        }
    }
}

try {
    New-Item -ItemType Directory -Path $stage | Out-Null
    foreach ($item in Get-ChildItem -LiteralPath $packageRoot -Force) {
        if ($item.Name -notin @('install.ps1', '.gmark-install')) {
            Copy-Item -LiteralPath $item.FullName -Destination $stage -Recurse -Force
        }
    }
    Copy-Item -LiteralPath $PSCommandPath -Destination (Join-Path $stage 'install.ps1') -Force

    $marker = [ordered]@{
        schema = 1
        product = 'gmark'
        version = $version
        install_location = $install
        installed_at_utc = [DateTime]::UtcNow.ToString('o')
    }
    $marker | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $stage '.gmark-install') `
        -Encoding UTF8

    if ($hadPrevious) { Move-Item -LiteralPath $install -Destination $backup }
    Move-Item -LiteralPath $stage -Destination $install
    $swapped = $true

    Import-Module (Join-Path $install 'gmark.Install.psm1') -Force
    $uninstallScript = Join-Path $install 'uninstall.ps1'
    $uninstallCommand = 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File "{0}"' -f `
        $uninstallScript
    Register-GmarkFileAssociations -ExecutablePath $installedExecutable `
        -InstallLocation $install -Version $version -UninstallCommand $uninstallCommand
    if (-not $NoStartMenuShortcut) {
        New-GmarkShortcut -ExecutablePath $installedExecutable -ShortcutPath $shortcutPath
    }

    Remove-Item -LiteralPath $backup -Recurse -Force -ErrorAction SilentlyContinue
    Write-Host "gmark $version installed to $install"
}
catch {
    $installError = $_
    if ($swapped -and (Get-Command Unregister-GmarkFileAssociations `
        -ErrorAction SilentlyContinue)) {
        Unregister-GmarkFileAssociations -ExecutablePath $installedExecutable `
            -ErrorAction SilentlyContinue
    }
    if ($swapped -and (Test-Path -LiteralPath $install)) {
        Remove-Item -LiteralPath $install -Recurse -Force -ErrorAction SilentlyContinue
    }
    if (Test-Path -LiteralPath $backup) {
        Move-Item -LiteralPath $backup -Destination $install -ErrorAction SilentlyContinue
    }
    if ($hadPrevious -and $previousVersion -match '^\d+\.\d+\.\d+') {
        $previousUninstall = Join-Path $install 'uninstall.ps1'
        $previousCommand = 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File "{0}"' -f `
            $previousUninstall
        Register-GmarkFileAssociations -ExecutablePath $installedExecutable `
            -InstallLocation $install -Version $previousVersion -UninstallCommand $previousCommand `
            -ErrorAction SilentlyContinue
    }
    if (-not $hadPrevious -and (Test-Path -LiteralPath $shortcutPath)) {
        try {
            $shell = New-Object -ComObject WScript.Shell
            $shortcut = $shell.CreateShortcut($shortcutPath)
            if ([IO.Path]::GetFullPath($shortcut.TargetPath) -eq $installedExecutable) {
                Remove-Item -LiteralPath $shortcutPath -Force -ErrorAction SilentlyContinue
            }
        }
        catch {}
    }
    throw $installError
}
finally {
    Remove-Item -LiteralPath $stage -Recurse -Force -ErrorAction SilentlyContinue
}

if ($Launch) { Start-Process -FilePath $installedExecutable }
