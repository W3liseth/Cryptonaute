; Installeur Windows de Cryptonaute (NSIS 3, Unicode).
;
; S'exécute en administrateur : c'est la seule élévation nécessaire ; ensuite, les
; utilisateurs se connectent sans droits particuliers grâce au service « Cryptonaute VPN ».
;
; Options de ligne de commande, utilisées par la mise à jour automatique :
;   /P       mode passif : barre de progression seule, aucune question ;
;   /UPDATE  mise à jour : les raccourcis existants sont conservés ;
;   /R       relance Cryptonaute (en tant qu'utilisateur) à la fin ;
;   /S       silencieux (option standard de NSIS).
;
; Compilation (voir scripts/build.ps1) :
;   makensis /DVERSION=x.y.z /DROOT=<racine du dépôt> /DOUTFILE=<installeur> installer.nsi

Unicode true
ManifestDPIAware true
SetCompressor /SOLID lzma

!ifndef VERSION
  !error "VERSION non défini (/DVERSION=x.y.z)"
!endif
!ifndef ROOT
  !error "ROOT non défini (/DROOT=<racine du dépôt>)"
!endif
!ifndef OUTFILE
  !define OUTFILE "Cryptonaute_${VERSION}_x64-setup.exe"
!endif

!define PRODUCT "Cryptonaute"
!define PUBLISHER "Cryptonaute"
!define MAINEXE "cryptonaute.exe"
!define UNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT}"
!define RUNKEY "Software\Microsoft\Windows\CurrentVersion\Run"

!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "FileFunc.nsh"
!include "x64.nsh"

Name "${PRODUCT}"
OutFile "${OUTFILE}"
InstallDir "$PROGRAMFILES64\${PRODUCT}"
InstallDirRegKey HKLM "${UNINSTKEY}" "InstallLocation"
RequestExecutionLevel admin
BrandingText "${PRODUCT} ${VERSION}"

VIProductVersion "${VERSION}.0"
VIAddVersionKey /LANG=1036 "ProductName" "${PRODUCT}"
VIAddVersionKey /LANG=1036 "ProductVersion" "${VERSION}"
VIAddVersionKey /LANG=1036 "FileVersion" "${VERSION}"
VIAddVersionKey /LANG=1036 "FileDescription" "Installeur de ${PRODUCT}"
VIAddVersionKey /LANG=1036 "CompanyName" "${PUBLISHER}"
VIAddVersionKey /LANG=1036 "LegalCopyright" "Cryptonaute contributors — licence MIT"

Var PassiveMode
Var UpdateMode

; ---------- Pages ----------
!define MUI_ICON "${ROOT}\crates\app\icons\icon.ico"
!define MUI_UNICON "${ROOT}\crates\app\icons\icon.ico"
!define MUI_ABORTWARNING

!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive
!insertmacro MUI_PAGE_WELCOME
!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassiveOrUpdate
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN
!define MUI_FINISHPAGE_RUN_TEXT "Lancer ${PRODUCT}"
!define MUI_FINISHPAGE_RUN_FUNCTION LaunchAsUser
!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "French"

Function SkipIfPassive
  ${If} $PassiveMode == 1
    Abort
  ${EndIf}
FunctionEnd

Function SkipIfPassiveOrUpdate
  ${If} $PassiveMode == 1
  ${OrIf} $UpdateMode == 1
    Abort
  ${EndIf}
FunctionEnd

; L'installeur est élevé : on lance Cryptonaute par l'Explorateur, donc avec les
; droits de l'utilisateur connecté.
Function LaunchAsUser
  Exec '"$WINDIR\explorer.exe" "$INSTDIR\${MAINEXE}"'
FunctionEnd

Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_ICONSTOP|MB_OK "${PRODUCT} nécessite une version 64 bits de Windows."
    Abort
  ${EndIf}
  SetRegView 64
  SetShellVarContext all
  ${GetParameters} $R0
  StrCpy $PassiveMode 0
  StrCpy $UpdateMode 0
  ClearErrors
  ${GetOptions} $R0 "/P" $R1
  ${IfNot} ${Errors}
    StrCpy $PassiveMode 1
  ${EndIf}
  ClearErrors
  ${GetOptions} $R0 "/UPDATE" $R1
  ${IfNot} ${Errors}
    StrCpy $UpdateMode 1
  ${EndIf}
  ${If} $PassiveMode == 1
    SetAutoClose true
  ${EndIf}
FunctionEnd

Function .onInstSuccess
  ; /R : relance après une installation passive ou silencieuse (mise à jour).
  ${GetParameters} $R0
  ClearErrors
  ${GetOptions} $R0 "/R" $R1
  ${If} ${Errors}
    Return
  ${EndIf}
  ${If} $PassiveMode == 1
  ${OrIf} ${Silent}
    Call LaunchAsUser
  ${EndIf}
FunctionEnd

; ---------- Migration depuis RustGuard ----------
; Cryptonaute s'appelait « RustGuard » jusqu'à la version 0.2.1. L'ancienne
; installation (service, fichiers, entrée de désinstallation) est retirée ; les
; tunnels des utilisateurs sont repris par l'application à son premier lancement.
!macro CN_REMOVE_LEGACY_RUSTGUARD
  Push $1
  Push $2
  ReadRegStr $1 HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\RustGuard" "InstallLocation"
  ${If} $1 == ""
    StrCpy $1 "$PROGRAMFILES64\RustGuard"
  ${Else}
    StrCpy $2 $1 1
    ${If} $2 == '"'
      StrCpy $1 $1 "" 1
      StrCpy $1 $1 -1
    ${EndIf}
  ${EndIf}
  ${If} ${FileExists} "$1\rustguard-service.exe"
    DetailPrint "Désinstallation de RustGuard (ancien nom de Cryptonaute)…"
    nsExec::Exec 'taskkill /F /IM rustguard.exe'
    Pop $0
    nsExec::ExecToLog '"$1\rustguard-service.exe" uninstall'
    Pop $0
    ${If} ${FileExists} "$1\uninstall.exe"
      ExecWait '"$1\uninstall.exe" /S _?=$1'
    ${EndIf}
    RMDir /r "$1"
  ${EndIf}
  DeleteRegValue HKCU "${RUNKEY}" "RustGuard"
  Pop $2
  Pop $1
