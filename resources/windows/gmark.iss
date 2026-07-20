; @author kongweiguang
; gmark per-user Windows x64 installer. The package intentionally remains unsigned
; until a trusted Authenticode certificate is available.

#ifndef MyAppVersion
  #error MyAppVersion must be provided by ISCC
#endif
#ifndef SourceDir
  #error SourceDir must be provided by ISCC
#endif
#ifndef OutputDir
  #error OutputDir must be provided by ISCC
#endif

#define MyAppName "gmark"
#define MyAppPublisher "kongweiguang"
#define MyAppExeName "gmark.exe"

[Setup]
AppId={{7E04F75C-109D-4C5E-9E7B-BDE8F91FD0E1}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL=https://github.com/kongweiguang/gmark
AppSupportURL=https://github.com/kongweiguang/gmark/issues
AppUpdatesURL=https://github.com/kongweiguang/gmark/releases
DefaultDirName={localappdata}\Programs\gmark
DefaultGroupName=gmark
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#OutputDir}
OutputBaseFilename=gmark-v{#MyAppVersion}-windows-x86_64-setup
SetupIconFile={#SourceDir}\gmark.ico
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
RestartApplications=no
UninstallDisplayIcon={app}\gmark.exe
VersionInfoVersion={#MyAppVersion}
VersionInfoCompany={#MyAppPublisher}
VersionInfoDescription=gmark Markdown Editor Setup
VersionInfoProductName={#MyAppName}
VersionInfoProductVersion={#MyAppVersion}
LicenseFile={#SourceDir}\LICENSE

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceDir}\gmark.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\PRIVACY.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\SECURITY.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\NOTICE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\gmark"; Filename: "{app}\gmark.exe"
Name: "{autodesktop}\gmark"; Filename: "{app}\gmark.exe"; Tasks: desktopicon

[Registry]
; Register as an Open With application without taking over the user's defaults.
Root: HKCU; Subkey: "Software\Classes\Applications\gmark.exe"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "gmark"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\Applications\gmark.exe\shell\open\command"; ValueType: string; ValueData: """{app}\gmark.exe"" ""%1"""
Root: HKCU; Subkey: "Software\Classes\Applications\gmark.exe\SupportedTypes"; ValueType: string; ValueName: ".md"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\gmark.exe\SupportedTypes"; ValueType: string; ValueName: ".markdown"; ValueData: ""

[Run]
Filename: "{app}\gmark.exe"; Description: "{cm:LaunchProgram,gmark}"; Flags: nowait postinstall skipifsilent
