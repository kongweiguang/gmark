# @author kongweiguang

[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Assert-True {
    param([bool] $Condition, [string] $Message)
    if (-not $Condition) { throw "Assertion failed: $Message" }
}

function Assert-Equal {
    param($Expected, $Actual, [string] $Message)
    if ($Expected -ne $Actual) {
        throw "Assertion failed: $Message. Expected '$Expected', got '$Actual'."
    }
}

function Test-GmarkPackageTransaction {
    $sandbox = Join-Path $env:TEMP ("gmark installer test {0}" -f [Guid]::NewGuid().ToString('N'))
    $package = Join-Path $sandbox 'package'
    $install = Join-Path $sandbox 'gmark'
    try {
        New-Item -ItemType Directory -Path $package -Force | Out-Null
        Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'install.ps1') -Destination $package
        Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'uninstall.ps1') -Destination $package
        Set-Content -LiteralPath (Join-Path $package 'gmark.exe') -Value 'binary-v1' -NoNewline
        Set-Content -LiteralPath (Join-Path $package 'version.txt') -Value '1.0.0' -NoNewline
        $fakeModule = @'
function Register-GmarkFileAssociations {
    [CmdletBinding()]
    param($ExecutablePath, $InstallLocation, $Version, $UninstallCommand)
    if ($env:GMARK_INSTALL_TEST_FAIL -eq '1' -and $Version -eq '2.0.0') {
        throw 'injected registration failure'
    }
}
function Unregister-GmarkFileAssociations {
    [CmdletBinding()]
    param($ExecutablePath)
}
Export-ModuleMember -Function Register-GmarkFileAssociations, Unregister-GmarkFileAssociations
'@
        Set-Content -LiteralPath (Join-Path $package 'gmark.Install.psm1') -Value $fakeModule

        & (Join-Path $package 'install.ps1') -InstallDir $install -AllowUnsigned `
            -NoStartMenuShortcut
        Assert-Equal 'binary-v1' (Get-Content -LiteralPath (Join-Path $install 'gmark.exe') -Raw) `
            'first install must publish the staged payload'
        $marker = Get-Content -LiteralPath (Join-Path $install '.gmark-install') -Raw |
            ConvertFrom-Json
        Assert-Equal '1.0.0' $marker.version 'first install must write its version marker'

        Set-Content -LiteralPath (Join-Path $package 'gmark.exe') -Value 'binary-v2' -NoNewline
        Set-Content -LiteralPath (Join-Path $package 'version.txt') -Value '2.0.0' -NoNewline
        $env:GMARK_INSTALL_TEST_FAIL = '1'
        $failedAsExpected = $false
        try {
            & (Join-Path $package 'install.ps1') -InstallDir $install -AllowUnsigned `
                -NoStartMenuShortcut
        }
        catch {
            $failedAsExpected = $true
        }
        Assert-True $failedAsExpected 'the injected upgrade failure must reach the caller'
        Assert-Equal 'binary-v1' (Get-Content -LiteralPath (Join-Path $install 'gmark.exe') -Raw) `
            'a failed upgrade must restore the previous payload'
        $restoredMarker = Get-Content -LiteralPath (Join-Path $install '.gmark-install') -Raw |
            ConvertFrom-Json
        Assert-Equal '1.0.0' $restoredMarker.version `
            'a failed upgrade must restore the previous marker'
        Assert-Equal 0 (@(Get-ChildItem -LiteralPath $sandbox -Directory | Where-Object {
            $_.Name -like 'gmark.stage-*' -or $_.Name -like 'gmark.backup-*'
        }).Count) 'a completed rollback must not leave stage or backup directories'

        $refusedAsExpected = $false
        try { & (Join-Path $package 'uninstall.ps1') }
        catch { $refusedAsExpected = $true }
        Assert-True $refusedAsExpected 'uninstall must reject a directory without its marker'
        Assert-True (Test-Path -LiteralPath $package) 'marker rejection must not delete package files'

        $uninstallScript = Join-Path $install 'uninstall.ps1'
        $uninstallArguments = '-NoProfile -ExecutionPolicy Bypass -File "{0}"' -f $uninstallScript
        $uninstallProcess = Start-Process -FilePath 'powershell.exe' -ArgumentList $uninstallArguments `
            -Wait -PassThru -WindowStyle Hidden
        Assert-Equal 0 $uninstallProcess.ExitCode 'uninstall child process must exit successfully'
        $deadline = [DateTime]::UtcNow.AddSeconds(10)
        while ((Test-Path -LiteralPath $install) -and [DateTime]::UtcNow -lt $deadline) {
            Start-Sleep -Milliseconds 100
        }
        Assert-True (-not (Test-Path -LiteralPath $install)) `
            'uninstall cleanup must remove an install path containing spaces'
    }
    finally {
        Remove-Item Env:\GMARK_INSTALL_TEST_FAIL -ErrorAction SilentlyContinue
        Remove-Module gmark.Install -Force -ErrorAction SilentlyContinue
        Remove-Item -LiteralPath $sandbox -Recurse -Force -ErrorAction SilentlyContinue
    }
}

$testId = [Guid]::NewGuid().ToString('N')
$root = "HKCU:\Software\gmarkInstallerTests\$testId"
$classes = Join-Path $root 'Classes'
$registeredApplications = Join-Path $root 'RegisteredApplications'
$appPaths = Join-Path $root 'AppPaths'
$uninstall = Join-Path $root 'Uninstall'
$capabilitiesRegistration = "Software\gmarkInstallerTests\$testId\Classes\Applications\gmark.exe\Capabilities"
$installLocation = 'C:\Program Files With Spaces\gmark'
$executable = Join-Path $installLocation 'gmark.exe'
$otherExecutable = 'C:\Another gmark\gmark.exe'
$module = Join-Path $PSScriptRoot 'gmark.Install.psm1'

try {
    Import-Module $module -Force
    New-Item -Path (Join-Path $classes '.md') -Force | Out-Null
    New-ItemProperty -LiteralPath (Join-Path $classes '.md') -Name 'sentinel' `
        -PropertyType String -Value 'preserve-me' | Out-Null

    $arguments = @{
        ExecutablePath = $executable
        InstallLocation = $installLocation
        Version = '1.2.3'
        UninstallCommand = 'powershell.exe -File "C:\Program Files With Spaces\gmark\uninstall.ps1"'
        ClassesRoot = $classes
        RegisteredApplicationsRoot = $registeredApplications
        AppPathsRoot = $appPaths
        UninstallRoot = $uninstall
        CapabilitiesRegistrationPath = $capabilitiesRegistration
    }
    Register-GmarkFileAssociations @arguments

    $mdKey = Join-Path $classes '.md'
    Assert-Equal $null (Get-Item -LiteralPath $mdKey).GetValue('') `
        'registration must not claim the .md default association'
    Assert-True (-not (Test-Path -LiteralPath (Join-Path $mdKey 'UserChoice'))) `
        'registration must not create UserChoice'
    Assert-True ((Get-Item -LiteralPath (Join-Path $mdKey 'OpenWithProgids')).GetValueNames() `
        -contains 'gmark.Markdown') 'the .md OpenWithProgids value must exist'

    $command = [string](Get-Item -LiteralPath `
        (Join-Path $classes 'gmark.Markdown\shell\open\command')).GetValue('')
    Assert-Equal ('"{0}" "%1"' -f $executable) $command `
        'open command must quote an executable path containing spaces'
    Assert-Equal 'gmark.Markdown' ([string](Get-ItemPropertyValue -LiteralPath `
        (Join-Path $classes 'Applications\gmark.exe\Capabilities\FileAssociations') `
        -Name '.markdown')) 'capabilities must advertise .markdown'
    foreach ($extension in @('.txt', '.log', '.json', '.jsonl', '.ndjson', '.csv', '.tsv')) {
        $extensionKey = Join-Path $classes $extension
        Assert-Equal $null (Get-Item -LiteralPath $extensionKey).GetValue('') `
            "registration must not claim the $extension default association"
        Assert-True ((Get-Item -LiteralPath (Join-Path $extensionKey 'OpenWithProgids')).GetValueNames() `
            -contains 'gmark.Markdown') "the $extension OpenWithProgids value must exist"
        Assert-Equal 'gmark.Markdown' ([string](Get-ItemPropertyValue -LiteralPath `
            (Join-Path $classes 'Applications\gmark.exe\Capabilities\FileAssociations') `
            -Name $extension)) "capabilities must advertise $extension"
    }
    Assert-Equal $capabilitiesRegistration ([string](Get-ItemPropertyValue -LiteralPath `
        $registeredApplications -Name 'gmark')) 'RegisteredApplications must reference capabilities'
    Assert-Equal $executable ([string](Get-Item -LiteralPath `
        (Join-Path $appPaths 'gmark.exe')).GetValue('')) 'App Paths must reference the installed exe'
    Assert-Equal $installLocation ([string](Get-ItemPropertyValue -LiteralPath `
        (Join-Path $uninstall 'gmark') -Name 'InstallLocation')) `
        'uninstall metadata must reference the install location'

    Unregister-GmarkFileAssociations -ExecutablePath $executable -ClassesRoot $classes `
        -RegisteredApplicationsRoot $registeredApplications -AppPathsRoot $appPaths `
        -UninstallRoot $uninstall -CapabilitiesRegistrationPath $capabilitiesRegistration

    Assert-True (Test-Path -LiteralPath $mdKey) 'the extension root key must be preserved'
    Assert-Equal 'preserve-me' ([string](Get-ItemPropertyValue -LiteralPath $mdKey `
        -Name 'sentinel')) 'unrelated extension values must be preserved'
    Assert-True (-not ((Get-Item -LiteralPath (Join-Path $mdKey 'OpenWithProgids')).GetValueNames() `
        -contains 'gmark.Markdown')) 'only the gmark OpenWithProgids value must be removed'
    foreach ($extension in @('.txt', '.log', '.json', '.jsonl', '.ndjson', '.csv', '.tsv')) {
        $openWith = Join-Path (Join-Path $classes $extension) 'OpenWithProgids'
        Assert-True (-not ((Get-Item -LiteralPath $openWith).GetValueNames() `
            -contains 'gmark.Markdown')) "the gmark $extension OpenWithProgids value must be removed"
    }
    Assert-True (-not (Test-Path -LiteralPath (Join-Path $classes 'gmark.Markdown'))) `
        'the owned ProgID key must be removed'

    Register-GmarkFileAssociations @arguments
    $replacement = @{} + $arguments
    $replacement.ExecutablePath = $otherExecutable
    $replacement.InstallLocation = [IO.Path]::GetDirectoryName($otherExecutable)
    Register-GmarkFileAssociations @replacement
    Unregister-GmarkFileAssociations -ExecutablePath $executable -ClassesRoot $classes `
        -RegisteredApplicationsRoot $registeredApplications -AppPathsRoot $appPaths `
        -UninstallRoot $uninstall -CapabilitiesRegistrationPath $capabilitiesRegistration
    Assert-Equal ('"{0}" "%1"' -f $otherExecutable) ([string](Get-Item -LiteralPath `
        (Join-Path $classes 'Applications\gmark.exe\shell\open\command')).GetValue('')) `
        'a stale uninstaller must not remove a newer registration'

    Test-GmarkPackageTransaction
    Write-Host 'Windows installer registry and transaction tests passed.'
}
finally {
    Remove-Module gmark.Install -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
}