!macroend

; ---------- Installation ----------
Section "Cryptonaute" SecMain
  SectionIn RO
  SetOutPath "$INSTDIR"

  ; L'application en cours d'exécution verrouille ses fichiers.
  DetailPrint "Fermeture de ${PRODUCT}…"
  nsExec::Exec 'taskkill /F /IM ${MAINEXE}'
  Pop $0

  ; Mise à jour : arrête et supprime l'ancien service avant de remplacer ses fichiers.
  ${If} ${FileExists} "$INSTDIR\cryptonaute-service.exe"
    DetailPrint "Arrêt du service Cryptonaute existant…"
    nsExec::ExecToLog '"$INSTDIR\cryptonaute-service.exe" uninstall'
    Pop $0
  ${EndIf}
  !insertmacro CN_REMOVE_LEGACY_RUSTGUARD

  File "${ROOT}\target\release\cryptonaute.exe"
  File "${ROOT}\target\release\cryptonaute-service.exe"
  File "${ROOT}\third_party\wireguard-nt\wireguard.dll"
  File "/oname=wireguard-nt-LICENSE.txt" "${ROOT}\third_party\wireguard-nt\LICENSE.txt"
  File "/oname=LICENSE.txt" "${ROOT}\LICENSE"

  DetailPrint "Installation du service Cryptonaute VPN…"
  nsExec::ExecToLog '"$INSTDIR\cryptonaute-service.exe" install'
  Pop $0
  ${If} $0 != 0
    MessageBox MB_ICONEXCLAMATION|MB_OK "Le service Cryptonaute n'a pas pu être installé (code $0).$\r$\nConsultez $INSTDIR\logs\service.log."
  ${EndIf}

  ; Désinstallation et entrée dans « Applications installées »
  WriteUninstaller "$INSTDIR\uninstall.exe"
  WriteRegStr HKLM "${UNINSTKEY}" "DisplayName" "${PRODUCT}"
  WriteRegStr HKLM "${UNINSTKEY}" "DisplayIcon" '"$INSTDIR\${MAINEXE}"'
  WriteRegStr HKLM "${UNINSTKEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKLM "${UNINSTKEY}" "Publisher" "${PUBLISHER}"
  WriteRegStr HKLM "${UNINSTKEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKLM "${UNINSTKEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKLM "${UNINSTKEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
  WriteRegDWORD HKLM "${UNINSTKEY}" "NoModify" 1
  WriteRegDWORD HKLM "${UNINSTKEY}" "NoRepair" 1
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  WriteRegDWORD HKLM "${UNINSTKEY}" "EstimatedSize" $0

  ; Raccourcis (conservés tels quels lors d'une mise à jour)
  ${If} $UpdateMode == 0
    CreateShortcut "$SMPROGRAMS\${PRODUCT}.lnk" "$INSTDIR\${MAINEXE}"
    CreateShortcut "$DESKTOP\${PRODUCT}.lnk" "$INSTDIR\${MAINEXE}"
  ${ElseIfNot} ${FileExists} "$SMPROGRAMS\${PRODUCT}.lnk"
    CreateShortcut "$SMPROGRAMS\${PRODUCT}.lnk" "$INSTDIR\${MAINEXE}"
  ${EndIf}
SectionEnd

; ---------- Désinstallation ----------
Function un.onInit
  SetRegView 64
  SetShellVarContext all
FunctionEnd

Section "Uninstall"
  nsExec::Exec 'taskkill /F /IM ${MAINEXE}'
  Pop $0
  ${If} ${FileExists} "$INSTDIR\cryptonaute-service.exe"
    DetailPrint "Suppression du service Cryptonaute VPN…"
    nsExec::ExecToLog '"$INSTDIR\cryptonaute-service.exe" uninstall'
    Pop $0
  ${EndIf}

  Delete "$INSTDIR\${MAINEXE}"
  Delete "$INSTDIR\cryptonaute-service.exe"
  Delete "$INSTDIR\wireguard.dll"
  Delete "$INSTDIR\wireguard-nt-LICENSE.txt"
  Delete "$INSTDIR\LICENSE.txt"
  Delete "$INSTDIR\uninstall.exe"
  RMDir /r "$INSTDIR\logs"
  RMDir "$INSTDIR"

  Delete "$SMPROGRAMS\${PRODUCT}.lnk"
  Delete "$DESKTOP\${PRODUCT}.lnk"
  DeleteRegKey HKLM "${UNINSTKEY}"
  ; Clé laissée par les installeurs des versions 0.5.x et antérieures.
  DeleteRegKey HKLM "Software\${PUBLISHER}\${PRODUCT}"
  DeleteRegKey /ifempty HKLM "Software\${PUBLISHER}"
  ; Option « Lancer au démarrage » de l'utilisateur qui désinstalle. Les
  ; configurations des utilisateurs (dans leur profil) sont conservées.
  DeleteRegValue HKCU "${RUNKEY}" "${PRODUCT}"
SectionEnd
