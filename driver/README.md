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

The app sends a profile with `HidD_SetFeature(report 3)`. The driver validates it (magic, version, size, CRC32, and every field in range), applies it atomically, and stores it as `REG_BINARY Config` in its subkey of the device hardware key. On the next start the driver reloads it, or falls back to identity if it is missing or invalid.

READ_REPORT policy: hidclass keeps a few reads pending per device. On every USB frame the driver completes up to two of them, first with report 1 and then with report 2. If only one read is pending it gets report 1, unless report 2 has gone unsent for 100 ms. See `DeviceCompletePendingReads` in `openx3d/device.c`.

## Layout

| Path | What |
|---|---|
| `pipeline/pipeline.{h,c}` | Pure C11 pipeline and blob validation, shared by the driver and the host test |
| `test/` | Host test: `make` runs `core/testdata/vectors.json` (falls back to `vectors.sample.json`) and checks that the descriptor copies match |
| `openx3d/` | UMDF driver: `driver.c` (entry, device add), `device.c` (state, read policy), `usb.c`, `hid.c` (IOCTLs), `registry.c`, `descriptor.h`, `openx3d.inf`, `openx3d.vcxproj`, `openx3d.rc` |
| `scripts/` | `package.ps1` (stage + Inf2Cat), `sign-catalog.ps1` (on-PC catalog signing, embedded in and run by the app installer) |

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

## Package

```powershell
cd driver\scripts
.\package.ps1 -BuildDir ..\openx3d\x64\Release -OutDir ..\out
```

`package.ps1` copies `openx3d.inf` and `openx3d.dll` and runs `Inf2Cat /os:10_VB_X64,10_CO_X64` (one OS per decorated models section; the INF has no model for plain `10_X64`). Nothing is signed: the catalog is signed only on the PC it is installed on, by `sign-catalog.ps1` with a certificate created there, whose private key it then deletes (see [README: Driver signing](../README.md#driver-signing)). CI (`.github/workflows/build.yml`, nightlies and releases) runs the same script.

## Install, uninstall, rollback

The driver is installed only by the app installer (`app/src-tauri/windows/hooks.nsh`); there is no separate driver package. The installer embeds the package and `sign-catalog.ps1` and extracts them to its own temporary folder, not the install folder. When its certificate checkbox is ticked, it runs `sign-catalog.ps1` (certificate created on this PC, catalog signed, private key deleted, then the certificate added to the machine's `Root` and `TrustedPublisher` stores) and then `pnputil /add-driver openx3d.inf /install`. Uninstalling the app removes the driver and runs `sign-catalog.ps1 -Remove`.

To try a local build by hand, from an elevated PowerShell:

```powershell
.\package.ps1 -BuildDir ..\openx3d\x64\Release -OutDir ..\out
.\sign-catalog.ps1 -Dir ..\out
pnputil /add-driver ..\out\openx3d.inf /install
```

`pnputil /delete-driver` only accepts the published `oemNN.inf` name, never `openx3d.inf`. The app installer records it in `driver-packages.txt` in the install folder at install time; its uninstaller deletes those packages (after checking `%WINDIR%\INF\oemNN.inf` is still openx3d's, since numbers are reused) and falls back to `Get-WindowsDriver` if the file is missing or a deletion fails.

Rollback means uninstalling the app (or `pnputil /delete-driver oemNN.inf /uninstall`, then `sign-catalog.ps1 -Remove`). Once the driver package is gone, PnP rebinds the stick to inbox `hidusb`/`input.inf` with no reboot. If the device is stuck, unplug and replug it, or run `pnputil /scan-devices`. Device Manager also works: Roll Back Driver, or Uninstall device with "Attempt to remove the driver" ticked.

## Go/no-go test (Windows 11 24H2/25H2, Secure Boot ON, Memory Integrity ON, TESTSIGNING off)

1. Check the baseline: `Confirm-SecureBootUEFI` returns True, `(Get-CimInstance -Namespace root\Microsoft\Windows\DeviceGuard Win32_DeviceGuard).SecurityServicesRunning` contains 2, and `bcdedit` shows no `testsigning`.
2. Run the setup exe with the certificate box ticked. No "install this device software?" prompt should appear (the local certificate is in `TrustedPublisher`). This is the decisive check that a catalog signed by a locally created, locally trusted certificate installs with Secure Boot and Memory Integrity on.
3. Run `pnputil /enum-devices /instanceid "USB\VID_046D&PID_C215\<instance>" /stack /services`. The stack must show **WUDFRd** and **mshidumdf** (service `mshidumdf`) and no `HidUsb`.
4. Check `Get-PnpDevice -PresentOnly | ? InstanceId -like 'HID\VID_046D&PID_C215*'`. It should list `...&Col01` and `...&Col02` with status OK.
5. Open `joy.cpl` and go to Properties. It should show the stick with X/Y, Z rotation, slider, POV and **32 buttons**, and all axes and buttons should move.
6. Read `setupapi.dev.log`: our INF should be selected with rank 0x00FF0000 and no signature error. Event Viewer, Microsoft-Windows-CodeIntegrity/Operational, should have no new 3076/3077/3089 events for `openx3d.dll`, `WUDFRd.sys` or `mshidumdf.sys`.
7. Push a profile from the app (or `HidD_SetFeature` report 3), unplug, replug, and confirm that feature report 4 `state` bit0 is set (config restored from the registry).
8. Uninstall the app. The stick must come back on `HidUsb` and still work in `joy.cpl`, and `Get-ChildItem Cert:\LocalMachine\Root | ? Subject -like '*Open X3D Pro*'` must print nothing.

Debugging: a Debug build logs `openx3d: ...` through `OutputDebugString`; view it with DebugView running as admin with "Capture Global Win32" enabled. Host process crashes show up as WUDFHost events in Event Viewer.

## Known limits

- x64 only. ARM64 would need WHQL or attestation signing, because Windows on ARM requires Microsoft-signed drivers.
- The catalog is signed on each PC by a certificate created there (not WHQL), and the installer and app are unsigned, so Smart App Control (when on) blocks them.
- Supported OS: Windows 10 2004–22H2 (build 19041+) and Windows 11. Earlier builds have no matching models section.
- No output reports and no force feedback (the stick has none). Feature report 4 is GET only.
- Key bindings (keyboard chords) are emitted by the app, not the driver.
- Any local user, elevated or not, can open the vendor collection (Col02) under the default HID ACL and push a config, which is stored in the device's hardware key and so applies to every user and survives reboots. This is accepted: the blob is fully validated, and the worst case is a remapped joystick. The driver cannot tell callers apart, since requests arrive through hidclass; restricting it would need a device-interface ACL on Col02.
