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
#ifndef MyAppFileVersion
  #define MyAppFileVersion MyAppVersion
#endif
#ifndef MyAppId
  #define MyAppId "{{7E04F75C-109D-4C5E-9E7B-BDE8F91FD0E1}"
#endif

#define MyAppName "Gmark"
#define MyAppPublisher "kongweiguang"
#define MyAppExeName "gmark.exe"

[Setup]
AppId={#MyAppId}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL=https://github.com/kongweiguang/gmark
AppSupportURL=https://github.com/kongweiguang/gmark/issues
AppUpdatesURL=https://github.com/kongweiguang/gmark/releases
DefaultDirName={localappdata}\Programs\gmark
DefaultGroupName=Gmark
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
VersionInfoVersion={#MyAppFileVersion}
VersionInfoCompany={#MyAppPublisher}
VersionInfoDescription=Gmark Markdown Editor Setup
VersionInfoProductName={#MyAppName}
VersionInfoProductVersion={#MyAppFileVersion}
LicenseFile={#SourceDir}\LICENSE

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
; Velopack owns the canonical current/Update.exe layout; Inno remains only the
; compatibility bridge understood by updater-v2 clients already in the field.
Source: "{#SourceDir}\GMark.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\Update.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\current\*"; DestDir: "{app}\current"; Flags: ignoreversion recursesubdirs createallsubdirs

[InstallDelete]
; Retired helper binaries must not remain beside the small bridge entry after migration.
Type: files; Name: "{app}\gmark-update-helper.exe"
Type: files; Name: "{app}\gmark-update-agent.exe"

[Icons]
#ifndef SmokeBuild
Name: "{group}\Gmark"; Filename: "{app}\gmark.exe"
Name: "{autodesktop}\Gmark"; Filename: "{app}\gmark.exe"; Tasks: desktopicon
#endif

[Registry]
#ifndef SmokeBuild
; Register as an Open With application without taking over the user's defaults.
Root: HKCU; Subkey: "Software\Classes\Applications\gmark.exe"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "Gmark"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\Applications\gmark.exe\shell\open\command"; ValueType: string; ValueData: """{app}\gmark.exe"" ""%1"""
Root: HKCU; Subkey: "Software\Classes\Applications\gmark.exe\SupportedTypes"; ValueType: string; ValueName: ".md"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\gmark.exe\SupportedTypes"; ValueType: string; ValueName: ".markdown"; ValueData: ""
#endif

[Run]
Filename: "{app}\gmark.exe"; Description: "{cm:LaunchProgram,Gmark}"; Flags: nowait postinstall skipifsilent
