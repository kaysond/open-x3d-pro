# Open X3D Pro driver

A user-mode (UMDF 2) HID minidriver for the Logitech Extreme 3D Pro (`USB\VID_046D&PID_C215`). It takes over from the inbox `hidusb`, reads the stick's 7-byte reports itself, and runs them through the configurable pipeline in `pipeline/` (calibration, inversion, deadzones, curves, axis remap, hat-as-buttons, a shift layer, button remap). It then publishes a 32-button joystick and a vendor channel for the app. There is no kernel code, so Secure Boot and Memory Integrity (HVCI) can stay on. The binding spec is in [`docs/CONTRACT.md`](../docs/CONTRACT.md) §2 and §4.

## How it binds

```
hidclass + mshidumdf.sys   function driver (kernel, inbox): HID class <-> UMDF bridge
WUDFRd.sys                 lower filter (inbox reflector) -> WUDFHost.exe loads openx3d.dll
openx3d.dll                this driver: WDF USB target, continuous reader on EP 0x81
USB\VID_046D&PID_C215      the stick (low speed, 10 ms interrupt-IN, 7-byte reports)
```

`openx3d.inf` matches the USB device node (`&REV_0204` ranks 0x00FF0000, plain ID 0x00FF0001), so it outranks inbox `input.inf` (0x00FF2005 on Win11) and LGS's `WmJoy.HidDevice`. The device is never sent a class request, because it stalls GET_REPORT/GET_FEATURE.

The driver publishes its own report descriptor (`openx3d/descriptor.h`, kept byte-identical to `core/src/hid.rs`), so hidclass enumerates two collections:

| Node | Collection | Reports | Bound by |
|---|---|---|---|
| `HID\VID_046D&PID_C215&Col01` | Generic Desktop / Joystick | ID 1: X, Y, Rz, Slider (16-bit), hat, 32 buttons | inbox `input.inf` (+ `hidgamepad` on Win11), as today |
| `HID\VID_046D&PID_C215&Col02` | Vendor 0xFF00 / 0x01 | ID 2 input (raw report, seq, flags); feature ID 3 config blob (SET/GET); feature ID 4 info (GET) | inbox HID-compliant vendor device; opened by the app |

Games see one joystick with the real VID/PID. LGS's `WmFilter` matched `HID\VID_046D&PID_C215` and does not match the `&ColNN` IDs.

The app sends a profile with `HidD_SetFeature(report 3)`. The driver validates it (magic, version, size, CRC32), applies it atomically, and stores it as `REG_BINARY Config` in its subkey of the device hardware key. On the next start the driver reloads it, or falls back to identity if it is missing or invalid.

READ_REPORT policy: hidclass keeps a few reads pending per device. On every USB frame the driver completes up to two of them, first with report 1 and then with report 2. If only one read is pending it gets report 1, unless report 2 has gone unsent for 100 ms. See `DeviceCompletePendingReads` in `openx3d/device.c`.

## Layout

| Path | What |
|---|---|
| `pipeline/pipeline.{h,c}` | Pure C11 pipeline and blob validation, shared by the driver and the host test |
| `test/` | Host test: `make` runs `core/testdata/vectors.json` (falls back to `vectors.sample.json`) and checks that the descriptor copies match |
| `openx3d/` | UMDF driver: `driver.c` (entry, device add), `device.c` (state, read policy), `usb.c`, `hid.c` (IOCTLs), `registry.c`, `descriptor.h`, `openx3d.inf`, `openx3d.vcxproj`, `openx3d.rc` |
| `scripts/` | `make-cert.ps1`, `package.ps1`, `install.ps1`, `uninstall.ps1` |

## Build

Host test (Linux/macOS, gcc or clang):

```sh
make -C driver/test          # real vectors if present, else the sample
make -C driver/test sample   # sample only
```

Driver (VS 2022 + WDK 10.0.26100, e.g. the `windows-2022` GitHub runner), from the repo root in a Developer prompt:

```bat
msbuild driver\openx3d\openx3d.vcxproj /p:Configuration=Release /p:Platform=x64
infverif /v /h driver\openx3d\openx3d.inf
```

The output is `driver\openx3d\x64\Release\openx3d.dll`. The INF is not part of the project, so the build does not run StampInf/InfVerif. Validate it with `/h`. `/w` will flag the Windows 10 section (inbox `ServiceBinary=%12%`, AddReg `LowerFilters`), which cannot be avoided before build 22000 (no `MsHidUmdf.inf`). `/w /wbuild 10.0.22000` is the `/w` equivalent.

