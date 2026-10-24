; Teardrop per-user installer (no admin rights).
; Build with: makensis /DVERSION=0.1.0 tools/installer.nsi
; Expects dist/teardrop.exe built via `cargo build -p teardrop --release`.
; Output: dist/teardrop-setup-<version>.exe

Unicode True

!ifndef VERSION
!define VERSION "0.1.0"
!endif

!define ROOT ".."  ; makensis runs from this script's folder
!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\Teardrop"

Name "Teardrop ${VERSION}"
OutFile "${ROOT}\dist\teardrop-setup-${VERSION}.exe"
InstallDir "$LOCALAPPDATA\Programs\Teardrop"
InstallDirRegKey HKCU "${UNINST_KEY}" "InstallLocation"
RequestExecutionLevel user
ShowInstDetails show
BrandingText "Teardrop ${VERSION}"

Page directory
Page instfiles
UninstPage uninstConfirm
UninstPage instfiles

Section "Install"
  ; Reinstalling over a running island would fail on the locked exe.
  nsExec::Exec 'taskkill /IM teardrop.exe /F'
  Pop $0
  Sleep 300

  SetOutPath "$INSTDIR"
  File "${ROOT}\dist\teardrop.exe"
  File "${ROOT}\LICENSE"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  CreateShortcut "$SMPROGRAMS\Teardrop.lnk" "$INSTDIR\teardrop.exe"

  ; "Start with Windows" is a setting inside the app (off by default), so the
  ; installer does not register autostart itself.
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayName" "Teardrop"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINST_KEY}" "Publisher" "Teardrop"
  WriteRegStr HKCU "${UNINST_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKCU "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoRepair" 1

  Exec '"$INSTDIR\teardrop.exe"'
SectionEnd

Section "Uninstall"
  nsExec::Exec 'taskkill /IM teardrop.exe /F'
  Pop $0
  Sleep 300

  Delete "$INSTDIR\teardrop.exe"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\uninstall.exe"
  Delete "$SMPROGRAMS\Teardrop.lnk"
  ; Remove autostart if the user had enabled it in Settings.
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Teardrop"
  DeleteRegKey HKCU "${UNINST_KEY}"
  RMDir "$INSTDIR"

  ; Cache + log are disposable; user data in %APPDATA%\Teardrop is kept on purpose.
  RMDir /r "$LOCALAPPDATA\Teardrop"
SectionEnd
