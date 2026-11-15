; Open X3D Pro installer hooks, included by Tauri's installer.nsi (which already includes
; LogicLib, x64.nsh, FileFunc.nsh and MUI2/nsDialogs). The installer runs elevated (installMode perMachine).
; The NSIS stub is 32-bit: 64-bit-only tools such as pnputil.exe are reached via Sysnative.
; The driver catalog ships unsigned. Only if the box on the certificate page stays ticked (or the user passes
; /TRUSTCERT to a silent/passive install), sign-catalog.ps1 signs it with a certificate made on this PC,
; deletes its private key and trusts that certificate (the libwdi/Zadig approach); otherwise the driver
; is not installed.
; The driver files and sign-catalog.ps1 are embedded here and extracted to $PLUGINSDIR, never to $INSTDIR:
; a user-chosen install folder may be writable by other users, and these files run or get signed elevated.

!define X3D_DRIVER_SRC "${__FILEDIR__}\..\resources\driver"
!define X3D_SIGN_SRC "${__FILEDIR__}\..\..\..\driver\scripts\sign-catalog.ps1"
!define X3D_DRIVER_DIR "$PLUGINSDIR\driver"
; HKLM value set while the user's consent stands. Read before Tauri's reinstall page runs the old
; uninstaller (which clears it), so upgrades keep the driver without asking again.
!define X3D_REG_KEY "Software\OpenX3DPro"
!define X3D_REG_CONSENT "TrustDriverCert"
; Provider in openx3d.inf; tells our packages from another vendor's openx3d.inf.
!define X3D_PROVIDER "Open X3D Pro project"
!define X3D_SIGNING_DOCS "https://github.com/kaysond/open-x3d-pro#driver-signing"

!if /FileExists "${X3D_DRIVER_SRC}\openx3d.inf"
  !define X3D_HAS_DRIVER
!endif

; Sets OUT to the native (64-bit) path of a System32 tool.
!macro X3D_SYSTOOL OUT NAME
  StrCpy ${OUT} "$WINDIR\Sysnative\${NAME}"
  ${IfNot} ${FileExists} "${OUT}"
    StrCpy ${OUT} "$SYSDIR\${NAME}"
  ${EndIf}
!macroend

; Extracts sign-catalog.ps1 to ${X3D_DRIVER_DIR}, plus the driver package when INSTALL is 1.
!macro X3D_EXTRACT INSTALL
  InitPluginsDir
  SetOutPath "${X3D_DRIVER_DIR}"
  File "${X3D_SIGN_SRC}"
  !if "${INSTALL}" == "1"
    File "${X3D_DRIVER_SRC}\openx3d.inf"
    File "${X3D_DRIVER_SRC}\openx3d.dll"
    File "${X3D_DRIVER_SRC}\openx3d.cat"
    SetOutPath "$INSTDIR"
  !else
    ; Not $INSTDIR: the uninstaller cannot remove its working directory.
    SetOutPath "$TEMP"
  !endif
!macroend

!ifdef X3D_HAS_DRIVER

Var X3DCertCheckbox
; "1" = trust the certificate and install the driver, "0" = don't, "" = page not shown (silent/passive).
Var X3DTrustCert

; Declared before Tauri's pages, so this is the first page of the wizard.
Page custom X3DCertPage X3DCertPageLeave

Function X3DCertPage
  ${If} $X3DTrustCert == ""
    ReadRegStr $0 HKLM "${X3D_REG_KEY}" "${X3D_REG_CONSENT}"
    ${IfThen} $0 == "1" ${|} StrCpy $X3DTrustCert "1" ${|}
  ${EndIf}
  ; Passive mode (/P) shows no pages; $PassiveMode is declared after this file, so parse it here.
  ClearErrors
  ${GetOptions} $CMDLINE "/P" $0
  ${IfNot} ${Errors}
    Abort
  ${EndIf}

  !insertmacro MUI_HEADER_TEXT "Driver certificate" "Open X3D Pro signs its driver with its own certificate."
  nsDialogs::Create 1018
  Pop $0
  ${If} $0 == error
    Abort
  ${EndIf}

  ${NSD_CreateLabel} 0 0 100% 56u "Windows installs the Open X3D Pro driver only if it is signed by a certificate this PC trusts. The setup creates a certificate on this PC, signs the driver with it and immediately deletes its private key, then adds it to the trusted stores, so nothing else can ever be signed with it. Nobody else, including the Open X3D Pro developers, holds a key this PC trusts. Uninstalling removes the certificate."
  Pop $0

  ${NSD_CreateCheckbox} 0 62u 100% 12u "&Create and trust a driver certificate for this PC, and install the driver"
  Pop $X3DCertCheckbox
  ; Ticked by default; "0" only when the user unticked it and came back to this page.
  ${If} $X3DTrustCert != "0"
    ${NSD_Check} $X3DCertCheckbox
  ${EndIf}

  ${NSD_CreateLabel} 0 80u 100% 24u "If you untick this, only the app is installed and the joystick keeps the stock Windows driver: no curves or remapping in games. You can run this installer again later."
  Pop $0

  ${NSD_CreateLink} 0 110u 100% 12u "Why is this needed, and how can I check the certificate?"
  Pop $0
  ${NSD_OnClick} $0 X3DOpenSigningDocs

  nsDialogs::Show
