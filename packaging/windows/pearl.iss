; The Windows installer, for one user with no administrator rights. Built by the release workflow:
; iscc /DVersion=<version> /DFrom=<folder> /DOut=<folder> packaging\windows\pearl.iss
[Setup]
AppName=Pearl
AppVersion={#Version}
AppPublisher=Morrison Kühlsen
AppPublisherURL=https://github.com/morrisonkuhlsen/mkpearl
DefaultDirName={autopf}\Pearl
DefaultGroupName=Pearl
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir={#Out}
OutputBaseFilename=pearl-{#Version}-windows-x64-setup
Compression=lzma2
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
LicenseFile=..\..\LICENSE-MIT
UninstallDisplayIcon={app}\pearl.exe
SetupIconFile=pearl.ico

[Languages]
Name: "brazilianportuguese"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#From}\pearl.exe"; DestDir: "{app}"
Source: "..\..\LICENSE-MIT"; DestDir: "{app}"
Source: "..\..\LICENSE-APACHE"; DestDir: "{app}"
Source: "..\..\NOTICE"; DestDir: "{app}"; DestName: "NOTICE.txt"
Source: "..\..\ATTRIBUTION.md"; DestDir: "{app}"
Source: "..\..\THIRD-PARTY-LICENSES.txt"; DestDir: "{app}"

[Icons]
Name: "{group}\Pearl"; Filename: "{app}\pearl.exe"
Name: "{autodesktop}\Pearl"; Filename: "{app}\pearl.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\pearl.exe"; Description: "{cm:LaunchProgram,Pearl}"; Flags: nowait postinstall skipifsilent
