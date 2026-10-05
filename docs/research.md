# Extreme 3D Pro open-source driver — research findings (2026-10-03)

Raw agent output: `research/workflow-result.json`, `research/raw-results.json`.

## What the old LGS 5.10 package actually does for PID_C215

Source: `extracted/wmjoyhid.lf.inf` (DriverVer 04/27/2010 5.09.129.0, Class=HIDClass).

| Node | Install section | Effect |
|---|---|---|
| `USB\VID_046D&PID_C215` | `WmJoy.HidDevice` = `Include=Input.inf / Needs=HID_Inst.NT` | Stock inbox `hidusb.sys`. Only adds `OEMName="Logitech Extreme 3D Pro USB"` and `OEMData=03,00,00,10,0A,00,00,00` under `HKLM\SYSTEM\CCS\Control\MediaProperties\PrivateProperties\Joystick\OEM\VID_046D&PID_C215`. |
| `HID\VID_046D&PID_C215` | `WmJoy.HidFilter` + `AddService=WmFilter,0x2` | `WmFilter.sys` becomes the **function driver** of the HID collection PDO. WDM, 2010, reads `DeadBandX/Y`, `OperatingRange`, `*Hysteresis` from `Services\WmFilter\Parameters`, exposes a device interface the LGS profiler talks to. |

Signatures (checked with osslsigncode): `.sys` embedded SHA-1 Logitech/VeriSign 2009 cross-chain; `wmjoyhid.cat` WHQL SHA-1, OSAttr 2:5.00–2:6.1 (Win2000–Win7). Imports `MmMapLockedPagesSpecifyCache`, `ExAllocatePoolWithTag` (non-NX pool) → classified HVCI-incompatible → this is why LGS users must turn off Memory Integrity. Not reusable; reference only.

## Signing/loading rules that constrain the design (verified against Microsoft primary sources)

- Kernel-mode (.sys): Win10 1607+/Win11 x64 + Secure Boot loads only Microsoft-signed drivers (Partner Center attestation or WHCP/HLK). Partner Center needs an **EV code-signing cert kept valid for every submission** (~$350–360/yr; SSL.com sole-proprietor option exists but registration is organization-oriented, gov-ID verification since 2025-10) plus Entra tenant global admin, legal contact, questionnaire. Azure Trusted Signing (renamed Artifact Signing 2026-01) is **not** accepted for kernel/Partner Center. Attestation now documented "for testing purposes only", not publishable to Windows Update; Ignite 2025 signalled more kernel tightening.
- April 2026 "Windows Driver Policy" (Win11 24H2/25H2/26H1, Server 2025): default trust for cross-signed kernel drivers removed after 250 h uptime + 3 reboots evaluation. Kills the 2010 WmFilter path for good.
- **User-mode (UMDF 2) drivers**: HVCI is kernel code integrity only; KMCS does not apply. The package catalog just has to chain to a trusted root on x86/x64 (WHQL mandatory only for S-mode and ARM64). Shipping precedents: HIDMaestro (self-signed cert in LocalMachine Root + TrustedPublisher), LizardByte libvirtualhid (Azure Trusted Signing), DsHidMini (attestation, but same architecture). **Not yet empirically verified on retail Win11 24H2/25H2 with Secure Boot + HVCI on — this is the go/no-go test.**

## Architecture options

| | Kernel code | MS signing | HVCI | Verdict |
|---|---|---|---|---|
| A. Own KMDF HID filter + attestation | yes | yes (EV + Partner Center, every rebuild) | yes if built clean | Fallback only. Recurring cost, slow iteration, policy drift. |
| **B. UMDF 2 HID minidriver replacing `hidusb` as function driver of `USB\VID_046D&PID_C215` (DsHidMini model)** | no | no | yes by construction | **Recommended.** Full control of report descriptor (extra buttons, hat-as-buttons, shift layer), one tuned device with real VID/PID, nothing to hide, no feeder. |
| C. UMDF 2 upper filter on `HID\VID_046D&PID_C215` (royeldar/HIDFilterDriver pattern) | no | no | yes | Milestone 1: least code, validates the signing/install premise, delivers curves/deadzone/invert/remap within the stock 12 buttons. Cannot change descriptor. |
| D. UMDF virtual joystick + HidHide + feeder | HidHide is KMDF | HidHide signed by Nefarius | unknown | Dominated by B. |
| E. Brunner vJoy + HidHide + feeder (Joystick Gremlin stack) | yes (theirs) | theirs | unknown, unmaintained | Interim only. |
| F. Pure user-mode viewer | no | no | n/a | Phase 0: needed anyway, doubles as descriptor/report dumper. Cannot change what games see. |
| G. Repackage WmFilter.sys | — | — | no | Dead. |

