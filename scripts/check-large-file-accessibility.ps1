# @author kongweiguang

[CmdletBinding()]
param(
    [string] $AppPath = (Join-Path $PSScriptRoot '..\target\release\gmark.exe'),
    [Parameter(Mandatory = $true)]
    [string] $FixturePath,
    [Parameter(Mandatory = $true)]
    [string] $OutputDirectory,
    [string] $ExpectedErrorContains
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$app = [IO.Path]::GetFullPath($AppPath)
$fixture = [IO.Path]::GetFullPath($FixturePath)
$output = [IO.Path]::GetFullPath($OutputDirectory)
if (-not (Test-Path -LiteralPath $app -PathType Leaf)) {
    throw "Application not found: $app"
}
if (-not (Test-Path -LiteralPath $fixture -PathType Leaf)) {
    throw "Fixture not found: $fixture"
}
if (Test-Path -LiteralPath $output) {
    throw "Refusing to reuse accessibility output: $output"
}
[IO.Directory]::CreateDirectory($output) | Out-Null

function Get-Sha256Hex {
    param([string] $Path)
    $stream = [IO.File]::OpenRead($Path)
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        $hash = $sha.ComputeHash($stream)
        return (($hash | ForEach-Object { $_.ToString('x2') }) -join '')
    }
    finally {
        $sha.Dispose()
        $stream.Dispose()
    }
}

$ready = Join-Path $output 'ready.json'
$startInfo = [Diagnostics.ProcessStartInfo]::new()
$startInfo.FileName = $app
$startInfo.Arguments = '"' + $fixture + '"'
$startInfo.UseShellExecute = $false
$startInfo.EnvironmentVariables['GMARK_UI_CHECK_CONFIG_ROOT'] = Join-Path $output 'config'
$startInfo.EnvironmentVariables['GMARK_SOAK_READY_PATH'] = $ready
$startInfo.EnvironmentVariables['GMARK_SOAK_MODE'] = 'accessibility-check'
$process = [Diagnostics.Process]::Start($startInfo)

