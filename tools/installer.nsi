; Droppy per-user installer (no admin rights).
; Build with: makensis /DVERSION=0.1.0 tools/installer.nsi
; Expects dist/droppy.exe built via `cargo build -p teardrop --release`.
; Output: dist/droppy-setup-<version>.exe

Unicode True

!ifndef VERSION
!define VERSION "0.1.0"
!endif

!define ROOT ".."  ; makensis runs from this script's folder
!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\Droppy"

Name "Droppy ${VERSION}"
OutFile "${ROOT}\dist\droppy-setup-${VERSION}.exe"
InstallDir "$LOCALAPPDATA\Programs\Droppy"
InstallDirRegKey HKCU "${UNINST_KEY}" "InstallLocation"
RequestExecutionLevel user
ShowInstDetails show
BrandingText "Droppy ${VERSION}"

Page directory
Page instfiles
UninstPage uninstConfirm
UninstPage instfiles

Section "Install"
  ; Reinstalling over a running island would fail on the locked exe.
  nsExec::Exec 'taskkill /IM droppy.exe /F'
  Pop $0
  Sleep 300

  SetOutPath "$INSTDIR"
  File "${ROOT}\dist\droppy.exe"
  File "${ROOT}\LICENSE"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  CreateShortcut "$SMPROGRAMS\Droppy.lnk" "$INSTDIR\droppy.exe"

  ; "Start with Windows" is a setting inside the app (off by default), so the
  ; installer does not register autostart itself.
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayName" "Droppy"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINST_KEY}" "Publisher" "Droppy"
  WriteRegStr HKCU "${UNINST_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKCU "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoRepair" 1

  Exec '"$INSTDIR\droppy.exe"'
SectionEnd

Section "Uninstall"
  nsExec::Exec 'taskkill /IM droppy.exe /F'
  Pop $0
  Sleep 300

  Delete "$INSTDIR\droppy.exe"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\uninstall.exe"
  Delete "$SMPROGRAMS\Droppy.lnk"
  ; Remove autostart if the user had enabled it in Settings.
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Droppy"
  DeleteRegKey HKCU "${UNINST_KEY}"
  RMDir "$INSTDIR"

  ; Cache + log are disposable; user data in %APPDATA%\Droppy is kept on purpose.
  RMDir /r "$LOCALAPPDATA\Droppy"
SectionEnd
