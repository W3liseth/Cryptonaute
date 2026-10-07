; Hooks NSIS de Cryptonaute, inclus par l'installeur généré par Tauri.
; L'installeur s'exécute en administrateur (installMode = perMachine) : c'est la
; seule élévation nécessaire, ensuite les utilisateurs n'en ont plus besoin.

!include "LogicLib.nsh"

!define CN_ROOT "${__FILEDIR__}\..\..\.."

!macro NSIS_HOOK_PREINSTALL
  ; Mise à jour : arrête et supprime l'ancien service avant de remplacer ses fichiers.
  ${If} ${FileExists} "$INSTDIR\cryptonaute-service.exe"
    DetailPrint "Arrêt du service Cryptonaute existant…"
    nsExec::ExecToLog '"$INSTDIR\cryptonaute-service.exe" uninstall'
    Pop $0
  ${EndIf}
  !insertmacro CN_REMOVE_LEGACY_RUSTGUARD
!macroend

; Migration : Cryptonaute s'appelait « RustGuard » jusqu'à la version 0.2.1.
; L'ancienne installation (service, fichiers, entrée de désinstallation) est retirée ;
; les tunnels des utilisateurs sont repris par l'application à son premier lancement.
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
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "RustGuard"
  Pop $2
  Pop $1
!macroend

!macro NSIS_HOOK_POSTINSTALL
  SetOutPath "$INSTDIR"
  File "${CN_ROOT}\target\release\cryptonaute-service.exe"
  File "${CN_ROOT}\third_party\wireguard-nt\wireguard.dll"
  File "/oname=wireguard-nt-LICENSE.txt" "${CN_ROOT}\third_party\wireguard-nt\LICENSE.txt"

  DetailPrint "Installation du service Cryptonaute VPN…"
  nsExec::ExecToLog '"$INSTDIR\cryptonaute-service.exe" install'
  Pop $0
  ${If} $0 != 0
    MessageBox MB_ICONEXCLAMATION|MB_OK "Le service Cryptonaute n'a pas pu être installé (code $0).$\r$\nConsultez $INSTDIR\logs\service.log."
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ${If} ${FileExists} "$INSTDIR\cryptonaute-service.exe"
    DetailPrint "Suppression du service Cryptonaute VPN…"
    nsExec::ExecToLog '"$INSTDIR\cryptonaute-service.exe" uninstall'
    Pop $0
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; Option « Lancer au démarrage » de l'utilisateur qui désinstalle.
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Cryptonaute"
  Delete "$INSTDIR\cryptonaute-service.exe"
  Delete "$INSTDIR\wireguard.dll"
  Delete "$INSTDIR\wireguard-nt-LICENSE.txt"
  RMDir /r "$INSTDIR\logs"
  RMDir "$INSTDIR"
!macroend
