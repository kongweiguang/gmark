# @author kongweiguang

Set-StrictMode -Version Latest

$script:GmarkProgId = 'gmark.Markdown'
$script:GmarkApplication = 'gmark.exe'
$script:GmarkSupportedExtensions = @(
    '.md', '.markdown', '.txt', '.log', '.json', '.jsonl', '.ndjson', '.csv', '.tsv'
)

function Set-RegistryDefaultValue {
    param(
        [Parameter(Mandatory)] [string] $Path,
        [Parameter(Mandatory)] [string] $Value
    )
    if (-not (Test-Path -LiteralPath $Path)) {
        New-Item -Path $Path -Force | Out-Null
    }
    Set-ItemProperty -LiteralPath $Path -Name '(default)' -Value $Value
}

function Set-RegistryStringValue {
    param(
        [Parameter(Mandatory)] [string] $Path,
        [Parameter(Mandatory)] [string] $Name,
        [Parameter(Mandatory)] [AllowEmptyString()] [string] $Value
    )
    if (-not (Test-Path -LiteralPath $Path)) {
        New-Item -Path $Path -Force | Out-Null
    }
    New-ItemProperty -LiteralPath $Path -Name $Name -PropertyType String -Value $Value -Force |
        Out-Null
}

function Register-GmarkFileAssociations {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [string] $ExecutablePath,
        [Parameter(Mandatory)] [string] $InstallLocation,
        [Parameter(Mandatory)] [string] $Version,
        [Parameter(Mandatory)] [string] $UninstallCommand,
        [string] $ClassesRoot = 'HKCU:\Software\Classes',
        [string] $RegisteredApplicationsRoot = 'HKCU:\Software\RegisteredApplications',
        [string] $AppPathsRoot = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\App Paths',
        [string] $UninstallRoot = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall',
        [string] $CapabilitiesRegistrationPath =
            'Software\Classes\Applications\gmark.exe\Capabilities'
    )

    $executable = [IO.Path]::GetFullPath($ExecutablePath)
    $install = [IO.Path]::GetFullPath($InstallLocation)
    $openCommand = '"{0}" "%1"' -f $executable
    $applicationKey = Join-Path $ClassesRoot "Applications\$script:GmarkApplication"
    $capabilitiesKey = Join-Path $applicationKey 'Capabilities'
    $progIdKey = Join-Path $ClassesRoot $script:GmarkProgId

    Set-RegistryDefaultValue -Path $progIdKey -Value 'Text Document'
    Set-RegistryStringValue -Path $progIdKey -Name 'FriendlyTypeName' -Value 'gmark Text Document'
    Set-RegistryDefaultValue -Path (Join-Path $progIdKey 'DefaultIcon') -Value ('"{0}",0' -f $executable)
    Set-RegistryDefaultValue -Path (Join-Path $progIdKey 'shell\open\command') -Value $openCommand

    Set-RegistryStringValue -Path $applicationKey -Name 'FriendlyAppName' -Value 'gmark'
    Set-RegistryDefaultValue -Path (Join-Path $applicationKey 'shell\open\command') -Value $openCommand
    $supportedTypes = Join-Path $applicationKey 'SupportedTypes'
    foreach ($extension in $script:GmarkSupportedExtensions) {
        Set-RegistryStringValue -Path $supportedTypes -Name $extension -Value ''
        Set-RegistryStringValue -Path (Join-Path $ClassesRoot "$extension\OpenWithProgids") `
            -Name $script:GmarkProgId -Value ''
    }

    Set-RegistryStringValue -Path $capabilitiesKey -Name 'ApplicationName' -Value 'gmark'
    Set-RegistryStringValue -Path $capabilitiesKey -Name 'ApplicationDescription' `
        -Value 'Native Markdown and large text editor built with Rust and GPUI'
    $associations = Join-Path $capabilitiesKey 'FileAssociations'
    foreach ($extension in $script:GmarkSupportedExtensions) {
        Set-RegistryStringValue -Path $associations -Name $extension -Value $script:GmarkProgId
    }
    Set-RegistryStringValue -Path $RegisteredApplicationsRoot -Name 'gmark' `
        -Value $CapabilitiesRegistrationPath

    $appPathKey = Join-Path $AppPathsRoot $script:GmarkApplication
    Set-RegistryDefaultValue -Path $appPathKey -Value $executable
    Set-RegistryStringValue -Path $appPathKey -Name 'Path' -Value $install

    $uninstallKey = Join-Path $UninstallRoot 'gmark'
    Set-RegistryStringValue -Path $uninstallKey -Name 'DisplayName' -Value 'gmark'
    Set-RegistryStringValue -Path $uninstallKey -Name 'DisplayVersion' -Value $Version
    Set-RegistryStringValue -Path $uninstallKey -Name 'Publisher' -Value 'kongweiguang'
    Set-RegistryStringValue -Path $uninstallKey -Name 'InstallLocation' -Value $install
    Set-RegistryStringValue -Path $uninstallKey -Name 'DisplayIcon' -Value ('"{0}",0' -f $executable)
    Set-RegistryStringValue -Path $uninstallKey -Name 'UninstallString' -Value $UninstallCommand
    New-ItemProperty -LiteralPath $uninstallKey -Name 'NoModify' -PropertyType DWord -Value 1 -Force |
        Out-Null
    New-ItemProperty -LiteralPath $uninstallKey -Name 'NoRepair' -PropertyType DWord -Value 1 -Force |
        Out-Null
}

function Remove-RegistryValueIfEqual {
    param(
        [Parameter(Mandatory)] [string] $Path,
        [Parameter(Mandatory)] [string] $Name,
        [Parameter(Mandatory)] [AllowEmptyString()] [string] $Expected
    )
    if (-not (Test-Path -LiteralPath $Path)) { return }
    $property = Get-ItemProperty -LiteralPath $Path -Name $Name -ErrorAction SilentlyContinue
    if ($null -ne $property -and [string]$property.$Name -eq $Expected) {
        Remove-ItemProperty -LiteralPath $Path -Name $Name -Force
    }
}

function Unregister-GmarkFileAssociations {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [string] $ExecutablePath,
        [string] $ClassesRoot = 'HKCU:\Software\Classes',
        [string] $RegisteredApplicationsRoot = 'HKCU:\Software\RegisteredApplications',
        [string] $AppPathsRoot = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\App Paths',
        [string] $UninstallRoot = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall',
        [string] $CapabilitiesRegistrationPath =
            'Software\Classes\Applications\gmark.exe\Capabilities'
    )

    $executable = [IO.Path]::GetFullPath($ExecutablePath)
    $expectedInstallLocation = [IO.Path]::GetDirectoryName($executable)
    $expectedOpenCommand = '"{0}" "%1"' -f $executable
    $applicationKey = Join-Path $ClassesRoot "Applications\$script:GmarkApplication"
    $applicationCommandKey = Join-Path $applicationKey 'shell\open\command'

    # 已有其他 gmark 安装接管注册时，旧卸载器不得删除共享入口和值。
    if (Test-Path -LiteralPath $applicationCommandKey) {
        $registeredOpenCommand = Get-ItemPropertyValue -LiteralPath $applicationCommandKey -Name '(default)'
        if ($registeredOpenCommand -ne $expectedOpenCommand) {
            return
        }
    }

    foreach ($extension in $script:GmarkSupportedExtensions) {
        Remove-RegistryValueIfEqual -Path (Join-Path $ClassesRoot "$extension\OpenWithProgids") `
            -Name $script:GmarkProgId -Expected ''
    }
    Remove-RegistryValueIfEqual -Path $RegisteredApplicationsRoot -Name 'gmark' `
        -Expected $CapabilitiesRegistrationPath

    $appPathKey = Join-Path $AppPathsRoot $script:GmarkApplication
    if (Test-Path -LiteralPath $appPathKey) {
        $current = Get-ItemPropertyValue -LiteralPath $appPathKey -Name '(default)'
        if ($current -eq $executable) {
            Remove-Item -LiteralPath $appPathKey -Recurse -Force
        }
    }

    # 旧安装目录中的卸载器可能晚于升级运行，只删除仍指向当前 exe 的自有键。
    $ownedKeys = @()
    $ownedKeys += (Join-Path $ClassesRoot $script:GmarkProgId)
    $ownedKeys += $applicationKey
    foreach ($ownedKey in $ownedKeys) {
        $commandKey = Join-Path $ownedKey 'shell\open\command'
        if (Test-Path -LiteralPath $ownedKey) {
            if (-not (Test-Path -LiteralPath $commandKey)) {
                Remove-Item -LiteralPath $ownedKey -Recurse -Force
                continue
            }
            $currentCommand = Get-ItemPropertyValue -LiteralPath $commandKey -Name '(default)'
            if ($currentCommand -eq $expectedOpenCommand) {
                Remove-Item -LiteralPath $ownedKey -Recurse -Force
            }
        }
    }

    $uninstallKey = Join-Path $UninstallRoot 'gmark'
    if (Test-Path -LiteralPath $uninstallKey) {
        $registeredLocation = Get-ItemPropertyValue -LiteralPath $uninstallKey -Name 'InstallLocation' `
            -ErrorAction SilentlyContinue
        if ($registeredLocation -eq $expectedInstallLocation) {
            Remove-Item -LiteralPath $uninstallKey -Recurse -Force
        }
    }
}

Export-ModuleMember -Function Register-GmarkFileAssociations, Unregister-GmarkFileAssociations
