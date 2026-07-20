# @author kongweiguang

[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string] $PackageDir,
    [string] $CertificatePath,
    [string] $CertificatePasswordEnv = 'WINDOWS_SIGNING_CERTIFICATE_PASSWORD',
    [string] $ExpectedSignerThumbprint = $env:WINDOWS_SIGNING_CERTIFICATE_THUMBPRINT,
    [string] $TimestampUrl = 'http://timestamp.digicert.com',
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

$package = [IO.Path]::GetFullPath($PackageDir)
$relativeFiles = @('gmark.exe', 'install.ps1', 'uninstall.ps1', 'gmark.Install.psm1')
$files = foreach ($relative in $relativeFiles) {
    $path = Join-Path $package $relative
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Windows package is missing $relative"
    }
    Get-Item -LiteralPath $path
}

if ($DryRun) {
    Write-Host "dry-run: would Authenticode-sign and verify $($files.Count) Windows payload files"
    return
}
if ($UnsignedDev) {
    Write-Warning 'UNSIGNED DEV: no Authenticode signatures were emitted'
    return
}
if ([string]::IsNullOrWhiteSpace($CertificatePath) -or
    -not (Test-Path -LiteralPath $CertificatePath -PathType Leaf)) {
    throw 'Production Windows signing requires -CertificatePath'
}
$password = [Environment]::GetEnvironmentVariable($CertificatePasswordEnv)
if ([string]::IsNullOrWhiteSpace($password)) {
    throw "Production Windows signing requires secret env $CertificatePasswordEnv"
}
$expectedThumbprint = ($ExpectedSignerThumbprint -replace '\s', '').ToUpperInvariant()
if ($expectedThumbprint -notmatch '^(?:[0-9A-F]{40}|[0-9A-F]{64})$') {
    throw 'Production Windows signing requires a reviewed signer thumbprint'
}

$flags = [Security.Cryptography.X509Certificates.X509KeyStorageFlags]::EphemeralKeySet
$certificate = [Security.Cryptography.X509Certificates.X509Certificate2]::new(
    [IO.Path]::GetFullPath($CertificatePath),
    $password,
    $flags
)
try {
    if (-not $certificate.HasPrivateKey) {
        throw 'Windows signing certificate does not contain a private key'
    }
    if ($certificate.Thumbprint.ToUpperInvariant() -ne $expectedThumbprint) {
        throw 'Windows signing PFX does not match the reviewed signer thumbprint'
    }
    foreach ($file in $files) {
        $result = Set-AuthenticodeSignature -LiteralPath $file.FullName -Certificate $certificate `
            -HashAlgorithm SHA256 -TimestampServer $TimestampUrl
        if ($result.Status -ne [Management.Automation.SignatureStatus]::Valid) {
            throw "Authenticode signing failed for $($file.Name): $($result.Status)"
        }
        if ($null -eq $result.SignerCertificate -or
            $result.SignerCertificate.Thumbprint.ToUpperInvariant() -ne $expectedThumbprint) {
            throw "Authenticode signer mismatch for $($file.Name)"
        }
        if ($null -eq $result.TimeStamperCertificate) {
            throw "Authenticode timestamp is missing for $($file.Name)"
        }
        $verified = Get-AuthenticodeSignature -LiteralPath $file.FullName
        if ($verified.Status -ne [Management.Automation.SignatureStatus]::Valid) {
            throw "Authenticode verification failed for $($file.Name): $($verified.Status)"
        }
        if ($null -eq $verified.SignerCertificate -or
            $verified.SignerCertificate.Thumbprint.ToUpperInvariant() -ne $expectedThumbprint) {
            throw "Verified Authenticode signer mismatch for $($file.Name)"
        }
        if ($null -eq $verified.TimeStamperCertificate) {
            throw "Verified Authenticode timestamp is missing for $($file.Name)"
        }
    }
}
finally {
    $certificate.Dispose()
    $password = $null
}

Write-Host "Authenticode signed and verified $($files.Count) Windows payload files"
