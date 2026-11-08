; Inno Setup script for the ShapeCraft installer; `cargo xtask bundle-windows` passes Version, Stage, OutDir and OutName.
; Installs for the current user (no administrator prompt), adds a Start menu entry and an uninstaller.

#ifndef Version
  #define Version "0.0.0"
#endif

[Setup]
AppId={{6F1C2A4E-3B7D-4C59-9E2A-5D8B1F0C7A31}
AppName=ShapeCraft
AppVersion={#Version}
AppVerName=ShapeCraft {#Version} alpha
AppPublisher=ShapeCraft contributors
AppPublisherURL=https://jackharvest.com/shapecraft/
AppSupportURL=https://github.com/jackharvest/shapecraft/issues
DefaultDirName={localappdata}\Programs\ShapeCraft
DefaultGroupName=ShapeCraft
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#OutDir}
OutputBaseFilename={#OutName}
SetupIconFile={#Stage}\shapecraft.ico
UninstallDisplayIcon={app}\shapecraft.exe
LicenseFile={#Stage}\LICENSE-MIT
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#Stage}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs

[Icons]
Name: "{group}\ShapeCraft"; Filename: "{app}\shapecraft.exe"
Name: "{group}\Uninstall ShapeCraft"; Filename: "{uninstallexe}"
Name: "{autodesktop}\ShapeCraft"; Filename: "{app}\shapecraft.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\shapecraft.exe"; Description: "{cm:LaunchProgram,ShapeCraft}"; Flags: nowait postinstall skipifsilent
