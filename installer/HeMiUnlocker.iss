; HeMiUnlocker installer (Inno Setup 6), built by release.ps1:  ISCC /DAppVersion=1.1.0 /DAppExe=<HeMiUnlocker.exe> /DOutputDir=<folder>
; For the current user, without administrator rights. Uninstalling removes the File Explorer item of the app as well.

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
DefaultDirName={localappdata}\Programs\HeMiUnlocker
DisableDirPage=yes
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
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
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "cs"; MessagesFile: "compiler:Languages\Czech.isl"

[Files]
Source: "{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{userprograms}\HeMiUnlocker"; Filename: "{app}\HeMiUnlocker.exe"

[Run]
Filename: "{app}\HeMiUnlocker.exe"; Description: "{cm:LaunchProgram,HeMiUnlocker}"; Flags: nowait postinstall skipifsilent

[Registry]
; the File Explorer item the app writes when it is switched on: removed with the app
Root: HKCU; Subkey: "Software\Classes\*\shell\HeMiUnlocker"; Flags: uninsdeletekey dontcreatekey
Root: HKCU; Subkey: "Software\Classes\Directory\shell\HeMiUnlocker"; Flags: uninsdeletekey dontcreatekey
