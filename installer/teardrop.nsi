; Teardrop installer (NSIS 3). Per-user install, no admin rights needed.
;
; Build:
;   makensis -DEXE=/abs/path/teardrop.exe -DOUTDIR=/abs/path/dist installer/teardrop.nsi

Target amd64-unicode
SetCompressor /SOLID lzma

!define APPNAME "Teardrop"
!define VERSION "0.1.0"
!define PUBLISHER "Teardrop"
!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\Teardrop"
!define RUN_KEY "Software\Microsoft\Windows\CurrentVersion\Run"

!ifndef EXE
  !define EXE "..\target\release\teardrop.exe"
!endif
!ifndef OUTDIR
  !define OUTDIR "..\dist"
!endif

Name "${APPNAME}"
OutFile "${OUTDIR}\teardrop-setup-${VERSION}.exe"
RequestExecutionLevel user
InstallDir "$LOCALAPPDATA\Programs\Teardrop"
InstallDirRegKey HKCU "Software\Teardrop" "InstallDir"
BrandingText "${APPNAME} ${VERSION}"

VIProductVersion "0.1.0.0"
VIAddVersionKey "ProductName" "${APPNAME}"
VIAddVersionKey "CompanyName" "${PUBLISHER}"
VIAddVersionKey "FileDescription" "Teardrop installer"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "LegalCopyright" "Copyright (C) 2026 ${PUBLISHER}"

!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "WinMessages.nsh"

!macro StopTeardrop
  FindWindow $0 "TeardropIsland"
  ${If} $0 <> 0
    SendMessage $0 ${WM_CLOSE} 0 0 /TIMEOUT=2000
    StrCpy $1 0
    ${DoWhile} $0 <> 0
      ${If} $1 >= 30
        ${Break}
      ${EndIf}
      Sleep 100
      FindWindow $0 "TeardropIsland"
      IntOp $1 $1 + 1
    ${Loop}
  ${EndIf}
  nsExec::Exec '"$SYSDIR\taskkill.exe" /IM teardrop.exe /F'
  Sleep 200
!macroend

!define MUI_ICON "..\crates\teardrop\res\teardrop.ico"
!define MUI_UNICON "..\crates\teardrop\res\teardrop.ico"
!define MUI_WELCOMEFINISHPAGE_BITMAP "art\nsis-welcome.bmp"
!define MUI_HEADERIMAGE
!define MUI_HEADERIMAGE_RIGHT
!define MUI_HEADERIMAGE_BITMAP "art\nsis-header.bmp"
!define MUI_ABORTWARNING
!define MUI_COMPONENTSPAGE_SMALLDESC
!define MUI_FINISHPAGE_RUN "$INSTDIR\teardrop.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Launch Teardrop now"
!define MUI_WELCOMEPAGE_TITLE "Welcome to Teardrop"
!define MUI_WELCOMEPAGE_TEXT "Teardrop puts a living notch at the top of your screen: music with synced lyrics, a focus timer, a file shelf, tasks, clipboard history and more.$\r$\n$\r$\nIt installs just for you and doesn't need administrator rights."

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Section "Teardrop" SecMain
  SectionIn RO
  !insertmacro StopTeardrop
  SetOutPath "$INSTDIR"
  File "/oname=teardrop.exe" "${EXE}"
  File "..\LICENSE"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  CreateShortcut "$SMPROGRAMS\${APPNAME}.lnk" "$INSTDIR\teardrop.exe" "" "$INSTDIR\teardrop.exe" 0
  WriteRegStr HKCU "Software\Teardrop" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayName" "${APPNAME}"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINST_KEY}" "Publisher" "${PUBLISHER}"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\teardrop.exe"
  WriteRegStr HKCU "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINST_KEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKCU "${UNINST_KEY}" "QuietUninstallString" '"$INSTDIR\Uninstall.exe" /S'
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoRepair" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "EstimatedSize" 2048
SectionEnd

Section "Start Teardrop with Windows" SecAutostart
  WriteRegStr HKCU "${RUN_KEY}" "Teardrop" '"$INSTDIR\teardrop.exe"'
SectionEnd

!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SecMain} "The Teardrop app and a Start Menu shortcut."
  !insertmacro MUI_DESCRIPTION_TEXT ${SecAutostart} "Open Teardrop automatically when you sign in. You can change this later from its menu."
!insertmacro MUI_FUNCTION_DESCRIPTION_END

Section "Uninstall"
  !insertmacro StopTeardrop
  Delete "$INSTDIR\teardrop.exe"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\${APPNAME}.lnk"
  DeleteRegValue HKCU "${RUN_KEY}" "Teardrop"
  DeleteRegKey HKCU "${UNINST_KEY}"
  DeleteRegKey HKCU "Software\Teardrop"
  RMDir /r "$LOCALAPPDATA\Teardrop"
SectionEnd