## Package and sign

```powershell
cd driver\scripts
.\make-cert.ps1 -OutDir C:\keys -PfxPassword 'pw'          # once; CN=Open X3D Pro (unsigned beta), 5 years
.\package.ps1 -BuildDir ..\openx3d\x64\Release -OutDir ..\out -PfxPath C:\keys\openx3d.pfx -PfxPassword 'pw'
```

`package.ps1` does the following:

1. Copies `openx3d.inf` and `openx3d.dll` and exports `openx3d.cer` from the PFX.
2. Signs the DLL.
3. Runs `Inf2Cat /os:10_VB_X64,10_CO_X64`. This is one OS per decorated models section; the INF has no model for plain `10_X64`.
4. Signs `openx3d.cat`.
5. Runs `signtool verify /pa`. Verification warns unless the certificate is already trusted on the build machine.

Release CI does the same with an ephemeral or secret certificate.

## Install, uninstall, rollback

Run from an elevated PowerShell in the folder with `openx3d.inf/.dll/.cat/.cer`. The release zip or the app installer does the same.

```powershell
.\install.ps1     # certutil -addstore Root + TrustedPublisher, pnputil /add-driver /install, prints the binding
.\uninstall.ps1   # pnputil /delete-driver oemNN.inf /uninstall /force, /scan-devices, removes the certificates
```

Rollback means running `uninstall.ps1`. Once the driver package is gone, PnP rebinds the stick to inbox `hidusb`/`input.inf` with no reboot. If the device is stuck, unplug and replug it, or run `pnputil /scan-devices`. Device Manager also works: Roll Back Driver, or Uninstall device with "Attempt to remove the driver" ticked.

## Go/no-go test (Windows 11 24H2/25H2, Secure Boot ON, Memory Integrity ON, TESTSIGNING off)

1. Check the baseline: `Confirm-SecureBootUEFI` returns True, `(Get-CimInstance -Namespace root\Microsoft\Windows\DeviceGuard Win32_DeviceGuard).SecurityServicesRunning` contains 2, and `bcdedit` shows no `testsigning`.
2. Run `.\install.ps1` elevated. Accepting the one-time "install this device software?" prompt is expected for a non-WHQL publisher.
3. Run `pnputil /enum-devices /instanceid "USB\VID_046D&PID_C215\<instance>" /stack /services`. The stack must show **WUDFRd** and **mshidumdf** (service `mshidumdf`) and no `HidUsb`. `install.ps1` prints the same via `DEVPKEY_Device_Stack`.
4. Check `Get-PnpDevice -PresentOnly | ? InstanceId -like 'HID\VID_046D&PID_C215*'`. It should list `...&Col01` and `...&Col02` with status OK.
5. Open `joy.cpl` and go to Properties. It should show the stick with X/Y, Z rotation, slider, POV and **32 buttons**, and all axes and buttons should move.
6. Read `setupapi.dev.log`: our INF should be selected with rank 0x00FF0000 and no signature error. Event Viewer, Microsoft-Windows-CodeIntegrity/Operational, should have no new 3076/3077/3089 events for `openx3d.dll`, `WUDFRd.sys` or `mshidumdf.sys`.
7. Push a profile from the app (or `HidD_SetFeature` report 3), unplug, replug, and confirm that feature report 4 `state` bit0 is set (config restored from the registry).
8. Run `.\uninstall.ps1`. The stick must come back on `HidUsb` and still work in `joy.cpl`.

Debugging: a Debug build logs `openx3d: ...` through `OutputDebugString`; view it with DebugView running as admin with "Capture Global Win32" enabled. Host process crashes show up as WUDFHost events in Event Viewer.

## Known limits

- x64 only. ARM64 would need WHQL or attestation signing, because Windows on ARM requires Microsoft-signed drivers.
- Nightly builds use a self-signed certificate that the installer adds to LocalMachine Root and TrustedPublisher (throwaway key per build). Tagged releases are CA-signed and only add to TrustedPublisher.
- Supported OS: Windows 10 2004–22H2 (build 19041+) and Windows 11. Earlier builds have no matching models section.
- No output reports and no force feedback (the stick has none). Feature report 4 is GET only.
- Key bindings (keyboard chords) are emitted by the app, not the driver.
