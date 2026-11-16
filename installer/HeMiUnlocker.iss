; HeMiUnlocker installer (Inno Setup 6), built by release.ps1:  ISCC /DAppVersion=1.1.0 /DAppExe=<HeMiUnlocker.exe> /DOutputDir=<folder>
; For the current user (no administrator) or for all users - the installer asks. Uninstalling removes the File Explorer item of the app as well.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef AppExe
  #define AppExe "..\build\release\HeMiUnlocker.exe"
#endif
#ifndef AppExeArm
  #define AppExeArm ""
#endif
#ifndef OutputDir
  #define OutputDir "..\build\dist"
#endif

[Setup]
AppId=HeMiUnlocker
AppName=HeMiUnlocker
AppVersion={#AppVersion}
AppVerName=HeMiUnlocker {#AppVersion}
AppPublisher=Richard Hemala
AppPublisherURL=https://github.com/Hemal1k/HeMiUnlocker
AppSupportURL=https://github.com/Hemal1k/HeMiUnlocker/issues
AppUpdatesURL=https://github.com/Hemal1k/HeMiUnlocker/releases
DefaultDirName={autopf}\HeMi\HeMiUnlocker
UsePreviousAppDir=no
DisableDirPage=yes
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
; the first page asks: for me only (default, no administrator) or for all users (Program Files, asks for administrator)
PrivilegesRequiredOverridesAllowed=dialog commandline
ArchitecturesAllowed=x64compatible or arm64
ArchitecturesInstallIn64BitMode=x64compatible or arm64
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
; x64, and the ARM64 program on Windows on ARM when the installer has it
#if AppExeArm != ""
Source: "{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion; Check: not IsArm64
Source: "{#AppExeArm}"; DestDir: "{app}"; DestName: "HeMiUnlocker.exe"; Flags: ignoreversion; Check: IsArm64
#else
Source: "{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion
#endif

[Tasks]
; offered on its own page, off by default
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Icons]
Name: "{autoprograms}\HeMiUnlocker"; Filename: "{app}\HeMiUnlocker.exe"
Name: "{autodesktop}\HeMiUnlocker"; Filename: "{app}\HeMiUnlocker.exe"; Tasks: desktopicon

[Run]
; the File Explorer items and the start with Windows point to this installation (also after a move from the old folder)
Filename: "{app}\HeMiUnlocker.exe"; Parameters: "--repair"; Flags: runasoriginaluser waituntilterminated runhidden
Filename: "{app}\HeMiUnlocker.exe"; Description: "{cm:LaunchProgram,HeMiUnlocker}"; Flags: nowait postinstall skipifsilent runasoriginaluser

[Registry]
; the File Explorer item the app writes when it is switched on: removed with the app
Root: HKCU; Subkey: "Software\Classes\*\shell\HeMiUnlocker"; Flags: uninsdeletekey dontcreatekey
Root: HKCU; Subkey: "Software\Classes\Directory\shell\HeMiUnlocker"; Flags: uninsdeletekey dontcreatekey
Root: HKCU; Subkey: "Software\Classes\AppUserModelId\HeMi.HeMiUnlocker"; Flags: uninsdeletekey dontcreatekey
; what Windows itself keeps about the app (its name in menus, "Open with", the settings of its notifications)
Root: HKCU; Subkey: "Software\Classes\Local Settings\Software\Microsoft\Windows\Shell\MuiCache"; ValueType: none; ValueName: "{app}\HeMiUnlocker.exe.FriendlyAppName"; Flags: uninsdeletevalue dontcreatekey
Root: HKCU; Subkey: "Software\Classes\Local Settings\Software\Microsoft\Windows\Shell\MuiCache"; ValueType: none; ValueName: "{app}\HeMiUnlocker.exe.ApplicationCompany"; Flags: uninsdeletevalue dontcreatekey
Root: HKCU; Subkey: "Software\Classes\Applications\HeMiUnlocker.exe"; Flags: uninsdeletekey dontcreatekey
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Notifications\Settings\HeMi.HeMiUnlocker"; Flags: uninsdeletekey dontcreatekey
Root: HKCU; Subkey: "Software\HeMi"; Flags: uninsdeletekeyifempty dontcreatekey

[UninstallDelete]
; the icon of the notifications of "Unlock"
Type: filesandordirs; Name: "{localappdata}\HeMi\HeMiUnlocker"
Type: filesandordirs; Name: "{localappdata}\HeMiUnlocker"
Type: dirifempty; Name: "{localappdata}\HeMi"

[Code]
// Since 2026-11 the HeMi apps go into one folder HeMi, as their settings do (%AppData%\HeMi\<app>): {autopf}\HeMi\HeMiUnlocker.
// An installation in the old folder {autopf}\HeMiUnlocker moves: the app running from there ends, the new version goes into the
// new folder ("--repair" points the File Explorer items and the start with Windows to it) and the old folder is deleted.
var
  OldFolder: String;

function FindOldFolder: String;
begin
  if RegQueryStringValue(HKA, 'Software\Microsoft\Windows\CurrentVersion\Uninstall\HeMiUnlocker_is1', 'InstallLocation', Result) then
  begin
    Result := RemoveBackslashUnlessRoot(Result);
    if (CompareText(Result, ExpandConstant('{app}')) = 0) or (CompareText(ExtractFileName(Result), 'HeMiUnlocker') <> 0)
      or not FileExists(Result + '\HeMiUnlocker.exe') then
      Result := '';
  end
  else
    Result := '';
end;

procedure StopOld;
var
  Code: Integer;
begin
  OldFolder := FindOldFolder;
  if OldFolder = '' then Exit;
  Exec(ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe'),
    '-NoProfile -NonInteractive -Command "Get-Process | Where-Object { $_.Path -like ''' + OldFolder + '\*'' } | Stop-Process -Force"',
    '', SW_HIDE, ewWaitUntilTerminated, Code);
end;

procedure DeleteOld;
begin
  if OldFolder <> '' then DelTree(OldFolder, True, True, True);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := '';
  StopOld;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then DeleteOld;
end;

// Uninstalling: the settings of the app (%AppData%\HeMi\HeMiUnlocker) are deleted or kept, as the user chooses (a quiet
// uninstall keeps them). %AppData%\HeMi goes too when no other HeMi app has anything there.
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Dir: String;
begin
  if CurUninstallStep <> usPostUninstall then Exit;
  Dir := ExpandConstant('{userappdata}\HeMi\HeMiUnlocker');
  if not DirExists(Dir) then Exit;
  if UninstallSilent then Exit;
  if MsgBox('Delete the settings of HeMiUnlocker too?' + #13#10#13#10 + 'Keep them to use them again when you install HeMiUnlocker later.', mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
  begin
    DelTree(Dir, True, True, True);
    RemoveDir(ExpandConstant('{userappdata}\HeMi'));
  end;
end;