FunctionEnd

Function X3DCertPageLeave
  ${NSD_GetState} $X3DCertCheckbox $0
  ${If} $0 == ${BST_CHECKED}
    StrCpy $X3DTrustCert "1"
  ${Else}
    StrCpy $X3DTrustCert "0"
  ${EndIf}
FunctionEnd

Function X3DOpenSigningDocs
  Pop $0 ; clicked control
  ExecShell "open" "${X3D_SIGNING_DOCS}"
FunctionEnd

!macro NSIS_HOOK_POSTINSTALL
  Push $0
  Push $1
  ${If} $X3DTrustCert == ""
    ; Silent install: /TRUSTCERT, or earlier consent if no uninstaller ran (/UPDATE).
    ReadRegStr $X3DTrustCert HKLM "${X3D_REG_KEY}" "${X3D_REG_CONSENT}"
    ClearErrors
    ${GetOptions} $CMDLINE "/TRUSTCERT" $0
    ${IfNot} ${Errors}
      StrCpy $X3DTrustCert "1"
    ${EndIf}
  ${EndIf}

  ${If} $X3DTrustCert != "1"
    DeleteRegValue HKLM "${X3D_REG_KEY}" "${X3D_REG_CONSENT}"
    DetailPrint "Driver certificate not trusted by the user; skipping driver install"
  ${Else}
    !insertmacro X3D_EXTRACT 1
    !insertmacro X3D_SYSTOOL $1 "WindowsPowerShell\v1.0\powershell.exe"
    DetailPrint "Signing the driver with a certificate created on this PC"
    nsExec::ExecToLog '"$1" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "${X3D_DRIVER_DIR}\sign-catalog.ps1" -Dir "${X3D_DRIVER_DIR}"'
    Pop $0
    DetailPrint "sign-catalog.ps1: exit code $0"
    ${If} $0 != 0
      MessageBox MB_ICONEXCLAMATION|MB_OK "The Open X3D Pro driver could not be signed (exit code $0).$\r$\nThe app still works with the stock Windows driver, without curves or remapping in games." /SD IDOK
    ${Else}
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
        WriteRegStr HKLM "${X3D_REG_KEY}" "${X3D_REG_CONSENT}" "1"
      ${EndIf}
    ${EndIf}
  ${EndIf}
  Pop $1
  Pop $0
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  Push $0
  Push $1
  DetailPrint "Removing the Open X3D Pro driver"
  DeleteRegKey HKLM "${X3D_REG_KEY}"
  ; Written by installers up to 0.1.0.
  Delete "$INSTDIR\driver-packages.txt"
  ; pnputil /delete-driver only takes published oemN.inf names. Get-WindowsDriver maps the original
  ; name to them, exactly and without parsing pnputil's localized text.
  !insertmacro X3D_SYSTOOL $1 "WindowsPowerShell\v1.0\powershell.exe"
  nsExec::ExecToLog `"$1" -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "Get-WindowsDriver -Online | Where-Object { $$_.OriginalFileName -like '*\openx3d.inf' -and $$_.ProviderName -eq '${X3D_PROVIDER}' } | ForEach-Object { pnputil /delete-driver $$_.Driver /uninstall; 'pnputil /delete-driver ' + $$_.Driver + ': exit code ' + $$LASTEXITCODE }"`
  Pop $0
  DetailPrint "driver removal via Get-WindowsDriver: exit code $0"

  DetailPrint "Removing the driver certificate"
  !insertmacro X3D_EXTRACT 0
  nsExec::ExecToLog '"$1" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "${X3D_DRIVER_DIR}\sign-catalog.ps1" -Remove'
  Pop $0
  DetailPrint "sign-catalog.ps1 -Remove: exit code $0"
  Pop $1
  Pop $0
!macroend

!endif
