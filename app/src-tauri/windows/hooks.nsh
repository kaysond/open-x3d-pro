; Open X3D Pro installer hooks, included by Tauri's installer.nsi (which already includes
; LogicLib and x64.nsh). The installer runs elevated (installMode perMachine).
; The NSIS stub is 32-bit: 64-bit-only tools such as pnputil.exe are reached via Sysnative.

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
    !insertmacro X3D_SYSTOOL $1 "certutil.exe"
    DetailPrint "Trusting the Open X3D Pro driver signing certificate"
    ; CI stages self-signed.flag only for self-signed builds; a CA-issued certificate must not become a root.
    ${If} ${FileExists} "${X3D_DRIVER_DIR}\self-signed.flag"
      nsExec::ExecToLog '"$1" -addstore -f Root "${X3D_DRIVER_DIR}\openx3d.cer"'
      Pop $0
      DetailPrint "certutil -addstore Root: exit code $0"
    ${EndIf}
    nsExec::ExecToLog '"$1" -addstore -f TrustedPublisher "${X3D_DRIVER_DIR}\openx3d.cer"'
    Pop $0
    DetailPrint "certutil -addstore TrustedPublisher: exit code $0"

    !insertmacro X3D_SYSTOOL $1 "pnputil.exe"
    DetailPrint "Installing the Open X3D Pro driver"
    nsExec::ExecToLog '"$1" /add-driver "${X3D_DRIVER_DIR}\openx3d.inf" /install'
    Pop $0
    DetailPrint "pnputil /add-driver: exit code $0"
    ; 259 = package staged but no device present; 3010 = done, reboot needed.
    ${If} $0 == 3010
      SetRebootFlag true
    ${ElseIf} $0 != 0
    ${AndIf} $0 != 259
      MessageBox MB_ICONEXCLAMATION|MB_OK "The Open X3D Pro driver could not be installed (pnputil exit code $0).$\r$\nThe app still works with the stock Windows driver, without curves or remapping in games." /SD IDOK
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
  !insertmacro X3D_SYSTOOL $1 "pnputil.exe"
  DetailPrint "Removing the Open X3D Pro driver"
  nsExec::ExecToLog '"$1" /delete-driver openx3d.inf /uninstall /force'
  Pop $0
  DetailPrint "pnputil /delete-driver openx3d.inf: exit code $0"
  ${If} $0 != 0
  ${AndIf} $0 != 3010
    ; pnputil may only accept the published oemN.inf name. Get-WindowsDriver maps original to
    ; published names without parsing pnputil's localized text.
    !insertmacro X3D_SYSTOOL $1 "WindowsPowerShell\v1.0\powershell.exe"
    nsExec::ExecToLog `"$1" -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "Get-WindowsDriver -Online | Where-Object { $$_.OriginalFileName -like '*\openx3d.inf' } | ForEach-Object { pnputil /delete-driver $$_.Driver /uninstall /force; 'pnputil /delete-driver ' + $$_.Driver + ': exit code ' + $$LASTEXITCODE }"`
    Pop $0
    DetailPrint "driver removal via published name: exit code $0"
  ${EndIf}

  ; cert-cn.txt (UTF-16LE, written by CI) holds the signing certificate's CN.
  StrCpy $2 ""
  ClearErrors
  FileOpen $0 "${X3D_DRIVER_DIR}\cert-cn.txt" r
  ${IfNot} ${Errors}
    FileReadUTF16LE $0 $2
    FileClose $0
  ${EndIf}
  ; Never run certutil -delstore with an empty CN.
  ${If} $2 != ""
    !insertmacro X3D_SYSTOOL $1 "certutil.exe"
    DetailPrint "Removing the Open X3D Pro certificate ($2)"
    ${If} ${FileExists} "${X3D_DRIVER_DIR}\self-signed.flag"
      nsExec::ExecToLog '"$1" -delstore Root "$2"'
      Pop $0
      DetailPrint "certutil -delstore Root: exit code $0"
    ${EndIf}
    nsExec::ExecToLog '"$1" -delstore TrustedPublisher "$2"'
    Pop $0
    DetailPrint "certutil -delstore TrustedPublisher: exit code $0"
  ${EndIf}
  Pop $2
  Pop $1
  Pop $0
!macroend
