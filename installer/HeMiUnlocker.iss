; HeMiUnlocker installer (Inno Setup 6), built by release.ps1:  ISCC /DAppVersion=1.1.0 /DAppExe=<HeMiUnlocker.exe> /DOutputDir=<folder>
; For the current user (no administrator) or for all users - the installer asks. Uninstalling removes the File Explorer item of the app as well.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef AppExe
  #define AppExe "..\build\release\HeMiUnlocker.exe"
#endif
#ifndef OutputDir
  #define OutputDir "..\build\dist"
#endif

[Setup]
AppId=HeMiUnlocker
AppName=HeMiUnlocker
AppVersion={#AppVersion}
AppVerName=HeMiUnlocker Beta {#AppVersion}
AppPublisher=Richard Hemala
AppPublisherURL=https://github.com/Hemal1k/HeMiUnlocker
AppSupportURL=https://github.com/Hemal1k/HeMiUnlocker/issues
AppUpdatesURL=https://github.com/Hemal1k/HeMiUnlocker/releases
DefaultDirName={autopf}\HeMiUnlocker
DisableDirPage=yes
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
; the first page asks: for me only (default, no administrator) or for all users (Program Files, asks for administrator)
PrivilegesRequiredOverridesAllowed=dialog commandline
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir={#OutputDir}
OutputBaseFilename=HeMiUnlocker-Setup
SetupIconFile=..\assets\HeMiUnlocker.ico
UninstallDisplayIcon={app}\HeMiUnlocker.exe
UninstallDisplayName=HeMiUnlocker
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
CloseApplications=force
LicenseFile=..\LICENSE

[Languages]
; English only
Name: "en"; MessagesFile: "compiler:Default.isl"

[Files]
Source: "{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion

[Tasks]
; offered on its own page, off by default
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Icons]
Name: "{autoprograms}\HeMiUnlocker"; Filename: "{app}\HeMiUnlocker.exe"
Name: "{autodesktop}\HeMiUnlocker"; Filename: "{app}\HeMiUnlocker.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\HeMiUnlocker.exe"; Description: "{cm:LaunchProgram,HeMiUnlocker}"; Flags: nowait postinstall skipifsilent runasoriginaluser

[Registry]
; the File Explorer item the app writes when it is switched on: removed with the app
Root: HKCU; Subkey: "Software\Classes\*\shell\HeMiUnlocker"; Flags: uninsdeletekey dontcreatekey
Root: HKCU; Subkey: "Software\Classes\Directory\shell\HeMiUnlocker"; Flags: uninsdeletekey dontcreatekey
Root: HKCU; Subkey: "Software\Classes\AppUserModelId\HeMi.HeMiUnlocker"; Flags: uninsdeletekey dontcreatekey

[UninstallDelete]
; the icon of the notifications of "Unlock"
Type: filesandordirs; Name: "{localappdata}\HeMiUnlocker"
