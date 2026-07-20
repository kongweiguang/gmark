# @author kongweiguang

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Installer,
    [Parameter(Mandatory = $true)]
    [string]$ExpectedVersion
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$installerPath = [IO.Path]::GetFullPath($Installer)
if (-not (Test-Path -LiteralPath $installerPath -PathType Leaf)) {
    throw "Installer does not exist: $installerPath"
}
if ($ExpectedVersion -notmatch '^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$') {
    throw 'ExpectedVersion must be exact SemVer'
}

$root = Join-Path ([IO.Path]::GetTempPath()) ("gmark-installer-smoke-" + [guid]::NewGuid().ToString('N'))
$installDir = Join-Path $root 'install'

function Invoke-HiddenProcess {
    param(
        [Parameter(Mandatory = $true)][string]$FilePath,
        [Parameter(Mandatory = $true)][string[]]$ArgumentList
    )
    $process = Start-Process -FilePath $FilePath -ArgumentList $ArgumentList `
        -Wait -PassThru -WindowStyle Hidden
    if ($process.ExitCode -ne 0) {
        throw "Process failed with exit code $($process.ExitCode): $FilePath"
    }
}

try {
    New-Item -ItemType Directory -Path $root | Out-Null
    $installArguments = @(
        '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/SP-',
        "/DIR=$installDir"
    )
    Invoke-HiddenProcess -FilePath $installerPath -ArgumentList $installArguments

    $executable = Join-Path $installDir 'gmark.exe'
    foreach ($relative in @('gmark.exe', 'README.md', 'PRIVACY.md', 'SECURITY.md', 'LICENSE', 'NOTICE')) {
        $path = Join-Path $installDir $relative
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "Installed payload is missing: $relative"
        }
    }

    $versionFile = Join-Path $root 'version.txt'
    $versionProcess = Start-Process -FilePath $executable -ArgumentList '--version' `
        -RedirectStandardOutput $versionFile -Wait -PassThru -WindowStyle Hidden
    $versionOutput = if (Test-Path -LiteralPath $versionFile) {
        Get-Content -LiteralPath $versionFile -Raw
    }
    else {
        ''
    }
    if ($versionProcess.ExitCode -ne 0 -or $versionOutput -notmatch "(^|\s)$([regex]::Escape($ExpectedVersion))(\s|$)") {
        throw "Installed version mismatch: $versionOutput"
    }

    # The same package must support the update/reinstall transaction.
    Invoke-HiddenProcess -FilePath $installerPath -ArgumentList $installArguments
    if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) {
        throw 'Reinstall removed the executable'
    }

    $uninstaller = Join-Path $installDir 'unins000.exe'
    if (-not (Test-Path -LiteralPath $uninstaller -PathType Leaf)) {
        throw 'Inno Setup uninstaller is missing'
    }
    Invoke-HiddenProcess -FilePath $uninstaller -ArgumentList @(
        '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART'
    )
    if (Test-Path -LiteralPath $executable) {
        throw 'Uninstall left the application executable behind'
    }
}
finally {
    $resolvedTemp = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
    $resolvedRoot = [IO.Path]::GetFullPath($root)
    if ($resolvedRoot.StartsWith($resolvedTemp, [StringComparison]::OrdinalIgnoreCase) -and
        (Split-Path -Leaf $resolvedRoot).StartsWith('gmark-installer-smoke-', [StringComparison]::Ordinal)) {
        Remove-Item -LiteralPath $resolvedRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Write-Host 'Windows installer install/reinstall/uninstall smoke passed.'
