; Droppy Instant One-Click Installer (Discord / VS Code / Spotify style).
;
; Double-click -> installs instantly to %LOCALAPPDATA%\Programs\Droppy,
; creates Start Menu shortcut, registers Windows App uninstaller, and launches Droppy.
; The app used to be called Teardrop: the AppId below is unchanged so this
; installs over an old copy, and [InstallDelete] clears the old folder.

#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif
#ifndef ExeSource
  #define ExeSource "..\target\release\droppy.exe"
#endif
#define AppName "Droppy"
#define AppDir "Droppy"
#define AppPublisher "Droppy"
#define AppExe "droppy.exe"
; Names from before the rename, cleaned up when updating an old install.
#define OldAppDir "Teardrop"
#define OldAppExe "teardrop.exe"
#define IslandClass "TeardropIsland"

[Setup]
AppId={{9B78C214-E481-4C59-A125-9FD324A48581}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#AppPublisher}
AppCopyright=Copyright (C) 2026 {#AppPublisher}
VersionInfoVersion={#AppVersion}
VersionInfoCompany={#AppPublisher}
VersionInfoDescription={#AppName} Installer
VersionInfoProductName={#AppName}
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
SetupMutex=DroppySetupMutex
; The folder name changed with the rename, so never reuse the old location.
UsePreviousAppDir=no
DefaultDirName={localappdata}\Programs\{#AppDir}
DefaultGroupName={#AppName}

; Streamlined One-Click: skip all wizard pages
DisableWelcomePage=yes
DisableDirPage=yes
DisableProgramGroupPage=yes
DisableReadyPage=yes
DisableFinishedPage=yes
ShowLanguageDialog=no

OutputDir=..\dist
OutputBaseFilename=droppy-setup-{#AppVersion}
SetupIconFile=..\crates\teardrop\res\teardrop.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
WizardStyle=modern

Compression=lzma2/max
SolidCompression=yes
; Droppy is stopped by StopTeardrop() below. Inno's own close-applications
; step asks the app to end the session and shows a "Try again" dialog forever
; when it doesn't, so it stays off.
CloseApplications=no
RestartApplications=no

[InstallDelete]
; Leftovers of the pre-rename install (Teardrop).
Type: filesandordirs; Name: "{localappdata}\Programs\{#OldAppDir}"
Type: files; Name: "{autoprograms}\{#OldAppDir}.lnk"
Type: files; Name: "{autodesktop}\{#OldAppDir}.lnk"

[Files]
Source: "{#ExeSource}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"; Comment: "A living notch for Windows"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Comment: "A living notch for Windows"

[Registry]
; Start with Windows on a fresh install only: an update must not undo a user
; who turned it off in Droppy's settings (the app owns this same value; it
; also carries a pre-rename "Teardrop" entry over on first start).
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "{#AppDir}"; ValueData: """{app}\{#AppExe}"""; Flags: uninsdeletevalue; Check: not IsUpdate

[Run]
Filename: "{app}\{#AppExe}"; Flags: nowait

[UninstallRun]
Filename: "{sys}\taskkill.exe"; Parameters: "/IM {#AppExe} /F"; Flags: runhidden waituntilterminated; RunOnceId: "StopTeardrop"

[UninstallDelete]
Type: filesandordirs; Name: "{localappdata}\{#AppDir}"

[Code]
const
  WM_CLOSE = $0010;

function OpenMutexW(dwDesiredAccess: DWORD; bInheritHandle: BOOL; lpName: String): THandle;
  external 'OpenMutexW@kernel32.dll stdcall';
function CloseHandle(hObject: THandle): BOOL;
  external 'CloseHandle@kernel32.dll stdcall';

// The app holds this mutex for its whole lifetime (the single-instance
// guard), so it existing *probably* means droppy is up. But a stale handle
// can outlive the process (a helper that opened it, slow OS reaping after
// /F), so the image list is the ground truth: taskkill exit 128 = gone.
function TeardropRunning(): Boolean;
var
  M: THandle;
  ResultCode: Integer;
begin
  M := OpenMutexW($00100000, False, 'Local\TeardropIslandMutex');
  if M = 0 then
  begin
    Result := False;
    exit;
  end;
  CloseHandle(M);
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/IM {#AppExe} /F', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  if ResultCode = 0 then
  begin
    Result := True;
    exit;
  end;
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/IM {#OldAppExe} /F', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Result := ResultCode = 0;
end;

procedure StopTeardrop();
var
  I, ResultCode: Integer;
  Wnd: HWND;
begin
  // Ask nicely first so recordings and settings get saved...
  Wnd := FindWindowByClassName('{#IslandClass}');
  if Wnd <> 0 then
  begin
    PostMessage(Wnd, WM_CLOSE, 0, 0);
    for I := 1 to 40 do
    begin
      if FindWindowByClassName('{#IslandClass}') = 0 then
        Break;
      Sleep(50);
    end;
  end;
  // ...then make sure the process is really gone, window or not, so the exe
  // can be replaced. (No /T: when Droppy launched this updater, the
  // updater is its child and must survive.) Exit code 128 = not running.
  // The pre-rename exe name is stopped too, for updates over an old install.
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/IM {#AppExe} /F', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/IM {#OldAppExe} /F', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Sleep(400);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  I: Integer;
begin
  // A surviving droppy.exe means DeleteFile fails mid-install with a raw
  // "Access is denied" retry dialog; retry the kill a few times, then abort
  // cleanly with a message instead.
  for I := 1 to 4 do
  begin
    StopTeardrop();
    if not TeardropRunning() then
    begin
      Result := '';
      exit;
    end;
  end;
  Result := 'Droppy is still running and could not be closed. Quit it from its menu, then run this installer again.';
end;

// True when Droppy (or its pre-rename self) is already installed: an update.
function IsUpdate(): Boolean;
begin
  Result := RegKeyExists(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Uninstall\{9B78C214-E481-4C59-A125-9FD324A48581}_is1');
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then
    StopTeardrop();
end;
