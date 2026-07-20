# @author kongweiguang

[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string] $Archive,
    [string] $ExpectedSignerThumbprint = $env:WINDOWS_SIGNING_CERTIFICATE_THUMBPRINT,
    [switch] $DryRun,
    [switch] $UnsignedDev
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if ($DryRun -and $UnsignedDev) {
    throw '-DryRun and -UnsignedDev are mutually exclusive'
}
if ($env:GMARK_RELEASE_MODE -eq 'production' -and ($DryRun -or $UnsignedDev)) {
    throw 'DryRun and UnsignedDev are forbidden when GMARK_RELEASE_MODE=production'
}
$archivePath = [IO.Path]::GetFullPath($Archive)
if (-not (Test-Path -LiteralPath $archivePath -PathType Leaf)) {
    throw "Windows release archive does not exist: $archivePath"
}

$sandbox = Join-Path $env:TEMP ("gmark-clean-smoke-{0}" -f [Guid]::NewGuid().ToString('N'))
$sandbox = [IO.Path]::GetFullPath($sandbox)
$tempRoot = [IO.Path]::GetFullPath($env:TEMP).TrimEnd('\') + '\'
if (-not $sandbox.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Refusing to create smoke sandbox outside the temporary directory'
}
$package = Join-Path $sandbox 'package'
$install = Join-Path $sandbox 'installed'

try {
    New-Item -ItemType Directory -Path $package -Force | Out-Null
    Expand-Archive -LiteralPath $archivePath -DestinationPath $package -Force
    $required = @(
        'gmark.exe', 'install.ps1', 'uninstall.ps1', 'gmark.Install.psm1', 'version.txt'
    )
    foreach ($relative in $required) {
        if (-not (Test-Path -LiteralPath (Join-Path $package $relative) -PathType Leaf)) {
            throw "Windows release archive is missing $relative"
        }
    }
    $version = (Get-Content -LiteralPath (Join-Path $package 'version.txt') -Raw).Trim()
    if ($version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?$') {
        throw 'Windows release archive contains an invalid version.txt'
    }

    if (-not $UnsignedDev -and -not $DryRun) {
        $expectedThumbprint = ($ExpectedSignerThumbprint -replace '\s', '').ToUpperInvariant()
        if ($expectedThumbprint -notmatch '^(?:[0-9A-F]{40}|[0-9A-F]{64})$') {
            throw 'Production Windows smoke requires a reviewed signer thumbprint'
        }
        foreach ($relative in @('gmark.exe', 'install.ps1', 'uninstall.ps1', 'gmark.Install.psm1')) {
            $signature = Get-AuthenticodeSignature -LiteralPath (Join-Path $package $relative)
            if ($signature.Status -ne [Management.Automation.SignatureStatus]::Valid) {
                throw "Authenticode verification failed for ${relative}: $($signature.Status)"
            }
            if ($null -eq $signature.SignerCertificate -or
                $signature.SignerCertificate.Thumbprint.ToUpperInvariant() -ne $expectedThumbprint) {
                throw "Authenticode signer mismatch for $relative"
            }
            if ($null -eq $signature.TimeStamperCertificate) {
                throw "Authenticode timestamp is missing for $relative"
            }
        }
    }
    if ($DryRun) {
        Write-Host "dry-run: Windows archive layout is valid; install/uninstall was not executed"
        return
    }

    $installArguments = @{
        InstallDir = $install
        NoStartMenuShortcut = $true
        AllowUnsigned = $UnsignedDev
    }
    & (Join-Path $package 'install.ps1') @installArguments
    $markerPath = Join-Path $install '.gmark-install'
    $marker = Get-Content -LiteralPath $markerPath -Raw | ConvertFrom-Json
    if ($marker.product -ne 'gmark' -or $marker.version -ne $version) {
        throw 'Installed marker does not match the release package'
    }

    # 同版本再次安装走完整 staging/swap 路径，覆盖 clean VM 的升级事务边界。
    & (Join-Path $package 'install.ps1') @installArguments
    $uninstallScript = Join-Path $install 'uninstall.ps1'
    $arguments = '-NoProfile -ExecutionPolicy Bypass -File "{0}"' -f $uninstallScript
    $process = Start-Process -FilePath 'powershell.exe' -ArgumentList $arguments -Wait -PassThru `
        -WindowStyle Hidden
    if ($process.ExitCode -ne 0) {
        throw "Windows uninstaller exited with $($process.ExitCode)"
    }
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    while ((Test-Path -LiteralPath $install) -and [DateTime]::UtcNow -lt $deadline) {
        Start-Sleep -Milliseconds 100
    }
    if (Test-Path -LiteralPath $install) {
        throw 'Windows uninstaller did not remove the installation'
    }
    Write-Host "Windows clean-runner install/reinstall/uninstall smoke passed for $version"
}
finally {
    $cleanupUninstaller = Join-Path $install 'uninstall.ps1'
    if ((Test-Path -LiteralPath $cleanupUninstaller -PathType Leaf) -and
        (Test-Path -LiteralPath (Join-Path $install '.gmark-install') -PathType Leaf)) {
        $cleanupArguments = '-NoProfile -ExecutionPolicy Bypass -File "{0}"' -f `
            $cleanupUninstaller
        Start-Process -FilePath 'powershell.exe' -ArgumentList $cleanupArguments -Wait `
            -WindowStyle Hidden -ErrorAction SilentlyContinue | Out-Null
    }
    if (Test-Path -LiteralPath $sandbox) {
        $resolved = [IO.Path]::GetFullPath($sandbox)
        if (-not $resolved.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase)) {
            throw 'Refusing to remove smoke sandbox outside the temporary directory'
        }
        Remove-Item -LiteralPath $resolved -Recurse -Force -ErrorAction SilentlyContinue
    }
}
