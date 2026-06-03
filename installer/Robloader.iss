; Installeur Windows pour Robloader (Inno Setup).
; Compiler : iscc installer\Robloader.iss  (lance automatiquement par build_windows.ps1 si ISCC present).
; Prerequis : avoir build l'app -> dist\Robloader\ (via build_windows.ps1).

[Setup]
AppName=Robloader
AppVersion=1.0.0
AppPublisher=Humanoid
DefaultDirName={autopf}\Robloader
DefaultGroupName=Robloader
DisableProgramGroupPage=yes
OutputDir=Output
OutputBaseFilename=Robloader-Setup
SetupIconFile=..\logo.ico
Compression=lzma2
SolidCompression=yes
ArchitecturesInstallIn64BitMode=x64
WizardStyle=modern
UninstallDisplayIcon={app}\Robloader.exe

[Languages]
Name: "french"; MessagesFile: "compiler:Languages\French.isl"

[Tasks]
Name: "desktopicon"; Description: "Créer un raccourci sur le Bureau"; GroupDescription: "Raccourcis :"

[Files]
Source: "..\dist\Robloader\*"; DestDir: "{app}"; Flags: recursesubdirs createallsubdirs ignoreversion

[Icons]
Name: "{group}\Robloader"; Filename: "{app}\Robloader.exe"
Name: "{group}\Désinstaller Robloader"; Filename: "{uninstallexe}"
Name: "{autodesktop}\Robloader"; Filename: "{app}\Robloader.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\Robloader.exe"; Description: "Lancer Robloader"; Flags: nowait postinstall skipifsilent
