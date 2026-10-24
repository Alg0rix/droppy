; Teardrop per-user installer (no admin rights).
; Build with: makensis /DVERSION=0.1.0 tools/installer.nsi
; Expects dist/teardrop.exe built via `cargo build -p teardrop --release`.

!ifndef VERSION
!define VERSION "0.1.0"
!endif

Name "Teardrop ${VERSION}"
OutFile "dist/teardrop-setup-${VERSION}.exe"
InstallDir "$LOCALAPPDATA\Programs\Teardrop"
RequestExecutionLevel user
ShowInstDetails show

Section "Install"
  SetOutPath "$INSTDIR"
  File "dist/teardrop.exe"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  CreateShortcut "$SMPROGRAMS\Teardrop.lnk" "$INSTDIR\teardrop.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Teardrop" '"$INSTDIR\teardrop.exe"'
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Teardrop" "DisplayName" "Teardrop ${VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Teardrop" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Teardrop" "InstallLocation" "$INSTDIR"
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\teardrop.exe"
  Delete "$INSTDIR\uninstall.exe"
  Delete "$SMPROGRAMS\Teardrop.lnk"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Teardrop"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Teardrop"
  RMDir "$INSTDIR"
  ; NOTE: %APPDATA%\Teardrop (user data) is intentionally kept.
  ; Cache + log live in %LOCALAPPDATA%\Teardrop.
SectionEnd
