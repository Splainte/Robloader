; Hooks NSIS pour l'installeur Robloader V2 (Tauri).
; Reference depuis tauri.conf.json -> bundle.windows.nsis.installerHooks.

; --- Migration V1 -> V2 -------------------------------------------------------
; Robloader V1 etait installe par Inno Setup (Program Files, admin). NSIS ne
; reconnait que ses propres installs : sans ce hook, V2 cohabiterait avec V1
; (deux entrees "Robloader"). On desinstalle donc V1 AVANT d'installer V2.
; Cle de desinstallation Inno (AppName "Robloader", sans AppId) : "Robloader_is1".
!macro NSIS_HOOK_PREINSTALL
  StrCpy $0 ""

  ; Inno installe en mode 64 bits (ArchitecturesInstallIn64BitMode=x64) -> vue 64.
  SetRegView 64
  ReadRegStr $0 HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Robloader_is1" "UninstallString"
  StrCmp $0 "" 0 rob_found

  ; Repli : vue 32 bits, puis ruche utilisateur (si V1 avait ete pose sans admin).
  SetRegView 32
  ReadRegStr $0 HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Robloader_is1" "UninstallString"
  StrCmp $0 "" 0 rob_found
  ReadRegStr $0 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Robloader_is1" "UninstallString"
  StrCmp $0 "" rob_done rob_found

  rob_found:
    ; UninstallString = chemin de unins000.exe entre guillemets -> on les retire.
    StrCpy $2 $0 1
    StrCmp $2 '"' 0 rob_noquote
      StrCpy $1 $0 "" 1
      StrCpy $1 $1 -1
      Goto rob_exec
    rob_noquote:
      StrCpy $1 $0
    rob_exec:
      DetailPrint "Suppression de l'ancienne version de Robloader (V1)..."
      ; unins000.exe est manifeste 'requireAdministrator' : le verbe 'runas'
      ; declenche l'elevation (UAC). Un seul prompt, le temps de la migration.
      ExecShellWait "runas" "$1" "/VERYSILENT /SUPPRESSMSGBOXES /NORESTART"

  rob_done:
    ; Restaure la vue de registre par defaut de NSIS.
    SetRegView 32
!macroend
