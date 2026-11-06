#define AppVersion "0.1.0"
[Setup]
AppId={{F57822FD-E913-47D1-BE84-668E2F8D5AC6}
AppName=PC-Mirror
AppVersion={#AppVersion}
AppPublisher=PC-Mirror contributors
DefaultDirName={localappdata}\Programs\PC-Mirror
DefaultGroupName=PC-Mirror
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.19041
OutputDir=..\dist
OutputBaseFilename=PC-Mirror-Setup-{#AppVersion}-x64
Compression=lzma2/normal
SolidCompression=yes
WizardStyle=modern
LicenseFile=..\LICENSE
UninstallDisplayIcon={app}\bin\PC-Mirror.exe
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "german"; MessagesFile: "compiler:Languages\German.isl"

[Tasks]
Name: "desktopicon"; Description: "Desktop-Verknüpfung erstellen"; Flags: unchecked

[Files]
Source: "..\dist\PC-Mirror\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\PC-Mirror"; Filename: "{app}\bin\PC-Mirror.exe"
Name: "{autodesktop}\PC-Mirror"; Filename: "{app}\bin\PC-Mirror.exe"; Tasks: desktopicon
Name: "{group}\PC-Mirror deinstallieren"; Filename: "{uninstallexe}"

[Run]
Filename: "{app}\bin\PC-Mirror.exe"; Description: "PC-Mirror starten"; Flags: nowait postinstall skipifsilent

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueName: "PC-Mirror"; Flags: uninsdeletevalue

; Firewall changes need elevation; the app exposes a focused UAC helper. The
; installer itself runs as the current user. Personal screenshots/settings are
; never recursively deleted by the uninstaller.