try {
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    while (-not (Test-Path -LiteralPath $ready -PathType Leaf)) {
        if ($process.HasExited) {
            throw "Application exited before readiness with code $($process.ExitCode)"
        }
        if ([DateTime]::UtcNow -gt $deadline) {
            throw 'Application did not publish readiness within 30 seconds'
        }
        Start-Sleep -Milliseconds 100
    }

    Add-Type -AssemblyName UIAutomationClient
    Add-Type -AssemblyName UIAutomationTypes
    $process.Refresh()
    if ($process.MainWindowHandle -eq 0) {
        throw 'Application has no main HWND'
    }
    $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)

    function Get-Nodes {
        param([Windows.Automation.AutomationElement] $Root)
        $all = $Root.FindAll(
            [Windows.Automation.TreeScope]::Descendants,
            [Windows.Automation.Condition]::TrueCondition
        )
        $nodes = @()
        for ($index = 0; $index -lt $all.Count; $index++) {
            $element = $all.Item($index)
            $nodes += [pscustomobject]@{
                name = $element.Current.Name
                control_type = $element.Current.ControlType.ProgrammaticName
                enabled = $element.Current.IsEnabled
                offscreen = $element.Current.IsOffscreen
                help_text = $element.Current.HelpText
            }
        }
        return $nodes
    }

    function Invoke-NamedButton {
        param(
            [Windows.Automation.AutomationElement] $Root,
            [string] $Name
        )
        $condition = [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::NameProperty,
            $Name
        )
        $element = $Root.FindFirst([Windows.Automation.TreeScope]::Descendants, $condition)
        if ($null -eq $element) {
            throw "Accessibility button missing: $Name"
        }
        $pattern = $element.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern)
        ([Windows.Automation.InvokePattern] $pattern).Invoke()
    }

    $before = Get-Nodes -Root $root
    $errorExposed = $false
    $errorValue = $null
    $errorActionInvoked = $false
    if (-not [string]::IsNullOrWhiteSpace($ExpectedErrorContains)) {
        $errorCondition = [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::NameProperty,
            'Document error'
        )
        $errorDeadline = [DateTime]::UtcNow.AddSeconds(30)
        while ([DateTime]::UtcNow -lt $errorDeadline) {
            $before = Get-Nodes -Root $root
            $errorElement = $root.FindFirst(
                [Windows.Automation.TreeScope]::Descendants,
                $errorCondition
            )
            if ($null -ne $errorElement) {
                try {
                    $valuePatternObject = $errorElement.GetCurrentPattern(
                        [Windows.Automation.ValuePattern]::Pattern
                    )
                    $errorValue = ([Windows.Automation.ValuePattern] $valuePatternObject).Current.Value
                }
                catch {
                    $errorValue = $errorElement.Current.HelpText
                }
                if ([string]::IsNullOrWhiteSpace($errorValue)) {
                    $errorValue = $errorElement.Current.HelpText
                }
            }
            $errorExposed = -not [string]::IsNullOrWhiteSpace($errorValue) -and
                $errorValue.Contains($ExpectedErrorContains)
            if ($errorExposed) {
                break
            }
            Start-Sleep -Milliseconds 200
        }
        if (-not $errorExposed) {
            throw "Accessibility tree did not expose error containing: $ExpectedErrorContains"
        }
        Invoke-NamedButton -Root $root -Name 'Document error'
        Start-Sleep -Milliseconds 800
        $errorActionInvoked = $true
    }
    $required = @(
        [IO.Path]::GetFileName($fixture),
        'Source editor',
        'Source',
        'Document status',
        'Save',
        'Find',
        'Go to line',
        'Line 1'
    )
    foreach ($name in $required) {
        if (-not ($before.name -contains $name)) {
            throw "Accessibility node missing: $name"
        }
    }
    $lineCount = @($before | Where-Object { $_.name -like 'Line *' }).Count
    if ($lineCount -lt 1 -or $lineCount -gt 512) {
        throw "Accessibility viewport line count is outside 1..512: $lineCount"
    }
    $editorCondition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::NameProperty,
        'Source editor'
    )
    $editorElement = $root.FindFirst(
        [Windows.Automation.TreeScope]::Descendants,
        $editorCondition
    )
    $textPatternObject = $editorElement.GetCurrentPattern(
        [Windows.Automation.TextPattern]::Pattern
    )
    $textPattern = [Windows.Automation.TextPattern] $textPatternObject
    $documentPrefix = $textPattern.DocumentRange.GetText(256)
    if ([string]::IsNullOrWhiteSpace($documentPrefix)) {
        throw 'Source editor TextPattern returned no viewport text'
    }
    if ($textPattern.GetSelection().Count -lt 1) {
        throw 'Source editor TextPattern returned no caret or selection'
    }

    Invoke-NamedButton -Root $root -Name 'Find'
    Start-Sleep -Milliseconds 800
    $afterFind = Get-Nodes -Root $root
    if (-not ($afterFind.name -contains 'Find in document')) {
        throw 'Invoking Find did not expose the search input'
    }

    Invoke-NamedButton -Root $root -Name 'Go to line'
    Start-Sleep -Milliseconds 800
    $afterGoTo = Get-Nodes -Root $root
    if (-not ($afterGoTo.name -contains 'Go to line or byte')) {
        throw 'Invoking Go to line did not expose the navigation input'
    }

    $result = [pscustomobject]@{
        schema_version = 1
        success = $true
        platform = [Environment]::OSVersion.ToString()
        app_path = $app
        app_sha256 = Get-Sha256Hex -Path $app
        fixture = $fixture
        process_id = $process.Id
        initial_node_count = $before.Count
        viewport_line_nodes = $lineCount
        find_action_exposed_input = $true
        go_to_line_action_exposed_input = $true
        text_pattern_exposed_viewport = $true
        text_pattern_exposed_selection = $true
        expected_error_exposed = $errorExposed
        error_value = $errorValue
        error_action_invoked = $errorActionInvoked
        required_nodes = $required
        nodes = $before
    }
    [IO.File]::WriteAllText(
        (Join-Path $output 'uia.json'),
        ($result | ConvertTo-Json -Depth 6),
        [Text.UTF8Encoding]::new($false)
    )
    $result | Select-Object `
        success, process_id, initial_node_count, viewport_line_nodes, `
        find_action_exposed_input, go_to_line_action_exposed_input, `
        text_pattern_exposed_viewport, text_pattern_exposed_selection | ConvertTo-Json
}
finally {
    if (-not $process.HasExited) {
        $null = $process.CloseMainWindow()
        if (-not $process.WaitForExit(2000)) {
            $process.Kill()
            $process.WaitForExit()
        }
    }
}
