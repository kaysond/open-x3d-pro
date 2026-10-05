; Open X3D Pro installer hooks, included by Tauri's installer.nsi (which already includes
; LogicLib and x64.nsh). The installer runs elevated (installMode perMachine).
; The NSIS stub is 32-bit: 64-bit-only tools such as pnputil.exe are reached via Sysnative.
; No certificate store is ever touched: the driver is SignPath Foundation-signed and Windows
; asks once whether to install device software from that publisher.

!define X3D_DRIVER_DIR "$INSTDIR\resources\driver"

; Sets OUT to the native (64-bit) path of a System32 tool.
!macro X3D_SYSTOOL OUT NAME
  StrCpy ${OUT} "$WINDIR\Sysnative\${NAME}"
  ${IfNot} ${FileExists} "${OUT}"
    StrCpy ${OUT} "$SYSDIR\${NAME}"
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  Push $0
  Push $1
  ${If} ${FileExists} "${X3D_DRIVER_DIR}\openx3d.inf"
    !insertmacro X3D_SYSTOOL $1 "pnputil.exe"
    DetailPrint "Installing the Open X3D Pro driver"
    nsExec::ExecToLog '"$1" /add-driver "${X3D_DRIVER_DIR}\openx3d.inf" /install'
    Pop $0
    DetailPrint "pnputil /add-driver: exit code $0"
    ; 259 = package staged but no device present; 3010 = done, reboot needed.
    ${If} $0 == 3010
      SetRebootFlag true
    ${EndIf}
    ${If} $0 != 0
    ${AndIf} $0 != 259
    ${AndIf} $0 != 3010
      MessageBox MB_ICONEXCLAMATION|MB_OK "The Open X3D Pro driver could not be installed (pnputil exit code $0).$\r$\nThe app still works with the stock Windows driver, without curves or remapping in games." /SD IDOK
    ${Else}
      ; pnputil /delete-driver only takes published oemN.inf names. Record them now so uninstall
      ; skips the slow lookup. All openx3d packages, since in-place upgrades leave older ones staged.
      ; Get-WindowsDriver is exact and locale-independent, unlike pnputil /enum-drivers text.
      !insertmacro X3D_SYSTOOL $1 "WindowsPowerShell\v1.0\powershell.exe"
      nsExec::ExecToStack `"$1" -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "Get-WindowsDriver -Online | Where-Object { $$_.OriginalFileName -like '*\openx3d.inf' } | ForEach-Object Driver"`
      Pop $0
      Pop $1
      DetailPrint "Get-WindowsDriver openx3d.inf: exit code $0"
      ${If} $0 == 0
      ${AndIf} $1 != ""
        ClearErrors
        FileOpen $0 "${X3D_DRIVER_DIR}\published.txt" w
        ${IfNot} ${Errors}
          FileWrite $0 $1
          FileClose $0
        ${EndIf}
      ${EndIf}
    ${EndIf}
  ${Else}
    DetailPrint "No driver bundled; skipping driver install"
  ${EndIf}
  Pop $1
  Pop $0
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  Push $0
  Push $1
  Push $2
  Push $3
  Push $4
  Push $5
  Push $6
  !insertmacro X3D_SYSTOOL $1 "pnputil.exe"
  DetailPrint "Removing the Open X3D Pro driver"
  ; $3: "" = nothing removed, "ok" = all removed, "fail" = fall back to Get-WindowsDriver.
  StrCpy $3 ""
  ClearErrors
  FileOpen $4 "${X3D_DRIVER_DIR}\published.txt" r
  ${IfNot} ${Errors}
    ${Do}
      ClearErrors
      FileRead $4 $2
      ${IfThen} ${Errors} ${|} ${ExitDo} ${|}
      StrCpy $5 $2 1 -1
      ${IfThen} $5 == "$\n" ${|} StrCpy $2 $2 -1 ${|}
      StrCpy $5 $2 1 -1
      ${IfThen} $5 == "$\r" ${|} StrCpy $2 $2 -1 ${|}
      ${IfThen} $2 == "" ${|} ${Continue} ${|}
      ; oemN numbers are reused, so delete only if the INF copy behind the name is still ours.
      StrCpy $5 ""
      ClearErrors
      FileOpen $6 "$WINDIR\INF\$2" r
      ${IfNot} ${Errors}
        FileRead $6 $5
        FileClose $6
      ${EndIf}
      StrCpy $5 $5 13
      ${If} $5 == "; openx3d.inf"
        nsExec::ExecToLog '"$1" /delete-driver "$2" /uninstall'
        Pop $0
        DetailPrint "pnputil /delete-driver $2: exit code $0"
      ${Else}
        StrCpy $0 "not an openx3d package"
        DetailPrint "Skipping $2: $0"
      ${EndIf}
      ${If} $0 == 0
      ${OrIf} $0 == 3010
        ${IfThen} $3 == "" ${|} StrCpy $3 "ok" ${|}
      ${Else}
        StrCpy $3 "fail"
      ${EndIf}
    ${Loop}
    FileClose $4
  ${EndIf}
  ; Created at install time, so not in Tauri's resource list; delete it or resources\driver stays.
  Delete "${X3D_DRIVER_DIR}\published.txt"

  ${If} $3 != "ok"
    ; Builds before published.txt, or a recorded name that failed: Get-WindowsDriver maps original
    ; to published names without parsing pnputil's localized text.
    !insertmacro X3D_SYSTOOL $1 "WindowsPowerShell\v1.0\powershell.exe"
    nsExec::ExecToLog `"$1" -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "Get-WindowsDriver -Online | Where-Object { $$_.OriginalFileName -like '*\openx3d.inf' } | ForEach-Object { pnputil /delete-driver $$_.Driver /uninstall; 'pnputil /delete-driver ' + $$_.Driver + ': exit code ' + $$LASTEXITCODE }"`
    Pop $0
    DetailPrint "driver removal via Get-WindowsDriver: exit code $0"
  ${EndIf}
  Pop $6
  Pop $5
  Pop $4
  Pop $3
  Pop $2
  Pop $1
  Pop $0
!macroend
