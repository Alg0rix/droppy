; Teardrop Instant One-Click Installer (Discord / VS Code / Spotify style).
;
; Double-click -> installs instantly to %LOCALAPPDATA%\Programs\Teardrop,
; creates Start Menu shortcut, registers Windows App uninstaller, and launches Teardrop.

#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif
#ifndef ExeSource
  #define ExeSource "..\target\release\teardrop.exe"
#endif
#define AppName "Teardrop"
#define AppDir "Teardrop"
#define AppPublisher "Teardrop"
#define AppExe "teardrop.exe"
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
SetupMutex=TeardropSetupMutex
UsePreviousAppDir=yes
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
OutputBaseFilename=teardrop-setup-{#AppVersion}
SetupIconFile=..\crates\teardrop\res\teardrop.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
WizardStyle=modern

Compression=lzma2/max
SolidCompression=yes
; Teardrop is stopped by StopTeardrop() below. Inno's own close-applications
; step asks the app to end the session and shows a "Try again" dialog forever
; when it doesn't, so it stays off.
CloseApplications=no
RestartApplications=no

[Files]
Source: "{#ExeSource}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"; Comment: "A living notch for Windows"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Comment: "A living notch for Windows"

[Registry]
; Start with Windows on a fresh install only: an update must not undo a user
; who turned it off in Teardrop's settings (the app owns this same value).
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
  // can be replaced. (No /T: when Teardrop launched this updater, the
  // updater is its child and must survive.) Exit code 128 = not running.
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/IM {#AppExe} /F', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Sleep(400);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  StopTeardrop();
  Result := '';
end;

// True when Teardrop is already installed (this run is an update).
function IsUpdate(): Boolean;
begin
  Result := RegKeyExists(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Uninstall\{9B78C214-E481-4C59-A125-9FD324A48581}_is1');
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then
    StopTeardrop();
end;
