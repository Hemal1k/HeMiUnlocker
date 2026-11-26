; HeMiUnlocker installer (Inno Setup 6), built by release.ps1:  ISCC /DAppVersion=<version> /DAppExe=<HeMiUnlocker.exe> /DOutputDir=<folder>
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
; the look of Windows 11, light or dark as Windows, and as big as the first question of the installer (it was
; smaller and old-looking next to it, 2026-11-19)
WizardStyle=modern dynamic windows11
WizardSizePercent=120
CloseApplications=force
LicenseFile=..\LICENSE

[Languages]
; English, Czech on a Czech Windows
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "cs"; MessagesFile: "compiler:Languages\Czech.isl"

[CustomMessages]
en.TypeFull=Full – the window and the File Explorer menu
cs.TypeFull=Úplná – okno a nabídka Průzkumníka souborů
en.TypeMenu=Minimal – only the items in the File Explorer menu
cs.TypeMenu=Minimální – jen položky v nabídce Průzkumníka souborů
en.ComponentWindow=The window of HeMiUnlocker in Start
cs.ComponentWindow=Okno HeMiUnlockeru v nabídce Start
en.DeleteSettings=Delete the settings of HeMiUnlocker too?
cs.DeleteSettings=Smazat i nastavení HeMiUnlockeru?
en.KeepSettings=Keep them to use them again when you install HeMiUnlocker later.
cs.KeepSettings=Když je ponecháš, použijí se znovu, až HeMiUnlocker nainstaluješ později.

[Files]
; x64, and the ARM64 program on Windows on ARM when the installer has it
#if AppExeArm != ""
Source: "{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion; Check: not IsArm64
Source: "{#AppExeArm}"; DestDir: "{app}"; DestName: "HeMiUnlocker.exe"; Flags: ignoreversion; Check: IsArm64
#else
Source: "{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion
#endif

; full, or only the items in the File Explorer menu (the user, 2026-11-19): the same one exe (it does the work of the
; menu), but no window in Start, no desktop icon, nothing opened at the end – the menu items are switched on instead
[Types]
Name: "full"; Description: "{cm:TypeFull}"
Name: "menu"; Description: "{cm:TypeMenu}"

[Components]
Name: "window"; Description: "{cm:ComponentWindow}"; Types: full

[Tasks]
; offered on its own page, off by default
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked; Components: window

[Icons]
Name: "{autoprograms}\HeMiUnlocker"; Filename: "{app}\HeMiUnlocker.exe"; Components: window
Name: "{autodesktop}\HeMiUnlocker"; Filename: "{app}\HeMiUnlocker.exe"; Tasks: desktopicon

[Run]
; the File Explorer items point to this installation (also after a move from the old folder)
Filename: "{app}\HeMiUnlocker.exe"; Parameters: "--repair"; Flags: runasoriginaluser waituntilterminated runhidden
; minimal: the menu items on (the window, where they are switched, is not offered)
Filename: "{app}\HeMiUnlocker.exe"; Parameters: "--menu-on"; Flags: runasoriginaluser waituntilterminated runhidden; Check: not WizardIsComponentSelected('window')
Filename: "{app}\HeMiUnlocker.exe"; Description: "{cm:LaunchProgram,HeMiUnlocker}"; Flags: nowait postinstall skipifsilent runasoriginaluser; Components: window

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
// new folder ("--repair" points the File Explorer items to it) and the old folder is deleted.
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
  Quoted: String;
begin
  OldFolder := FindOldFolder;
  if OldFolder = '' then Exit;
  // the folder as a PowerShell string: an apostrophe doubled (C:\Users\O'Brien), compared as text – no wildcards ([ ])
  Quoted := OldFolder + '\';
  StringChangeEx(Quoted, '''', '''''', True);
  Exec(ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe'),
    '-NoProfile -NonInteractive -Command "$d = ''' + Quoted + '''; Get-Process | Where-Object { $_.Path -and $_.Path.StartsWith($d, [StringComparison]::OrdinalIgnoreCase) } | Stop-Process -Force"',
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
  Dir, Cmd: String;
begin
  // "Take ownership" in the new menu of folders (the verb runas of the user) – only when it is the one of HeMiUnlocker
  if CurUninstallStep = usUninstall then
    if RegQueryStringValue(HKCU, 'Software\Classes\Directory\shell\runas\command', '', Cmd) and (Pos('hemiunlocker', Lowercase(Cmd)) > 0) then
      RegDeleteKeyIncludingSubkeys(HKCU, 'Software\Classes\Directory\shell\runas');
  if CurUninstallStep <> usPostUninstall then Exit;
  Dir := ExpandConstant('{userappdata}\HeMi\HeMiUnlocker');
  if not DirExists(Dir) then Exit;
  if UninstallSilent then Exit;
  if MsgBox(CustomMessage('DeleteSettings') + #13#10#13#10 + CustomMessage('KeepSettings'), mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
  begin
    DelTree(Dir, True, True, True);
    RemoveDir(ExpandConstant('{userappdata}\HeMi'));
  end;
end;