Sequence: F → C (go/no-go install test on user's Win11) → B → release signing.

## App stack

History: 2026-10-03 first pass picked Tauri (no Wine on the Linux box); second pass picked Electron once CI moved to GitHub Windows runners (JS-only, bundled Chromium, WebHID). **Third pass (same day) reverts to Tauri v2** because the app's requirements changed from "config window" to a **resident agent**: start with Windows, live in the system tray, watch running/foreground processes, and push per-application profiles (curves, inversions, deadzones, remaps) to the driver when a matching executable appears.

Why the new requirements flip the decision:

| Requirement | Electron | Tauri v2 |
|---|---|---|
| Always-resident at logon, idle in tray | Chromium host stays loaded even with no window: ~40–80 MB RSS idle, ~150 MB with window; adds ~1 s to logon | Rust process ~5–15 MB idle; WebView2 created only when the window is opened, destroyed on close |
| Process / foreground-window watch | Node has no process API: spawn `tasklist`/PowerShell (slow, flickers a console unless hidden), `ps-list` ships a helper exe, or koffi FFI to `CreateToolhelp32Snapshot` | Direct Win32 from Rust: `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` → `GetWindowThreadProcessId` → `QueryFullProcessImageNameW` (event-driven, zero polling), or `sysinfo` crate for a running-process scan |
| Push profile to driver with no window open | Needs `node-hid` in the main process (WebHID needs a live renderer + user gesture once) | `hidapi` crate (`windows-native`, pure `windows-sys`) → `HidD_SetFeature` from the backend |
| Tray icon, autostart | `Tray`, `app.setLoginItemSettings` | built-in `TrayIcon`, `tauri-plugin-autostart` — equal |
| UI (curve editor, live input) | HTML/JS in Chromium | same HTML/JS in WebView2; rendering differences are negligible for sliders + one canvas |
| Installer with driver | electron-builder NSIS `customInstall` → `pnputil` | NSIS `installerHooks` → `pnputil`; `perMachine` — equal |
| Size | 80–150 MB installer | 3–10 MB |
| Dev language | JS only | JS UI + ~300–500 lines of Rust backend (HID, process watch, profile store, tray) |
| WebView2 dependency | none | inbox on Win11, present on nearly all Win10; `downloadBootstrapper` fallback |

Electron can do everything above, but the cost is a Chromium instance resident on every boot for a tool whose window is open a few minutes per month, plus a bolt-on process-enumeration dependency. The Rust backend Tauri forces on us is exactly the code that should not run in a browser anyway (Win32 event hook, HID feature reports, profile matching). Decision: **Tauri v2**.

Tauri plan:
- Backend (Rust): `hidapi` (`windows-native`) opens `HID\VID_046D&PID_C215`, reads input reports for the live view, writes the driver's vendor feature report (0xFF00:0001 collection) to apply a profile. Profile switcher: `SetWinEventHook(EVENT_SYSTEM_FOREGROUND, WINEVENT_OUTOFCONTEXT)` on the Tauri main thread (it owns a message loop), resolve PID → full image path, case-insensitive match against profile `exe` paths (full path first, basename fallback), debounce 250 ms, fall back to the default profile when no match. Optional second trigger "while process is running" via a 2 s `sysinfo` scan for games that lose foreground to overlays. Profiles: JSON files in `%APPDATA%\<app>\profiles\`, one default + per-exe; schema holds axes {calibration, invert, deadzones, curve points/exponent}, button map, shift layer. Tray: open window, active profile name, pause switching, quit. Autostart: `tauri-plugin-autostart` (HKCU Run key), per-user — the driver keeps the last config across reboot in its device registry key so the agent need not race logon.
- Frontend: plain HTML/JS, canvas curve editor with live marker, per-axis tabs, profile list with "add running app" (backend enumerates windows with titles → pick → stores exe path).
- Installer: Tauri NSIS bundler, `installMode: perMachine`, driver files as `resources`, `installerHooks` `.nsh` with `NSIS_HOOK_POSTINSTALL` → `pnputil /add-driver "$INSTDIR\driver\x3dpro.inf" /install` and `NSIS_HOOK_PREUNINSTALL` → `pnputil /delete-driver oemN.inf /uninstall` (record `oemN` at install). `webviewInstallMode: downloadBootstrapper`.
- Signing: Tauri `signCommand` hook → signtool with Artifact Signing / SignPath / OV cert; same cert signs the driver catalog (Inf2Cat) in the same CI job.
- CI: one workflow on pinned `windows-2022` (WDK 10.1.26100.4202 preinstalled) or `windows-2025-vs2026` + WDK NuGet 10.0.28000.2526: msbuild UMDF driver → Inf2Cat → signtool → `tauri-action` (NSIS) → upload artifacts. Local dev on Debian: `cargo tauri dev` runs the UI against mocked input (recorded report logs); Rust backend unit-tests the curve/deadzone math and the profile matcher without hardware.

Checks before committing (Windows host):
1. ~~HID collection opens and streams while DirectInput holds it~~ **PASSED** (Chrome WebHID, Win10, 2026-10-03). `hidapi` uses the same `hid.dll` path.
2. `HidD_SetFeature` to our UMDF driver's vendor report from a non-elevated process (ACL on the HID collection is open by default for game controllers; verify after the driver exists).
3. `SetWinEventHook` foreground switch → correct full path for elevated games (needs `PROCESS_QUERY_LIMITED_INFORMATION`; works for most, fails for protected processes — fall back to window-title match).
4. NSIS `installerHooks` runs `pnputil` elevated; exit codes 0/259/3010 handled; stick re-binds without reboot; uninstall rolls back to inbox `hidusb`.
5. ~~WebView2 present on the Win10 box~~ **confirmed**: Evergreen Runtime `pv = 154.0.4258.53` on the Win10 22H2 box (2026-10-03). Bootstrapper path still untested; low priority.
6. Idle RSS of the tray agent ≤ 20 MB with window closed (acceptance for the "resident" requirement).

Rejected: Electron (see table — fine as a config window, wrong shape for a resident agent), Deno (stale webview bindings), Wails v3 (beta), Avalonia/.NET (60–100 MB resident, no NativeAOT cross-OS), WPF (Windows-only build). Fallback if WebView2 proves a support burden: egui single-exe (same Rust backend, plainer curve editor).

Non-goals for v1: macros/keystroke emission (LGS-style), per-profile LED/FFB (device has none), cloud sync, multi-device.

## Device facts (three independent hardware-verified parsers agree)

7-byte input report, no report ID: X bits 0–9, Y bits 10–19 (0..1023, rest ≈508), hat bits 20–23 (0–7 clockwise from N, 8 = centered), Rz twist bits 24–31 (128 center), byte 4 = buttons 1–8, byte 5 = slider (0 = fully forward), byte 6 low nibble = buttons 9–12. Descriptor 122 bytes, one interrupt-IN endpoint. Two hardware revisions share VID/PID: bcdDevice 35.00 (low-speed, 10 ms poll) and 57.11 (full-speed, 1 ms poll, serial string). Linux `hid-lg.c` applies `LG_NOGET` (GET_REPORT stalls) — read interrupt pipe only. The raw 122-byte descriptor hex is published nowhere; must be dumped from a unit. Do NOT copy the EffortlessMetrics/OpenFlight crate layout (wrong).

## Probe results from user's Windows host (2026-10-03, `probe/`, `webhid-test.txt`)

Host: Windows 10 Pro 19045, UEFI, **Secure Boot ON, HVCI/VBS OFF**, no Smart App Control (Win10). LGS 5.10.127 installed same day; no Visual Studio/WDK.

Current binding with LGS (this is what we must out-rank or replace):
- `USB\VID_046D&PID_C215\9&1DBFBBF1&1&2`: service `HidUsb`, stack `\Driver\HidUsb, \Driver\USBHUB3`, INF `oem66.inf` section `WmJoy.HidDevice`, rank `0x00FF0001`, Signer Score WHQL. Hardware IDs `USB\VID_046D&PID_C215&REV_0204`, `USB\VID_046D&PID_C215`.
- `HID\VID_046D&PID_C215\A&83E3A90&0&0000`: service **`WmFilter`**, stack `\Driver\WmFilter, \Driver\HidUsb`, section `WmJoy.HidFilter`, rank `0x00FF0001`. Hardware IDs `HID\VID_046D&PID_C215&REV_0204`, `HID\VID_046D&PID_C215`, `HID\VID_046D&UP:0001_U:0004`, `HID_DEVICE_SYSTEM_GAME`; CompatibleIds empty.
- LGS installer forces binding with `UpdateDriverForPlugAndPlayDevices` (setupapi.dev.log 10:56:35). All five `Wm*.sys` show `Status: Valid` on Win10; no CodeIntegrity 3076/3077/3087/3111 events (HVCI is off, so no HVCI verdict from this box).
- Rank math for our INF: listing `HID\VID_046D&PID_C215&REV_0204` → `0x00FF0000`; `HID\VID_046D&PID_C215` → `0x00FF0001`; inbox `input.inf` matches `HID_DEVICE_SYSTEM_GAME` at index 3 → `0x00FF0003`. Same scheme at the USB node for option B. Any Authenticode-signed catalog gets signer score `00` on desktop x64, so `pnputil /add-driver /install` should select ours over inbox without forcing; keep `UpdateDriverForPlugAndPlayDevices` fallback like Logitech.

Hardware (`probe/x3dpro-usb.txt`, hub IOCTLs): **bcdDevice 0x0204** — a third revision, not the 35.00/57.11 seen online, but electrically the classic one: **USB 1.10 low-speed (1.5 Mb/s)**, bMaxPacketSize0 8, one interrupt-IN endpoint 0x81, **wMaxPacketSize 7, bInterval 10 → 10 ms, 100 Hz**, bus-powered 30 mA, HID 1.10, country 33 (US), report descriptor 122 bytes, strings "Logitech" / "Logitech Extreme 3D", no serial. Sits behind an external Realtek 0BDA:5411 USB 2.0 hub. Device descriptor `12 01 10 01 00 00 00 08 6d 04 15 c2 04 02 01 02 00 01`; config descriptor `09 02 22 00 01 01 00 80 0f 09 04 00 00 01 03 00 00 00 09 21 10 01 21 01 22 7a 00 07 05 81 03 07 00 0a`. Raw report descriptor via hub GET_DESCRIPTOR failed (err 995); parsed form from WebHID is sufficient. `WriteReportExSupported=1`, selective suspend off.

Correction (2026-10-04, Linux research): xHCI hosts round low-speed `bInterval 10` down to 8 ms, so the real rate on modern PCs is likely **125 Hz**, not 100 Hz — unmeasured; see `linux-support-2026-10-04.md` §6. Consequences: ~100–125 Hz is the ceiling — no point in sub-ms processing; a UMDF hop (~tens of µs) is negligible against a 10 ms frame. Low-speed means max 8-byte packets, so a republished descriptor on option B is free to add buttons (report stays ≤ 8 bytes to games anyway since we own it, not the wire).

WebHID in Chrome (device open while `WmFilter` was the function driver — so the HID interface survives a vendor function driver):
- `opened=true`, `reportId=0`, 7-byte input reports streaming; decode `fc 01 88 82 00 00 00` → X=508 Y=512 hat=8 Rz=130 slider=0, buttons 0. **Layout confirmed.**
- Parsed descriptor: Generic Desktop app collection usage 0x04 (Joystick); input: X 10b (0..1023), Y 10b, Hat 4b (logical 0..7, physical 0..315°, **hasNull=true**), Rz 8b, Buttons 1–8, Slider 8b, Buttons 9–12, 4-bit constant pad. No output reports. One **feature report, ID 0, 4 bytes, usage 0xFF00:0x0001** (vendor-defined).
- `receiveFeatureReport(0)` → `NotAllowedError: Failed to receive the feature report` — GET_REPORT fails on Windows too (matches Linux `LG_NOGET`). Our UMDF driver must never issue GET_REPORT to the device; the app↔driver config channel must be a feature report our minidriver *answers itself* (option B) — an upper filter (option C) could not intercept GET_FEATURE without the device stalling, so option C's config channel would need to be an IOCTL on a separate control interface instead.
- Electron/WebHID check passed: Chromium opens the joystick collection shared with DirectInput and streams reports. Electron confirmed as app stack; `node-hid` fallback not needed for input.

Probe script bugs fixed after this run: `hid-caps.txt` failed (`0xC0000000` parsed as negative Int32), `reg-oem-joystick.reg`/`reg-dinput.reg` clobbered by the log writer, `vswhere` path unguarded. Re-run later on the Win11 box.

## Win11 baseline (2026-10-03, `probe-win11/`)

Host: **Windows 11 Pro 26200 (25H2)**, UEFI, **Secure Boot ON, Memory Integrity ON** (`SecurityServicesRunning = {2}`, VBS running), **Smart App Control off** (`VerifiedAndReputablePolicyState = 0`). No LGS, no Logitech joystick drivers, no VS/WDK/git/node (CI-only build confirmed again). Edge present.

App Control policies (`citool --list-policies`): `784c4414-…` "Microsoft Windows Cross Certificates for Code Integrity Exceptions **Audit** Policy" enforced → the April‑2026 kernel trust removal is in **evaluation/audit** on this box, enforce policy `8f9cb695-…` not yet present; `d2bda982-…` "Microsoft Windows Driver Policy" (vulnerable-driver blocklist) enforced. CodeIntegrity log: no 3076/3077/3087/3111.

Clean inbox binding (what our package must out-rank):
- `USB\VID_046D&PID_C215\5&17594E6D&0&9`: `input.inf` `HID_Inst.NT` → `HidUsb`, rank **`0x00FF2005`** (device compatible-ID match; Win11 adds `USB\COMPAT_VID_046D&Class_03…` IDs). Our `USB\VID_046D&PID_C215` HWID → `0x00FF0001`.
- `HID\VID_046D&PID_C215\6&E4FD2E&0&0000`: `input.inf` `HID_Raw_Inst.NT`, rank **`0x00FF1003`** (INF compatible-ID `HID_DEVICE_SYSTEM_GAME` vs device HWID index 3). Stack **`\Driver\hidgamepad, \Driver\HidUsb`** — Win11 24H2+ binds an inbox kernel function driver **`hidgamepad.sys` ("HID Gamepad", 24 KB, manual start)** on game-controller collections; absent on Win10 (stack there was `WmFilter/HidUsb`, inbox would be bare `HidUsb`). Our `HID\VID_046D&PID_C215&REV_0204` → `0x00FF0000`.
- HID caps via `hid.dll` on Win11: `InputReportByteLength 8` (report-ID byte + 7), `FeatureReportByteLength 5` (1 + 4), 3 link collections, hat `units 0x14` (degrees), feature value caps `page FF00 usage 01 bits 8 count 4`. Raw `ReadFile` on the collection works alongside `hidgamepad`: `00 FC F1 87 80 00 02 00`.

Implications:
- Signing/HVCI premise still untested here: needs our artifact. Box is the right target (HVCI on, SAC off, TESTSIGNING off).
- `hidgamepad.sys` matters for **option C**: use the `UpperFilters` (WUDFRd) form so `hidgamepad` stays function driver; do not copy Logitech's "replace the function driver on the HID node" trick on Win11 until `hidgamepad`'s role (GameInput/WGI plumbing?) is understood. **Option B** is unaffected: hidclass still enumerates our republished collection and `input.inf`/`hidgamepad` bind on top exactly as today.
- Open: what `hidgamepad.sys` does. Cheap check on the Win11 box: `findstr /i hidgamepad C:\Windows\INF\input.inf`, `sc qc hidgamepad`, `(Get-Item C:\Windows\System32\drivers\hidgamepad.sys).VersionInfo | fl FileDescription,ProductVersion`.

## Minimal feature list (from VKB/VIRPIL/TARGET/Gremlin/DCS survey)

Pipeline: calibrate(min/center/max) → physical invert → center + end deadzones with rescale → curve → logical invert → quantize.
MUST: raw + processed view of X/Y/Rz/slider/hat/12 buttons; per-axis calibration; center deadzone; saturation; invert; one exponent/curvature knob AND 11-point piecewise-linear editor with live marker; button remap; hat-as-4-buttons; one momentary shift layer; JSON profiles with import/export; reset-to-linear; **per-application profiles auto-applied by a tray agent that starts with Windows (foreground-exe match, default profile fallback, tray shows active profile, pause toggle)** — this is what LGS's LWEMon did and what Elite/older sims lack.
SHOULD: toggle/tempo button modes, presets, copy/paste curve.
Skip: macros, encoders, logic, trimmers, mixing.

## What I do not have and need

### Blocking
1. **Windows host agent (or you at the keyboard)**: Win10 22H2 Pro (Secure Boot ON, HVCI OFF) and **Win11 25H2 Pro (Secure Boot ON, HVCI ON, SAC off)** both baselined 2026-10-03. Both remain needed for the go/no-go UMDF install test once a built package exists, then for DirectInput/RawInput/WGI visibility, latency, uninstall/rollback. Needed for driver ranking vs inbox `hidusb`, DirectInput/RawInput/WGI/SDL visibility, latency, uninstall/rollback. No KVM here (`/dev/kvm` absent, no root), so a VM is not possible on this box.
2. ~~The device on any machine I can reach~~ — **done via WebHID**: parsed descriptor + live reports captured (see Probe results). Raw 122-byte hex still useful but no longer blocking (parsed items are enough to regenerate an equivalent descriptor). bInterval/speed: **done** via `tools/probe-usb.ps1` → `probe/x3dpro-usb.txt` (low-speed, 10 ms).
3. ~~Windows info dump~~ — **done on Win10** (`probe/`). Re-run the fixed `tools/probe-x3dpro.ps1` + `tools/webhid-test.html` on the Win11 box when available. Collects OS/Secure Boot/HVCI/CI policy state, `pnputil /enum-devices /stack`, HID caps + raw reports, OEM/DirectInput/WmFilter registry, CodeIntegrity events, `setupapi.dev.log` C215 entries, WebView2 presence, installed WDK/VS. Extra one-liners if the script fails: `Confirm-SecureBootUEFI`; `Get-CimInstance -Namespace root\Microsoft\Windows\DeviceGuard -ClassName Win32_DeviceGuard | select SecurityServicesRunning`; `reg query HKLM\SYSTEM\CurrentControlSet\Control\CI\Policy /v VerifiedAndReputablePolicyState`; `citool --list-policies`; `pnputil /enum-devices /connected /deviceids /stack /drivers`.
4. **Windows build environment for driver AND app**: decided — GitHub Actions Windows runners for both (no Linux toolchain for UMDF; Tauri NSIS from Linux is possible via cargo-xwin but CI is the release path). `gh` is logged in (account `kaysond`); need go-ahead + repo name (public → free runners).
5. **Code-signing certificate for release** (not for dev): OV cert ($150–300/yr, hardware token), Azure Artifact Signing (~$9.99/mo, paid Azure sub, Individual validation US/Canada only, catalog use unverified), SignPath Foundation (free for OSI-licensed public repos, catalog signing unverified), or Certum Open Source (€49). Dev/beta can use a self-signed cert installed to LocalMachine Root + TrustedPublisher on your own machine.

### Not blocking
6. EV cert + Partner Center account (~$360/yr + weeks) — only for option A fallback, ARM64, or to remove the "Would you like to install this device software?" prompt by attestation-signing the UMDF package.
7. Debian packages — mostly moot now that CI builds everything. Still useful locally (no root, `apt download` + `dpkg -x`): `osslsigncode` (verify signed artifacts), `vim-common` (xxd), `usbutils` (if the stick is ever plugged in here). Root-only and not planned: `wine`, `qemu-system-x86` + kvm group.
8. Both hardware revisions (35.00 and 57.11) and a Win10 22H2 test box.
9. Hardware token or cloud HSM for whichever cert is chosen.
10. Decisions from you: target games/anti-cheat posture (in-stack design keeps real VID/PID), yearly signing budget, acceptable beta UX (self-signed root? SmartScreen "Run anyway"?), whether option B's extra buttons justify more code than option C.
11. Network egress from the sandbox to `github.com`, `api.nuget.org`, `crates.io`, `registry.npmjs.org`, `deb.debian.org` (all answered this session), and authenticated GitHub code search for `WmFilter`/`WmHidLo`/`DeadBandX` notes.

## Unresolved questions (ordered by impact)
1. GO/NO-GO: UMDF-only package with non-Microsoft catalog signature installs silently on retail Win11 24H2/25H2 with Secure Boot + HVCI + default Smart App Control? (docs + 3 shipping projects say yes; untested here)
2. Does our INF out-rank inbox `input.inf`/`hidusb` after `pnputil /add-driver /install`, and survive Windows Update driver offers? (`setupapi.dev.log` Rank / Signer Score)
3. Does Artifact Signing / SignPath sign an Inf2Cat catalog acceptably?
4. Is the 57.11 descriptor identical to 35.00; what is the "Undefined"-usage feature report; does GET_REPORT stall on Windows too?
5. Does joy.cpl/DirectInput need `IOCTL_HID_GET_INPUT_REPORT` in addition to `IOCTL_HID_READ_REPORT` (implement both, as vhidmini2 does)?
5b. Does `HidD_SetFeature` from a non-elevated Tauri backend reach our UMDF driver's vendor report, and does `SetWinEventHook` resolve full exe paths for elevated games (see App stack checks)?
6. Win10 1809–22H2: Include/Needs `MsHidUmdf.inf`+`WUDFRD.inf` form vs DsHidMini's legacy AddService branches.
7. UMDF per-report latency under load at 1 ms poll (REV 57.11).
8. Attestation-signed kernel drivers' fate under the April-2026 policy (only matters for option A/E).
