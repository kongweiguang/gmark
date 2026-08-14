# @author kongweiguang

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Version,
    [Parameter(Mandatory = $true)]
    [string]$OutputDirectory,
    [string]$VpkPath = ''
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

# 原因：发布脚本会删除自己的临时 staging，先固定到仓库与输出目录可防止变量错误扩大范围。
function Resolve-SafeDirectory {
    param([string]$Path)

    $resolved = [IO.Path]::GetFullPath($Path)
    if ([string]::IsNullOrWhiteSpace($resolved) -or [IO.Path]::GetPathRoot($resolved) -eq $resolved) {
        throw "Unsafe directory path: $resolved"
    }
    return $resolved
}

# 原因：CI 与本机共用同一打包入口，工具既可由 workflow 显式传入也可从 PATH 解析。
function Resolve-VpkExecutable {
    param([string]$ExplicitPath)

    if (-not [string]::IsNullOrWhiteSpace($ExplicitPath)) {
        $candidate = [IO.Path]::GetFullPath($ExplicitPath)
        if (-not (Test-Path -LiteralPath $candidate -PathType Leaf)) {
            throw "vpk executable is missing: $candidate"
        }
        return $candidate
    }
    $command = Get-Command vpk -ErrorAction SilentlyContinue
    if ($null -eq $command) {
        throw 'vpk 1.2.0 is required; install it with dotnet tool install vpk --version 1.2.0'
    }
    return $command.Source
}

if ($Version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?$') {
    throw "Version must be exact SemVer: $Version"
}

$repository = Resolve-SafeDirectory (Join-Path $PSScriptRoot '..')
$output = Resolve-SafeDirectory $OutputDirectory
$stage = Resolve-SafeDirectory (Join-Path $output 'windows-velopack-stage')
$inputDirectory = Join-Path $stage 'input'
$vpkOutput = Join-Path $stage 'vpk'
$innoPayload = Join-Path $output 'payload'
$vpk = Resolve-VpkExecutable $VpkPath

if (Test-Path -LiteralPath $stage) {
    Remove-Item -LiteralPath $stage -Recurse -Force
}
if (Test-Path -LiteralPath $innoPayload) {
    Remove-Item -LiteralPath $innoPayload -Recurse -Force
}
New-Item -ItemType Directory -Path $inputDirectory, $vpkOutput, $innoPayload -Force | Out-Null

Copy-Item -LiteralPath (Join-Path $repository 'target\release\gmark.exe') -Destination $inputDirectory
Copy-Item -LiteralPath (Join-Path $repository 'README.md') -Destination $inputDirectory
Copy-Item -LiteralPath (Join-Path $repository 'LICENSE') -Destination $inputDirectory

& $vpk pack `
    --packId GMark `
    --packVersion $Version `
    --packDir $inputDirectory `
    --mainExe gmark.exe `
    --packTitle GMark `
    --packAuthors kongweiguang `
    --runtime win-x64 `
    --channel win-x64 `
    --outputDir $vpkOutput `
    --icon (Join-Path $repository 'assets\icon\gmark.ico') `
    --delta None `
    --shortcuts StartMenuRoot `
    --yes true `
    --skip-updates true
if ($LASTEXITCODE -ne 0) {
    throw "vpk pack failed with exit $LASTEXITCODE"
}

$portable = Join-Path $vpkOutput 'GMark-win-x64-Portable.zip'
$package = Join-Path $vpkOutput "GMark-$Version-win-x64-full.nupkg"
foreach ($required in @($portable, $package)) {
    if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
        throw "Velopack output is missing: $required"
    }
}

Expand-Archive -LiteralPath $portable -DestinationPath $innoPayload
$portableMarker = Join-Path $innoPayload '.portable'
if (Test-Path -LiteralPath $portableMarker -PathType Leaf) {
    Remove-Item -LiteralPath $portableMarker -Force
}
# The published 0.2.1 helper requires root gmark.exe --version to be one exact line. Velopack's
# generated stub prefixes updater logs, so the Inno bridge uses the app's tiny root-dispatch mode.
Copy-Item -LiteralPath (Join-Path $innoPayload 'current\gmark.exe') `
    -Destination (Join-Path $innoPayload 'GMark.exe') -Force
# Inno reads these two files at compile time; they are not part of Velopack's installed root.
Copy-Item -LiteralPath (Join-Path $repository 'assets\icon\gmark.ico') -Destination $innoPayload
Copy-Item -LiteralPath (Join-Path $repository 'LICENSE') -Destination $innoPayload
$manifest = Join-Path $innoPayload 'current\sq.version'
$updateExecutable = Join-Path $innoPayload 'Update.exe'
$entryStub = Join-Path $innoPayload 'GMark.exe'
foreach ($required in @($manifest, $updateExecutable, $entryStub)) {
    if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
        throw "Inno bridge payload is missing: $required"
    }
}

$releasePackage = Join-Path $output "gmark-v$Version-windows-x86_64-full.nupkg"
Copy-Item -LiteralPath $package -Destination $releasePackage -Force
Write-Host "Windows Velopack bridge payload and update package created for $Version"
