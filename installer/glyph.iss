
#define AppName "Glyph"
#define AppVersion "0.3.0"
#define AppExe "glyph.exe"

[Setup]
AppId={{7E5C1B2A-4C44-4A32-9C1B-3F8D2F5B6A10}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=Glyph contributors
DefaultDirName={localappdata}\Programs\{#AppName}
DefaultGroupName={#AppName}
PrivilegesRequired=lowest
OutputDir=..\dist
OutputBaseFilename=GlyphSetup
SetupIconFile=..\assets\glyph.ico
UninstallDisplayIcon={app}\{#AppExe}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ChangesAssociations=yes

[Files]
Source: "..\target\release\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{userdesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked

[Registry]
Root: HKCU; Subkey: "Software\Classes\Glyph.Markdown"; ValueType: string; ValueData: "Markdown Document"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\Glyph.Markdown\DefaultIcon"; ValueType: string; ValueData: """{app}\{#AppExe}"",0"
Root: HKCU; Subkey: "Software\Classes\Glyph.Markdown\shell\open\command"; ValueType: string; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKCU; Subkey: "Software\Classes\.md"; ValueType: string; ValueData: "Glyph.Markdown"; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Classes\.markdown"; ValueType: string; ValueData: "Glyph.Markdown"; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Classes\.mdown"; ValueType: string; ValueData: "Glyph.Markdown"; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Classes\.mkd"; ValueType: string; ValueData: "Glyph.Markdown"; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Classes\.mmd"; ValueType: string; ValueData: "Glyph.Markdown"; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Classes\.mermaid"; ValueType: string; ValueData: "Glyph.Markdown"; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Classes\.md\OpenWithProgids"; ValueType: string; ValueName: "Glyph.Markdown"; ValueData: ""; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Classes\.txt\OpenWithProgids"; ValueType: string; ValueName: "Glyph.Markdown"; ValueData: ""; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Classes\Applications\{#AppExe}"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "{#AppName}"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\Applications\{#AppExe}\shell\open\command"; ValueType: string; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKCU; Subkey: "Software\Glyph\Capabilities"; ValueType: string; ValueName: "ApplicationName"; ValueData: "{#AppName}"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Glyph\Capabilities"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "A native Markdown editor for notes, code and diagrams."
Root: HKCU; Subkey: "Software\Glyph\Capabilities\FileAssociations"; ValueType: string; ValueName: ".md"; ValueData: "Glyph.Markdown"
Root: HKCU; Subkey: "Software\Glyph\Capabilities\FileAssociations"; ValueType: string; ValueName: ".markdown"; ValueData: "Glyph.Markdown"
Root: HKCU; Subkey: "Software\Glyph\Capabilities\FileAssociations"; ValueType: string; ValueName: ".txt"; ValueData: "Glyph.Markdown"
Root: HKCU; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "Glyph"; ValueData: "Software\Glyph\Capabilities"; Flags: uninsdeletevalue

[Run]
Filename: "{app}\{#AppExe}"; Description: "Launch Glyph"; Flags: nowait postinstall skipifsilent
