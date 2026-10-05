# Linux support: evaluation and plan

Date: 2026-10-04. This is a decision report; no code was written. It combines the six research files in [`research/linux/`](../research/linux/) and checks them against the Windows contract ([`CONTRACT.md`](CONTRACT.md), cited as `[C §n]`).

- Research citations look like `[architecture.md §2.3]`.
- **Unverified** marks anything no researcher confirmed in a primary source or on hardware.
- Nobody has plugged the stick into a Linux machine yet. Every device behaviour below therefore comes from kernel, SDL and Wine source, not from captures [device.md, status line].

## 1. Verdict in 10 lines

1. **Add Linux support, built on HID-BPF (option A).** A struct_ops program on the real 046d:c215 rewrites the descriptor, the input reports and the feature requests inside hid-core. Every consumer sees one processed device, root is needed only at install and plug time, and no process sits in the input path. This is the Windows UMDF design, on Linux.
2. **Not B first.** An evdev grab plus a uinput clone leaves a dead twin that Proton games (the core audience) list, and only a permanently privileged service can hide it. B also can never offer the Windows layout through Proton's hidraw path. B is the cheaper degraded mode and becomes the phase-2 fallback; C (uhid root daemon) is never built.
3. **Reach of A.** Yes: Fedora 43/44, Arch, Debian sid/forky and trixie-backports, SteamOS 3.7/3.8. No: Debian 13 stock, Debian 12, Ubuntu 22.04, Ubuntu 24.04 GA kernel. Unverified: Ubuntu 24.04 HWE and 26.04. On kernels without HID-BPF, v1 is read-only: live view, calibration and curve editing work, games are unaffected.
4. **Same app layer for every backend.** A headless `open-x3d-pro --daemon` (systemd user unit), with the Tauri UI as a client over a Unix socket. Profile JSON and §5 IPC are unchanged. `running` is the default match mode, with Proton matched via `cmdline[0]`. Key chords go through a uinput keyboard. Packages: deb, rpm, AppImage. No Flatpak, no Snap, no tray.
5. **Pipeline.** Port it to exact-fraction u64 C, shared by the BPF program and the host test. The prototype passes 360/360 vectors and survives fuzzing. Q16.16 cannot stay within ±1.
6. **Effort (person-days, optimistic/likely).** Phase 0 is 3.5/6.5 and phase 1 is 18/33.5, so **v1 = 21.5/40**. Phase 2 (B fallback, BPF VM CI) adds 5.25/10.25. A B-first v1 would cost 16/29.75.
7. **Triggers that change the plan.** Switch to B first if the spike shows the 2×7-byte report split fails, or if the `hid_hw_request` intercept doesn't work on current kernels. Pull B into v1 if Ubuntu 24.04 HWE and 26.04 both lack `CONFIG_HID_BPF`.
8. **The owner provides** a Linux machine with the stick and sudo (the dev box has neither root nor KVM [research.md "What I do not have"]), a HID-BPF kernel on it (Debian 13 → trixie-backports 7.2.6, or a Fedora 44 live USB), one Proton flight sim plus a DirectInput viewer, and about half a day for the three decisive checks (§7).
9. **Optional hardware:** a Steam Deck on SteamOS 3.8, an NVIDIA desktop, an Ubuntu 24.04 HWE or 26.04 install.
10. **Owner decisions:** accept read-only on non-BPF kernels for v1; accept uinput `uaccess`, which lets any process of the active user inject keys (Steam's rule already does the same); accept a one-time rebind, because the device is renamed "Logitech Extreme 3D Pro (Open X3D)" as on Windows.

## 2. Requirements on Linux (derived from the Windows contract)

| # | Requirement | From | A | B | C |
|---|---|---|---|---|---|
| R1 | Pipeline output matches `core/testdata/vectors.json`: axes ±1 LSB, hat/buttons exact | [C §4.1, §4.3] | ✔ u64 port, validated | ✔ float reference itself | ✔ |
| R2 | Games see **exactly one** device, VID/PID 046d:c215 (SDL's flight-stick list and Wine key on it), named as on Windows | [C §2, §2.1]; [architecture.md §2.4] | ✔ | ✘ dead twin unless root | ✘ unless root hides raw; needs `BUS_VIRTUAL` |
| R3 | Same profile/settings JSON and §5 commands/events; frontend unchanged | [C §3, §5, §7] | ✔ | ✔ | ✔ |
| R4 | Per-app switching incl. Proton/Wine, default fallback, pause, manual override | [C §5, §6] | shared | shared | shared |
| R5 | Key chords on physical buttons, layout-independent like scan codes | [C §3 `KeyBinding`, §6] | shared | shared | shared |
| R6 | No root at runtime beyond plug time; root only to install | brief; [C header] spirit | ✔ | ✔ (✘ if hiding) | ✘ |
| R7 | No kernel modules; Secure Boot and lockdown fine | [C header] HVCI rule, Linux analogue | ✔ | ✔ | ✔ |
| R8 | Never send GET_REPORT/GET_FEATURE to the device | [C header] | ✔ BPF answers all | ✔ | ✔ |
| R9 | Works on Steam Deck desktop mode (Game Mode a bonus) | brief | ✔ | ✔ | ✔ |
| R10 | Processed output survives the app dying, as the Windows driver keeps applying | [C §2 persistence, §6] | ✔ | ✘ falls back to raw | ✘ |
| R11 | Windows DirectInput layout reachable under Proton | [C §2.1] usages | ✔ opt-in hidraw (unverified) | ✘ | ✔ |

**Accepted deviations from Windows:**
- **No config before login.** The BPF program starts at identity. The daemon pushes the active profile when the hidraw node appears [architecture.md §2.6; code-reuse.md §2.4].
- **Default `matchMode` is `running`** (§4.3).
- **No tray.**

## 3. Architecture decision

### 3.1 Options compared

| Criterion | **A: HID-BPF (struct_ops)** | **B: evdev grab + uinput, user daemon** | **C: uhid system daemon** |
|---|---|---|---|
| Kernel floor | ≥ 6.11 + `CONFIG_HID_BPF=y` + BTF. The real floor may be later: `hid_hw_request` first failed to load (CFI-stub bug), and an overflow fix `2b658c1c442e` landed 2026-03-16 [code-reuse.md §2.5, §6.3; device.md §1] | ≥ 4.5 (uinput 0.5) [architecture.md §3.1] | any with uhid [architecture.md §5.1] |
| Distro reach (checked 2026-10-04) | **Yes:** Fedora 43/44 (kernel-ark `=y`), Arch 7.2.9 `=y`, Debian sid/forky (`=y` since 6.16.3-1), trixie-backports 7.2.6, SteamOS 3.7 (6.11.11) and 3.8 (6.18.50, plus `DEBUG_INFO_BTF=y`). **No:** Debian 13 stock 6.12 (measured on the dev box), Debian 12 (6.1), Ubuntu 22.04 (5.15), Ubuntu 24.04 GA (6.8 < 6.11). **Unverified:** Ubuntu 24.04 HWE 6.17 and 26.04/26.10 (indirect: CVE-2025-38016 fix, udev-hid-bpf packaged), RHEL family [architecture.md §2.8; distribution.md §5; device.md §1] | all | all |
| Device identity | The real device: bus USB, 046d:c215, `id.version` 0x0110. The name is writable (`hid_set_name`); VID/PID are not. hid-lg stays bound, because the fixup runs before driver matching (SpaceNavigator precedent) [architecture.md §2.4; device.md §1–2] | The clone copies bus/VID/PID/version/name verbatim; it has no hidraw node [architecture.md §3.1] | Must use `BUS_VIRTUAL`: with bus USB plus c215, hid-lg rejects the non-USB transport and hid-generic defers, so the device stays unbound. SDL GUID changes [architecture.md §5.1] |
| Duplicates | None by construction: hidraw, evdev and joydev all see post-BPF data [architecture.md §2.1] | Dead raw twin wherever nodes are enumerated. Hiding it needs a permanent privileged service [architecture.md §3.2] | as B |
| Privileges | Install: rules, hwdb, loader. Plug time: udev runs the loader as root, which pins the link and exits. Runtime: none; the app only needs hidraw and uinput `uaccess` [architecture.md §2.7] | Install: one uinput rule. Runtime: none [distribution.md §1]. The twin stays | Permanent root/dedicated-user service, `/dev/uhid` (root-only), hide rules, IPC auth [architecture.md §5.1] |
| Config channel | hidraw `HIDIOCSFEATURE` 3 / `HIDIOCGFEATURE` 3,4, answered in `hid_hw_request`. Bytes and hidapi code are the same as on Windows [architecture.md §2.6; code-reuse.md §2.4] | in-process `switcher.compiled()` [code-reuse.md §3.3] | `UHID_SET/GET_REPORT` → daemon; the app side is as on Windows |
| Raw passthrough for the app | The in-place rewrite removes raw for every reader, so the app polls feature 4 `raw_last[7]` [architecture.md §2.3] | The daemon has exact raw values (after `EVIOCSABS` fuzz 0) [code-reuse.md §3.2] | the daemon has raw |
| Output layout vs Windows | Same usages, split into two 7-byte input reports: ID 1 = X, Y u16, Rz u12, hat; ID 5 = slider u16, 32 buttons. Features 3/4 identical [architecture.md §2.3] | 16-bit axes, 32 buttons, hat, fuzz/flat 0 [code-reuse.md §3.3] | Windows descriptor verbatim |
| Pipeline port | **Integer required.** Exact-fraction u64: two unsigned divisions per axis, intermediates ≤ 2⁴⁴. Passes `driver/test` 360/360; across 51.2 M fuzz outputs, 1 647 are off by exactly 1 (.5 ties). **Q16.16 fails ±1**: the deadzone rescale amplifies up to 1000×, and the LUT slope is ~16 LSB per quantum [code-reuse.md §0.3, §2] | none (`x3d_core::pipeline`) | none |
| Latency | inline in URB completion, effectively 0 (estimate) [architecture.md §2.9] | 20–100 µs (estimate) [code-reuse.md §3.3] | as B |
| Failure mode | Loader fails → stock stick. Daemon dies → last config stays applied | Daemon dies → grab released, clone gone, games see raw [code-reuse.md §3.4] | Dies with raw hidden → no stick until rules are reverted [architecture.md §6] |
| Dev/debug loop | Root to attach; verifier errors; `bpf_printk` via root's `trace_pipe` (lost under lockdown=confidentiality); native-C build tests the math [architecture.md §2.7, §2.9] | plain userspace | userspace + root service |
| CI testability | Stock runner: host vector test, clang compile, uhid fake *processed* device for the app path. Kernel behaviour only in a virtme-ng VM (KVM on public runners) [code-reuse.md §6.7; distribution.md §4.4] | Full e2e on the stock runner (uinput or uhid fake source) [code-reuse.md §6.7; distribution.md §4.3] | full e2e on the stock runner |
| Effort, backend only (opt/likely pd) | 11.75 / 22.5 [code-reuse.md §5.1] | 3.75 / 7.25 | not estimated; > B [architecture.md §6] |
| Long-term maintenance | ABI changed once (6.11); kernel fixes still landing. One rdesc fixup per device, so a distro-shipped c215 fixup would collide [distribution.md §3.1]. Novel: no HID-BPF curve prior art [prior-art.md §2] | Stable ABIs; twin bug reports forever; suspend/resume unverified | B + privileged surface |

### 3.2 Duplicate devices, per consumer

| Consumer | A | B | C |
|---|---|---|---|
| Native SDL2/3, evdev, host | 1 device | Clone + dead twin. Host-only mitigation: a static rule stripping `ID_INPUT_JOYSTICK` from the physical node [code-reuse.md §4.4] | twin unless root hides it |
| SDL inside pressure-vessel (Proton's default SDL path, Steam Runtime native games) | 1 device; positional axes X, Y, Z, Rx, as the stock stick today | Clone + dead twin. Property stripping does **not** help: containerised SDL probes nodes directly [architecture.md §3.1–3.2], and Proton runs inside pressure-vessel [app-stack.md §3.1–3.2] | twin unless root chmod-hides it |
| SDL HIDAPI | n/a: never claims c215 [architecture.md §4.2] | n/a | n/a |
| joydev (FlightGear) | 1 device | raw `js*` silent but listed, plus the clone's `js*` | as B |
| Upstream Wine evdev path | 1 device; Rz → Rz, throttle → Slider | twin | twin |
| Proton hidraw path (opt-in `PROTON_ENABLE_HIDRAW=0x046D/0xC215` + our hidraw rule) | 1 device, **Windows layout**; SDL twin dropped (unverified) | **Broken**: Proton's VID/PID dedupe keeps the raw hidraw device and drops the clone [architecture.md §3.2] | Windows layout if raw hidraw is hidden |
| Steam Input | Cannot open c215 hidraw on stock systems. "Generic gamepad support" putting c215 into `SDL_GAMECONTROLLER_IGNORE_DEVICES` is unverified, and would hit A and B alike [architecture.md §4.3] | same | same |

### 3.3 Decision: A, behind phase 0 and a hardware spike; B (not C) as the fallback

**Why A over B's cheaper path:**
- **Only A meets R2 and R6 together.** B's twin is structural for Proton, and Proton is the core audience: DCS, IL-2, Elite and MSFS are Windows-only [architecture.md §4.1]. Hiding the twin needs permanent root, because games run as the same user as any user daemon [architecture.md §3.2]. Prior art shows the twin in the field: input-remapper #1045, the xboxdrv man page ("may still appear in device lists"), ControllerBuddy [prior-art.md §1a, §5].
- **It keeps the project's Windows choice.** On Windows the project rejected a virtual joystick plus HidHide (options D/E, "dominated by B") in favour of an in-stack driver that keeps the real VID/PID, which also matters for anti-cheat [research.md options table, L138; C §2]. A is that design; B is the D/E shape.
- **Robustness (R10).** No process sits in the frame path. Config survives daemon or WebKitGTK crashes, as on Windows.
- **Only A or C can put the Windows layout under Proton (R11).** The hidraw path is the one way around positional axis slotting (Wine bug 55653, Proton #7121). Unverified (check 18).
- **The cost is bounded and de-risked up front.** A adds +5.5/+10.25 pd to v1 (§3.5). Both design-killing unknowns, URB framing and the feature intercept, are settled by a 1–2 pd spike before any A code is written.
- **The reach gap has an escape hatch.** Non-BPF kernels get read-only mode, which is phase-0 behaviour, needs no extra rule and is still useful for calibration. Debian 13 users can install the trixie-backports kernel [architecture.md §7].

**Answers to the dissent:**
- **code-reuse.md §5.2** builds A "only if" B's twin or the winebus-hidraw bypass hurts in practice. The evidence in hand shows both are structural, not hypothetical: pressure-vessel probes nodes directly, and Proton's hidraw dedupe drops the clone.
- **distribution.md §6:** "the product needs (U) anyway". That is true only if non-BPF reach matters for v1, which criterion S3 decides.
- **app-stack.md §7:** its daemon/client split carries over unchanged; only the device session differs (§4.2).

**Kernels without HID-BPF.**
- v1: read-only, with a banner naming the per-distro fix.
- Phase 2: B, running as the user, with its twin documented.
- **Never C.** C's only gain over B is the opt-in Proton hidraw layout. In exchange it needs a permanent privileged service plus hide rules (fails R6), and `BUS_VIRTUAL` changes the SDL GUID. Without hiding, it has B's twin anyway [architecture.md §5.1]. architecture.md §7 prefers C; it is overruled on R6.

**Switch criteria** (checks are in §7):
- **S1: report layout fails.** Check 9 shows a single 14-byte report arriving glued at half rate, **and** check 10's 2×7 split does not deliver both reports every poll → build B first. If check 9 shows no gluing, keep the Windows 14-byte ID 1 unchanged.
- **S2: config intercept fails.** Check 11's `hid_hw_request` intercept does not load or work on current stable kernels. The config channel and GET protection would be lost, and the only replacement is a root map writer (C-like) → build B first.
- **S3: Ubuntu lacks HID-BPF.** Check 1 shows both Ubuntu 24.04 HWE and 26.04 without `CONFIG_HID_BPF` → keep A and add B to v1 (+3.75/+7.25).

### 3.4 Disagreements and rulings

| # | Topic | Positions | Ruling and reason |
|---|---|---|---|
| 1 | Backend order | architecture.md, prior-art.md: A + fallback. code-reuse.md: B in-process, A later. distribution.md: uinput base, HID-BPF an "optional fast path". app-stack.md: B-shaped daemon | **A** (§3.3) |
| 2 | Integer math | architecture.md §2.5/§7: "Q16 / u64", "Q16". code-reuse.md §2.1: Q16.16 breaks ±1 | **u64 exact fraction**, prototyped and fuzzed (§3.1). Change C §4.1 (§5) |
| 3 | Input report shape | architecture.md §2.3: every input report ≤ 7 B (wMaxPacketSize), two per event via `hid_bpf_try_input_report`. code-reuse.md §2.5: one 24 B report with a raw tail; injection from `hid_device_event` "unproven" | **2 × 7 B**, pending checks 9–10. architecture.md cites the kfunc source ("can be safely used in IRQ context") and the selftest `hid_test_multiply_events`, which does exactly this. A 24 B report runs into usbhid sizing the URB from the fixed descriptor with no clamp, which code-reuse.md did not analyse. Raw comes from feature 4 instead |
| 4 | Loader model | architecture.md §2.7: plug-time udev loader, pinned link. distribution.md §1.4/§3.2: hardened system service holding fds, root config socket. prior-art.md §2: daemon writes pinned maps | **Plug-time loader, our own binary.** Config travels in hidraw feature reports, so no runtime writer needs hosting, and bpffs is 0700, so there are no user pins [code-reuse.md §0.4]. Kept from distribution.md: our own libbpf-rs loader, since udev-hid-bpf is missing on Ubuntu 24.04, Debian 13 and SteamOS and its paths differ per distro |
| 5 | Fallback | architecture.md: C. Others: uinput | **B** (§3.3) |
| 6 | Process model | code-reuse.md §3.4: in-process, split later (+3/+5). app-stack.md §7: split now | **Split** (§4.1). No tray on GNOME means the resident agent cannot be the UI. Deck Game Mode has no XDG autostart. WebKit (100+ MB) should not stay resident |
| 7 | hidraw rule | distribution.md §1.3: ship it. code-reuse.md §3.2: B needs raw hidraw root-only, or winebus bypasses the grab | **Ship it**: A requires it. On the B fallback the bypass needs explicit opt-in, since c215 is on no Wine allowlist [architecture.md §4.1]. Document it |
| 8 | Profile dir | code-reuse.md §1: `~/.local/share/OpenX3DPro`, no change. app-stack.md §7: `$XDG_CONFIG_HOME/OpenX3DPro` | **`$XDG_CONFIG_HOME/OpenX3DPro`**: profiles are config. One cfg-gated line |
| 9 | Bundle host | distribution.md §4.1: `ubuntu-22.04` runner. app-stack.md §1.5: `ubuntu:22.04` container on `ubuntu-24.04` | **Container**: same glibc 2.35 floor, and it survives the runner's retirement |
| 10 | User-unit target | app-stack.md §1.4: `graphical-session.target`. distribution.md §1.4: `default.target` | **`default.target`**, connecting to X lazily and only in foreground mode. Whether Game Mode reaches `graphical-session.target` is unverified (check 26) |
| 11 | uhid fake, bus USB 046d:c215 | distribution.md §4.3, code-reuse.md §6.7: it binds | **It stays unbound** [architecture.md §5.1; consistent with device.md §1]. Use `BUS_VIRTUAL`, or a VM kernel with `CONFIG_HID_LOGITECH=n` |
| 12 | Device name | code-reuse.md §3.3: keep the physical name (same SDL GUID). architecture.md §7, prior-art.md §6: Windows name | **"Logitech Extreme 3D Pro (Open X3D)"** [C §2]: consistent across OSes, distinguishes processed from stock (and from B's twin). Cost: one rebind, because the SDL GUID hashes the name [architecture.md §3.1] |
| 13 | evdev `id.version` | code-reuse.md §3.3: bcdDevice. device.md §2: bcdHID 0x0110 on all revisions | **device.md** (traced through `usbhid_parse`). The B clone copies `input_id()` from the node |
| 14 | hwdb vs joydev | architecture.md §2.4: hwdb fuzz/flat 0 fixes it | **evdev only.** joydev computes its correction at registration, before udev [device.md §3]. Under A, `js*` keeps a 12.5 % centre band on the u16 axes, throttle included: the same relative size as stock (derived; check 12) |
| 15 | Proton SDL-path axes | architecture.md §4.1: positional X, Y, Z, Rx (Proton #7121). device.md §5: Wine bug 55653, twist → Z, throttle → Rz | Twist → Z agreed; throttle slot disputed (Wine version or backend?). Check 17 |
| 16 | SteamOS 3.8 `HID_BPF` | distribution.md §5: "verify" | **Verified `=y`** for 3.7 and 3.8 in the linux-neptune configs [architecture.md §2.8] |
| 17 | `SDL_GAMECONTROLLER_IGNORE_DEVICES` scope | prior-art.md §3: gamepad API only. architecture.md §3.2: all joysticks (SDL3) | architecture.md for SDL3 (cites source lines); SDL2 may differ. Matters only for check 19 |

### 3.5 Effort reconciliation

- **code-reuse.md §5.1:** A path 19.5/37.25; B path 11.5/22; A plus a mandatory B 23.25/44.5; in-process → daemon split +3/+5.
- **app-stack.md §7:** ≈ 21 pd plus 3–4 pd of desktop testing, for a B-shaped app that is already split. That equals code-reuse.md's B likely plus the split (22 + 5 = 27), within 2–3 pd. app-stack.md lacks the uinput e2e CI test (B3, 1/2) and a separate hardware line (B4, partly its testing days). It adds `steam:<appid>` (0.75), systemd autostart (0.75) and a bigger frontend pass (1.5 vs 0.5). Its single-point figures track code-reuse.md's "likely" column.
- **This report** = code-reuse.md lines + split (3/5) + `steam:` entries (0.5/0.75) + a stock-runner uhid e2e lane (0.5/1; from distribution.md §4.3, since code-reuse.md has no non-VM test for A) − the VM lane (2/4, moved to phase 2).
- **Result:** A v1 = 21.5/40; B-first v1 = 16/29.75. Building both backends costs about 27/50 in either order.

## 4. Shared design, regardless of backend

### 4.1 Process model and IPC

- **Two processes, one crate** [app-stack.md §7].
  - **`open-x3d-pro --daemon`** (systemd user unit; never initialises GTK/WebKit) owns the device session (§4.2), the profile and settings store, switcher and watcher, calibration, the uinput keyboard and `AppState`.
  - **The Tauri UI** forwards each §5 command over the socket and re-emits the socket's events with `app.emit`. The frontend and `ipc.ts` are unchanged.
  - **Phase 0 stays single-process**, since it is read-only.
- **Socket:** `$XDG_RUNTIME_DIR/open-x3d-pro.sock`, newline-delimited JSON. Requests are `{id, cmd, args}`, answered with `{id, ok}` or `{id, err}`; events `{event, payload}` are pushed to subscribed connections. Stdlib `UnixListener` plus serde_json. The runtime directory is mode 0700, which limits the socket to the same user, and binding it makes the daemon single-instance [app-stack.md §1.4, §7].
- **No D-Bus/zbus.** prior-art.md §4 suggests it; it adds a dependency for no gain on a same-user channel.

### 4.2 Device session in the daemon

- **Backend selection.** `enum Backend { Bpf(hidraw), Uinput{src, out}, ReadOnly(evdev) }`, chosen at connect time in that order, with no trait [code-reuse.md §5.3]. Phase 1 builds `Bpf` and `ReadOnly`; phase 2 adds `Uinput`.
- **Detect A from the descriptor**, never by probing. Compare the hidraw report descriptor (sysfs `report_descriptor`, or hidapi `get_report_descriptor`) with `x3d_core::hid::REPORT_DESCRIPTOR_LINUX`.
  - **Never send GET_FEATURE to a stock device.** It hangs for about 5 s or returns EPIPE, and NOGET does not stop it on Linux [code-reuse.md §2.4; device.md §6].
- **`Bpf` backend.**
  - `hidapi` with `linux-native-basic-udev`, in its own Linux target table. It is pure Rust, like the Windows `windows-native` choice; per-top-level-collection enumeration and `get_report_descriptor` are unverified [code-reuse.md §6.1].
  - Push with `send_feature_report(3)` when the CRC changes (`take_push`, reused) and whenever the hidraw node appears. Info comes from `get_feature_report(4)`.
  - Processed values and `seq` come from input reports ID 1 and ID 5. Raw comes from feature 4 `raw_last`, polled at ≤ 60 Hz; the BPF program answers it, so it never reaches the wire.
- **`ReadOnly` backend.** Evdev `ABS_X/Y/RZ/THROTTLE`, `ABS_HAT0X/Y` (inverse of the hid-input table, (0,0) → 8) and `BTN_TRIGGER..BTN_BASE6` go through `RawReport::to_bytes()` → `x3d_core::pipeline::process` [code-reuse.md §3.2; device.md §2]. Values are exact with our hwdb (§4.6); without it, X/Y lose 1–2 LSB steps to fuzz 3 [device.md §3].
- **State.** `driverInstalled` means "games see processed output": true for `Bpf`/`Uinput`, false for `ReadOnly`. `driverVersion` comes from feature 4 [code-reuse.md §1].
- **Reporting is event-driven.** The stick reports only on change (`SET_IDLE(0)`) [device.md §2], so the coalescer must not assume a steady rate.

### 4.3 Profile matching

- **Default on Linux: `matchMode: 'running'`.** It works in every session type, gamescope included, and sims run for hours [app-stack.md §2].
- **Process scan:** `/proc` every 2 s, own uid only.
  1. Prefilter on `/proc/<pid>/comm`. Wine sets it to the exe basename, truncated to 15 bytes.
  2. Then `name = cmdline[0]`. Wine rewrites it to the Windows path, `C:\…\DCS.exe` or `Z:\home\…`.
  3. Otherwise `readlink /proc/<pid>/exe`. Never use this for Wine: it points at the preloader [app-stack.md §3.1, §3.3].
- **Matching:** `name` goes to the **unchanged** `switcher::match_profile`. Its normalise step plus basename fallback lets a profile with `DCS.exe` match on both OSes. Generic Linux basenames (`java`, `python3`) over-match, so use full paths for those [code-reuse.md §1].
- **`steam:<appid>` in `exePaths`** matches `SteamAppId`, `SteamGameId` or `STEAM_COMPAT_APP_ID` in `/proc/<pid>/environ`. In foreground mode it also matches `GAMESCOPE_FOCUSED_APP` or `WM_CLASS steam_app_<id>`. It never matches on Windows, so no §3 type change [app-stack.md §3.3].
- **`foreground` mode:** an x11rb watcher on `$DISPLAY` listens for root `PropertyNotify` on `_NET_ACTIVE_WINDOW`. It gets the PID via XRes `QueryClientIds` (fallback `_NET_WM_PID`) and feeds the existing 250 ms debounce.
  - It covers X11 sessions and XWayland windows on Mutter, KWin, wlroots and gamescope. All four keep the atom accurate, with a dummy id when a Wayland-native window has focus (verified in source), and Proton is XWayland by default.
  - Native-Wayland windows get the default profile, and the UI says so [app-stack.md §2]. Connect only while in this mode, because GNOME starts XWayland on demand.
- **`list_windows`** = `_NET_CLIENT_LIST` + `_NET_WM_PID` + `_NET_WM_NAME`, plus own-uid Wine processes from `/proc` [app-stack.md §7; code-reuse.md §4.1].
- **Flatpak Steam** games are visible to a native daemon. `_NET_WM_PID` may be namespaced (unverified), hence XRes [app-stack.md §3.2].
- **Deferred:** KWin script, GNOME extension, wlroots/Hyprland IPC, GameMode D-Bus [app-stack.md §2; prior-art.md §4].

### 4.4 Key chords

- **Device:** one uinput keyboard, created lazily on the first `send()`, separate from any joystick. It works on X11, Wayland and gamescope [app-stack.md §4; code-reuse.md §4.2].
- **`keys.rs` change:** add a Linux `KEY_*` column. 85 of the 117 entries match the set-1 code. The other 32 differ: Pause (Linux 69 is `KEY_NUMLOCK`), F13–F24, and the 19 `0xE0xx` keys. `Chords` is unchanged.
- **Permission:** the uinput rule (§4.6), needed under every backend.

### 4.5 UI lifecycle, autostart, paths

- **UI:** a normal app-grid window. Closing it quits only the UI. `tauri-plugin-single-instance` handles it via D-Bus.
- **No tray in v1.** GNOME needs an extension to show one, Tauri gets no tray click events on Linux, and tooltips are unsupported [app-stack.md §1.3].
- **Autostart:** `Settings.autostart` runs `systemctl --user enable|disable --now open-x3d-pro.service`. Drop `tauri-plugin-autostart` on Linux, because XDG autostart does not run in Game Mode [app-stack.md §1.4].
- **Unit file:** `/usr/lib/systemd/user/open-x3d-pro.service`. The AppImage writes its own copy to `~/.config/systemd/user/` [distribution.md §1.4]:

```ini
[Unit]
Description=Open X3D Pro agent
[Service]
ExecStart=/usr/bin/open-x3d-pro --daemon
Restart=on-failure
NoNewPrivileges=yes
LockPersonality=yes
RestrictAddressFamilies=AF_UNIX AF_NETLINK
[Install]
WantedBy=default.target
```

| What | Path |
|---|---|
| Profiles, settings | `$XDG_CONFIG_HOME/OpenX3DPro/{profiles/,settings.json}` |
| Socket / logs | `$XDG_RUNTIME_DIR/open-x3d-pro.sock` / `journalctl --user -u open-x3d-pro` |
| Rule, hwdb (deb/rpm) | `/usr/lib/udev/rules.d/70-open-x3d-pro.rules`, `/usr/lib/udev/hwdb.d/61-open-x3d-pro.hwdb` (AppImage/SteamOS: same names under `/etc/udev/`) |
| BPF loader (embeds `openx3d.bpf.o`) | `/usr/libexec/open-x3d-pro/x3d-bpf-load` (deb/rpm); `/var/lib/open-x3d-pro/x3d-bpf-load` (AppImage/SteamOS; whether it persists across SteamOS updates is unverified) |

### 4.6 Permissions: udev rule, hwdb, loader

The rule file must sort before `73-seat-late.rules`, where the `uaccess` builtin runs [distribution.md §1.1]. Defaults today: joystick evdev is already `uaccess` [code-reuse.md §0.5]; hidraw is root 0600 [device.md §6]; `/dev/uinput` is root-only unless Steam's rule is installed [app-stack.md §5]. Lint the rule with `udevadm verify`.

```udev
# 70-open-x3d-pro.rules: Logitech Extreme 3D Pro (046d:c215)
# uinput: key chords (all backends), B fallback joystick. Same line as Valve's steam-devices.
KERNEL=="uinput", SUBSYSTEM=="misc", TAG+="uaccess", OPTIONS+="static_node=uinput"
# hidraw: A's config channel (feature reports 3/4, answered by BPF, never sent to the device).
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="046d", ATTRS{idProduct}=="c215", TAG+="uaccess"
# A: attach HID-BPF at plug, unpin at unplug. HID_ID string: verify (check 8).
SUBSYSTEM=="hid", ACTION=="add", ENV{HID_ID}=="0003:0000046D:0000C215", RUN+="/usr/libexec/open-x3d-pro/x3d-bpf-load add $sys$devpath"
SUBSYSTEM=="hid", ACTION=="remove", ENV{HID_ID}=="0003:0000046D:0000C215", RUN+="/usr/libexec/open-x3d-pro/x3d-bpf-load remove $sys$devpath"
```

```
# 61-open-x3d-pro.hwdb: zero hid-input's fuzz/flat on X, Y, Rz, Throttle (stock and BPF device).
# joydev is unaffected: it computes its correction before udev runs.
evdev:input:b0003v046DpC215*
 EVDEV_ABS_00=:::0:0
 EVDEV_ABS_01=:::0:0
 EVDEV_ABS_05=:::0:0
 EVDEV_ABS_06=:::0:0
```

**The loader:**
- **Build:** Rust, libbpf-rs, with libbpf vendored and statically linked.
- **At plug**, running as root from the udev event, it sets `hid_id`, calls `bpf_map__attach_struct_ops()`, pins the link (udev kills `RUN` children when the event ends) and exits [architecture.md §2.7; distribution.md §1.1, §3.2]. **At unplug**, `remove` unpins; the maps die with the device [architecture.md §2.6].
- **Dev loop:** `sudo udev-hid-bpf add …` where the distro packages it. Loading needs `CAP_BPF` + `CAP_PERFMON`.
- **Lockdown:** integrity mode (Secure Boot on Fedora and Ubuntu) blocks nothing HID-BPF uses [architecture.md §2.7]. Check 15 confirms.
- **Licence:** the `.bpf.c` needs a GPL-compatible string, and `"Dual MIT/GPL"` is accepted [architecture.md §2.5]. Vendoring udev-hid-bpf's GPL-2.0-only helpers would make that one file GPL-2.0, which is fine as a separate file loaded by an MIT app [prior-art.md §2].

### 4.7 Packaging

- **Formats:** Tauri `deb`, `rpm` and `appimage`, set in `app/src-tauri/tauri.linux.conf.json` with `"resources": []` (the Windows driver glob matches nothing on Linux) [distribution.md §2.1].
- **Build environment:** an `ubuntu:22.04` container, for a glibc 2.35 floor (Debian 12+, Ubuntu 22.04+). Pin tauri-bundler ≥ 2.10.0, which fixes AppImage EGL white screens [app-stack.md §1.2, §1.5].
- **deb/rpm contents:** the rule, the hwdb, the user unit and the loader.
- **postinst** (skipped unless `/run/systemd/system` exists): `udevadm control --reload-rules`, `modprobe uinput`, `systemd-hwdb update`, then `udevadm trigger` for uinput, hidraw and the stick's hid device, so the loader runs without a replug (unverified; otherwise ask the user to replug). **postrm:** reload rules [distribution.md §2.2].
- **deb `Depends`:** override it so it carries no appindicator (`libappindicator3-1` does not exist on Debian; no tray anyway) [app-stack.md §1.1].
- **AppImage:** it cannot install rules. On first run it offers "Install device permissions": one `pkexec sh -c '…' < payload`, fed over stdin because root often cannot read the FUSE mount [distribution.md §2.3].
- **AUR:** `open-x3d-pro-bin`, repackaged from the `.deb` [distribution.md §2.5].
- **No Flatpak or Snap.** The sandbox cannot see host game processes, has no `/dev/uinput` without `--device=all`, cannot install rules or units, and cannot load BPF [app-stack.md §3.2; distribution.md §2.4].

### 4.8 SteamOS

- **Filesystem:** `/usr` is read-only and replaced on update. Since 3.6, only allow-listed `/etc` paths survive an update, so the helper also writes `/etc/atomic-update.conf.d/open-x3d-pro.conf` listing `/etc/udev/rules.d/70-open-x3d-pro.rules` and `/etc/udev/hwdb.d/61-open-x3d-pro.hwdb`, one per line [app-stack.md §6; distribution.md §2.6].
- **Install flow:** Desktop Mode → AppImage → pkexec helper. `deck` has no password until the user runs `passwd`. Never suggest `steamos-readonly disable` [distribution.md §2.3, §2.6].
- **Kernel:** HID-BPF is `=y` on 3.7 and 3.8 [architecture.md §2.8]. udev-hid-bpf is not shipped, hence our own loader.
- **Game Mode:** user services run there, unlike XDG autostart. `running` mode needs no display. In foreground mode, `GAMESCOPE_FOCUSED_APP` gives the appid [app-stack.md §6].
- **Deferred:** a Decky plugin (3–5 pd).

### 4.9 CI lanes (additions to [C §8])

| Lane | Runner | What |
|---|---|---|
| `rust-linux` (existing) | ubuntu-24.04 | Adds `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev`, now that the shell compiles for Linux [code-reuse.md §4.5] |
| `driver-host-test` (existing) | ubuntu-24.04 | Also links the integer core `pipeline_math.h` against `vectors.json`. The harness needed no changes in the prototype [code-reuse.md §2.6] |
| `bpf-build` | ubuntu-24.04 | `clang-18 -g -O2 --target=bpf` against a committed `vmlinux.h`. libbpf-cargo `SkeletonBuilder` builds the skeleton; no bpftool [distribution.md §3.3] |
| `linux-e2e` | ubuntu-24.04, sudo | 1. `modprobe uinput uhid`, then `setfacl` (no logind seat on runners).<br>2. uhid **stock** fake (`BUS_VIRTUAL`, the real descriptor once captured) → `ReadOnly` reader.<br>3. uhid fake of the **BPF-shaped** device, with the harness answering SET/GET_REPORT 3/4 → daemon push/info.<br>4. Phase 2 adds B: grab, fuzz reset, replug [distribution.md §4.3; code-reuse.md §6.7] |
| `linux-bundle` (`build.yml`) | ubuntu-24.04 + `container: ubuntu:22.04` | 1. `tauri build --bundles deb,rpm,appimage`.<br>2. `dpkg-deb -I/-c`, `rpm -qip --scripts`.<br>3. Install smoke on debian:12/13, ubuntu:24.04, fedora:44, archlinux.<br>4. `udevadm verify`, `systemd-analyze verify`.<br>5. One SHA256SUMS together with the Windows assets [distribution.md §4.1–4.2] |
| `hid-bpf-vm` (phase 2) | ubuntu-24.04 + KVM | 1. virtme-ng with a kernel built from `tools/testing/selftests/hid/config` (`HID_BPF=y`, `UHID=y`, BTF).<br>2. Matrix: 6.12 LTS and latest stable, cached `bzImage`.<br>3. uhid fake → BPF → vectors checked on evdev and hidraw [distribution.md §4.4] |
| `release.yml` | — | Split the SignPath `check` gate so Linux assets publish without SignPath secrets. Linux assets are unsigned; SHA256SUMS is the anchor [code-reuse.md §6.8] |

### 4.10 Support tiers

| Tier | Distros | Game output in v1 | Packages |
|---|---|---|---|
| 1 | Fedora 43/44, Arch, SteamOS 3.8 (Desktop; Game Mode via user unit), Debian sid/forky, Debian 13 + trixie-backports kernel | A | rpm, AUR-bin, deb, AppImage |
| 1 if `HID_BPF=y` | Ubuntu 24.04 HWE, 26.04 (check 1) | A, else read-only (B in phase 2 or via S3) | deb, AppImage |
| 1 pending check 11 | SteamOS 3.7 (6.11.11, the first struct_ops kernel; `hid_hw_request` fixes came later) | A or read-only | AppImage |
| 2 | Debian 13 stock, Ubuntu 24.04 GA, Debian 12, Ubuntu 22.04; Mint and Pop!_OS follow their base | read-only (B in phase 2) | deb, AppImage |
| 2 | Bazzite, Nobara, Tumbleweed: follow their kernels (unverified) | A or read-only | rpm, AppImage |
| 3 | NixOS, non-logind: `input`/`uinput` group fallback (needs re-login) | community | — |
| ✗ | Flatpak, Snap | — | — |

## 5. Contract changes needed (list only; edit CONTRACT.md in the PR that implements them)

- **Header.** Add a Linux constraint paragraph:
  - no kernel modules;
  - Secure Boot plus lockdown (integrity) supported;
  - root only at install and plug time;
  - game output needs kernel ≥ 6.11 with `CONFIG_HID_BPF=y`, otherwise read-only;
  - fix "100 Hz" (§6, item 1).
- **§1 Layout and names.**
  - Add `driver/bpf/openx3d.bpf.c` (`"Dual MIT/GPL"`) with a committed `vmlinux.h`.
  - Add `driver/bpf/loader/` (Rust bin `x3d-bpf-load`, libbpf-rs, embeds the `.bpf.o`).
  - Add `driver/pipeline/pipeline_math.h` and `app/src-tauri/tauri.linux.conf.json`.
  - Add `app/src-tauri/linux/`: `70-open-x3d-pro.rules`, `61-open-x3d-pro.hwdb`, `open-x3d-pro.service`, the atomic-update conf, `install-permissions.sh`.
  - Names: daemon `open-x3d-pro --daemon`, unit `open-x3d-pro.service`, socket `open-x3d-pro.sock`.
  - Packages: `Open-X3D-Pro_<ver>_amd64.deb`, `Open-X3D-Pro-<ver>-1.x86_64.rpm`, `Open-X3D-Pro_<ver>_amd64.AppImage` [distribution.md §4.2].
- **§2: new §2.2 "Linux: HID-BPF program".** struct_ops only; no 6.3–6.10 tracing API.
  - **`hid_rdesc_fixup`:**
    - publishes input ID 1 (7 B: X, Y u16, Rz u12, hat u4) and ID 5 (7 B: slider u16, buttons 1–32);
    - features 3/4 are byte-identical to §2.1, and there is no vendor input ID 2;
    - calls `hid_set_name("Logitech Extreme 3D Pro (Open X3D)")` and initialises identity.
  - **`hid_device_event`:** passes `source != 0` through. Otherwise it injects ID 1, rewrites the buffer in place as ID 5, and stashes raw and `seq`.
  - **`hid_hw_request` / `hid_hw_output_report`:**
    - SET 3 is validated (magic, version, size, CRC, field ranges as `blob::parse`) and written to a double-buffered map;
    - GET 3/4 are answered from the map;
    - anything else returns `-EIO`. Nothing is ever forwarded to the device.
  - No persistence.
  - `x3d_core::hid::REPORT_DESCRIPTOR_LINUX` plus `check-descriptor` keep the bytes in sync.
  - Conditional on checks 9–10. If check 9 shows no gluing, use the §2.1 layout.
- **§3.**
  - Linux paths: `$XDG_CONFIG_HOME/OpenX3DPro/…`.
  - Linux `Settings` default: `matchMode: 'running'`.
  - New `exePaths` entry form `steam:<appid>`.
  - Linux match name: `cmdline` argv[0], else `/proc/<pid>/exe`.
  - On Linux, `autostart` means the daemon's user unit.
- **§4.1.** Replace the Q16 sentence: integer implementations use the exact-fraction u64 form in `pipeline_math.h`. Q16.16 cannot meet ±1 at validated extremes, and .5 ties may differ by 1. Adopt it in the Windows driver at the same time [code-reuse.md §2.7].
- **§5.**
  - `driverInstalled` = "games see processed output"; `driverVersion` comes from feature 4.
  - Add `missingAccess: ('uinput' | 'hidraw')[]` (always `[]` on Windows) for the permission banner.
  - On Linux, `list_windows` returns X11/XWayland windows plus Wine processes, with `exePath` = the Windows path.
  - New subsection: the daemon socket protocol (§4.1).
- **§6.**
  - Linux process model (daemon + UI, no tray, systemd autostart).
  - Device access per §4.2, including "never GET_FEATURE before descriptor detection".
  - Push when the hidraw node appears.
  - Switching per §4.3.
  - uinput keyboard plus a `KEY_*` column.
  - cfg gates become `any(windows, target_os = "linux")`.
  - A Linux installer paragraph (§4.6–4.8).
- **§7.**
  - Platform copy via `navigator.userAgent`, no contract field [code-reuse.md §1].
  - A `missingAccess` banner.
  - Caveat text: "On Wayland, foreground switching sees XWayland/Proton windows only".
  - Autostart labelled "background service".
- **§8.**
  - The lanes in §4.9, with bundles built in an `ubuntu:22.04` container.
  - Release gate split; one SHA256SUMS over all assets.
  - Prerelease tags map `-` → `~` for deb/rpm, or no Linux prereleases (verify Tauri's behaviour) [distribution.md §6].

## 6. Corrections to existing docs

1. **Poll rate.**
   - **Where:** CONTRACT header ("7-byte input report at 100 Hz"); research.md L77 ("10 ms poll"), L89 ("bInterval 10 → 10 ms, 100 Hz"), L91 ("100 Hz is the ceiling").
   - **Correction:** on xHCI, Linux rounds the low-speed `bInterval 10` down to 64 µframes, i.e. **8 ms (125 Hz)**. The full-speed revision polls at 1 ms, and `usbhid.jspoll` has no effect on xHCI.
   - **Status:** Windows' xHCI probably rounds too (unverified). Measure with usbmon (check 6) [device.md §4].
2. **Report size vs the wire.** research.md L91 says the "report stays ≤ 8 bytes to games anyway since we own it, not the wire". That is true for UMDF but false for Linux HID-BPF: usbhid sizes the interrupt URB from the fixed descriptor, and the cap is wMaxPacketSize **7** [architecture.md §2.3].
3. **NOGET.** research.md L77 says "hid-lg applies LG_NOGET … read interrupt pipe only". In fact NOGET has been vestigial since v4.12 and does not stop hidraw `HIDIOCGFEATURE`/`HIDIOCGINPUT`, which go to the wire with a 5 s timeout. On Linux, only our BPF intercept and descriptor-first detection enforce the "never GET_REPORT" rule [device.md §1, §6].
4. **Permissions (missing from the docs).**
   - Joystick evdev nodes are `uaccess` by default.
   - hidraw nodes are root 0600, and no systemd or steam-devices rule covers c215.
   - `/dev/uinput` is root-only. Valve's steam-devices grants it, but Arch's `steam` ships no udev rules [device.md §6; app-stack.md §5].
5. **Proton axis order (missing).**
   - On Proton's default SDL path, axes are slotted positionally rather than as Windows' X, Y, Rz, Slider. This applies to the stock stick today and to both A and B.
   - Agreed: twist → Z.
   - Disputed: throttle → Rx (Proton #7121; architecture.md §4.1) or → Rz (Wine bug 55653; device.md §5).
   - Only the opt-in hidraw path gives the Windows layout, so document `PROTON_ENABLE_HIDRAW=0x046D/0xC215`.
6. **SDL gamepad DB history (missing).**
   - SDL < 2.0.14, and community `gamecontrollerdb.txt` before 2020-12, classified c215 as a gamepad. Games bundling an old SDL2 still do (Elite: Proton #3188).
   - Current SDL lists it as a flight stick.
   - Workaround: `SDL_GAMECONTROLLER_IGNORE_DEVICES=0x046d/0xc215`.
   - Never expose exactly 6 axes, or winebus's gamepad heuristic triggers [device.md §5; architecture.md §4.1].
7. **Kernel filtering (missing).**
   - Fuzz 3 on X/Y drops +1/+2 steps asymmetrically.
   - flat 63/15 become joydev dead bands of about 12 % in the centre, **mid-throttle included**. hwdb fixes evdev, not joydev [device.md §3].
   - The throttle reads 0 when fully forward; this is the hardware.
8. **Errors within the research set** (rulings in §3.4):

| File | Claim | Correction |
|---|---|---|
| code-reuse.md §3.3 | clone version = bcdDevice | bcdHID 0x0110 |
| code-reuse.md §6.7, distribution.md §4.3 | a bus-USB uhid fake binds | it stays unbound |
| code-reuse.md §2.5 | injection from `device_event` is unproven | selftest precedent exists |
| architecture.md §2.5/§7 | Q16 | u64 exact fraction |
| architecture.md §2.4 | hwdb fixes joydev | evdev only |
| prior-art.md §1a (OpenTabletDriver row) | the stick "is on hid-generic" | hid-lg [device.md §1] |
| prior-art.md §2 sketch | config via pinned maps | ruled out by bpffs 0700 [code-reuse.md §0.4] |
| distribution.md §5 | SteamOS 3.8 `HID_BPF` "verify" | verified `=y` |

## 7. Hardware and desktop verification checklist

This merges architecture.md §8 (13 items), device.md §9 (11), app-stack.md §9 (10) and distribution.md §7 (8), deduplicated.

★ marks the checks that decide the design:
- **D1, checks 9–11:** HID-BPF report size and the feature intercept (switch criteria S1/S2).
- **D2, check 1:** `CONFIG_HID_BPF` on the owner's distro and on Ubuntu (S3).
- **D3, check 22:** XWayland focus. It decides whether `foreground` mode is offered on Wayland.

Run group A on the stock stick before loading anything. Group B needs a HID-BPF kernel.

**A. Kernel and stock baseline**

1. ★ **Kernel config.** Run on the owner's distro, Ubuntu 24.04 HWE, Ubuntu 26.04 (live USB fine), SteamOS 3.8 and the GitHub runner:
   ```
   uname -r; grep -E 'CONFIG_(HID_BPF|DEBUG_INFO_BTF|HID_LOGITECH|HIDRAW|UHID|INPUT_UINPUT)=' /boot/config-$(uname -r); cat /sys/module/usbhid/parameters/jspoll
   ```
   On the runner, also:
   ```
   bpftool btf dump file /sys/kernel/btf/vmlinux | grep -c hid_bpf_ops; sudo modprobe uinput uhid && ls -l /dev/uinput /dev/uhid
   ```
2. **USB facts.**
   ```
   sudo lsusb -v -d 046d:c215 | grep -E 'bcdDevice|bInterval|wMaxPacketSize|iProduct|iSerial'
   for d in /sys/bus/usb/devices/*; do grep -qs c215 $d/idProduct && cat $d/bcdDevice $d/speed; done
   ```
3. **Descriptor fixture.** Needed by A's probe check, the uhid CI fakes and `core::hid`.
   ```
   for f in /sys/bus/hid/devices/0003:046D:C215.*; do readlink $f/driver; xxd $f/report_descriptor; done
   ```
   Expect `…/logitech` and 122 bytes, and commit the hex. Also run `sudo cat /sys/kernel/debug/hid/0003:046D:C215.*/rdesc` and check whether Rz inherits the hat's degree unit.
4. **evdev capabilities, pacing and initial state.**
   - `evtest /dev/input/by-id/usb-Logitech_Logitech_Extreme_3D-event-joystick`: diff the header against device.md §2. Note the value before the first move. Stir for 10 s: event gaps should be multiples of 8 ms (low-speed) or 1 ms (full-speed).
   - `cat /sys/class/input/event*/device/id/{vendor,product,version}`: expect `0110`.
   - `evdev-joystick --showcal <node>`.
5. **joydev baseline.** `jstest --normal /dev/input/js0` (order, mid-throttle band); `jscal -p /dev/input/js0`.
6. **Poll truth.** Is there an IN token every 8 ms? NAKs between changes? Repeat once booted with `usbhid.jspoll=1`.
   ```
   sudo modprobe usbmon; sudo cat /sys/kernel/debug/usb/usbmon/<bus>u; lspci -k | grep -A2 USB
   ```
7. **GET_REPORT on the stock stick** (root). In python3 on `/dev/hidrawN`, issue `HIDIOCGFEATURE(5)`:
   ```
   fcntl.ioctl(fd, (3<<30)|(5<<16)|(ord('H')<<8)|0x07, bytearray(5))
   ```
   Then `HIDIOCGINPUT` (`0x0A`, 8 bytes). Record the errno (110 after about 5 s, or 32) and whether interrupt reports keep flowing meanwhile.
8. **udev and permissions.**
   ```
   udevadm info /dev/input/eventN /dev/input/js0 /dev/hidrawN
   getfacl /dev/input/eventN /dev/hidrawN /dev/uinput
   ls -l /dev/hidraw* /dev/uinput /dev/uhid
   udevadm info /sys/bus/hid/devices/0003:046D:C215.*
   ```
   Look for `ID_INPUT_JOYSTICK=1` and the uaccess tag. The last command gives the exact `HID_ID` string for our rule.

**B. HID-BPF spike** (trixie-backports 7.2.6, or a Fedora 44/Arch live USB; two throwaway `.bpf.c` files, 1–2 pd)

9. ★ **URB framing.** Attach a test fixup that declares one 14-byte input report:
   ```
   sudo udev-hid-bpf add /sys/bus/hid/devices/0003:046D:C215.NNNN test14.bpf.o
   ```
   Do the `hid-recorder`/`evtest` timestamps show one report per 8 ms poll, or two raw reports glued at half rate?
10. ★ **2 × 7 split.** Inject ID 1 with `hid_bpf_try_input_report()`, rewrite the buffer as ID 5 and `return 7`. `hid-recorder` on hidraw should show ID 1 + ID 5 every poll, and `evtest` every code.
11. ★ **Config intercept.** `HIDIOCSFEATURE` 3 (256 B), then `HIDIOCGFEATURE` 3 and 4 via hidraw, should return the BPF's data, with **no** control transfer to the device in usbmon. Record the oldest kernel where the `hid_hw_request` member loads.
12. **Reprobe at attach.** The nodes reappear. `udevadm info` on the new event node shows `ID_INPUT_JOYSTICK=1`, uaccess and our name. The `evtest` header shows hwdb fuzz/flat 0. `jscal -p` shows joydev's flat (derived expectation: 4095).
13. **Codes and order.**
    - `evtest`: 0x120–0x12f + 0x2c0–0x2cf, `ABS_X/Y/RZ/THROTTLE`, `ABS_HAT0X/Y`.
    - `jstest`: 32 buttons in order.
    - SDL `testcontroller`: a flight stick, not a gamepad.
14. **Hotplug and suspend.** After replug and after `systemctl suspend`, the program is reattached and the daemon re-pushes (feature 4 CRC = pushed CRC). Check `bpftool struct_ops show` and `sudo udev-hid-bpf list-loaded`, and that no stale pins remain in `/sys/fs/bpf/hid/`.
15. **Lockdown.** On Fedora with Secure Boot and SELinux enforcing, `cat /sys/kernel/security/lockdown` shows `[integrity]` and the loader attaches unchanged.
16. **Full-speed revision 57.11** (if obtainable). Repeat checks 2, 3, 7 and 9. If its wMaxPacketSize is above 7, a single 14-byte report may be fine there.

**C. Games** (a Proton flight sim plus DIView; X-Plane and FlightGear if available)

17. **Proton default SDL path**, on the stock stick and then under A. Exactly one device should appear in DIView or the DCS controls. Record the axis slots (throttle on Rx or Rz?). It must not be detected as a gamepad.
18. **Proton hidraw path.** With `PROTON_ENABLE_HIDRAW=0x046D/0xC215 %command%` plus our hidraw rule, expect X, Y, Rz, Slider, the hat and 32 buttons, and the SDL twin dropped. Does it also fix the stock stick's order?
19. **Steam with "Generic gamepad support" on.** Run `tr '\0' '\n' < /proc/<game pid>/environ | grep SDL_`: is c215 in `SDL_GAMECONTROLLER_IGNORE_DEVICES`? Does Steam Input touch the stick?
20. **Native games.** X-Plane 12 should see one device via evdev, FlightGear one via `js*`.
21. **B only** (phase 2, or if S1/S2 triggers). `EVIOCGRAB` silences `jstest` and SDL. Confirm the twin is listed inside Proton.

**D. Desktop and app**

22. ★ **XWayland focus.** On GNOME 48/50, KDE 6 and Sway/Hyprland, run `xprop -root -spy _NET_ACTIVE_WINDOW` on `$DISPLAY` while switching between a Proton game, a native Wayland app and an XWayland app. Expect: game id, dummy id, app id.
23. **PID source.** Compare the XRes `QueryClientIds` PID with `xprop _NET_WM_PID` for a Proton game, under native and under Flatpak Steam.
24. **Proton identity.** For native Steam, Flatpak Steam (is environ readable?) and Lutris:
    ```
    cat /proc/<pid>/comm; tr '\0' '\n' < /proc/<pid>/cmdline; grep -z SteamAppId /proc/<pid>/environ
    ```
25. **Chords.** The uinput keyboard should reach a Wayland-native app, an XWayland app, a Proton game and a gamescope game, with the right keys on a non-US layout.
26. **Daemon unit.** Does it start on GNOME, KDE and in Deck Game Mode, and is `DISPLAY` set?
    ```
    systemctl --user status open-x3d-pro; systemctl --user show-environment | grep DISPLAY
    ```
27. **Tauri UI.**
    - NVIDIA proprietary + Wayland: blank window? Which env var fixes it (`WEBKIT_DISABLE_DMABUF_RENDERER=1`, …)?
    - KDE at 125 % and 150 % scaling.
    - GTK client-side decorations.

**E. Packaging, permissions, SteamOS**

28. **Rule + trigger, no re-login.** After `dpkg -i` or `dnf install`, `getfacl /dev/uinput /dev/hidrawN` should show `user:<you>:rw-` (GNOME, KDE). The `/dev/uinput` ACL should survive a reboot on module-built kernels without `modules-load.d`.
29. **Package output.** `dpkg-deb -I/-c`, `rpm -qip --scripts`, `rpm -qp --requires`. Check the prerelease version mapping and the scriptlets on clean VMs, then run `udevadm verify` and `systemd-analyze verify`.
30. **AppImage.** Built in `ubuntu:22.04` with bundler ≥ 2.10.0, it should start on current Fedora, Arch and SteamOS Desktop Mode, and the pkexec helper should work on KDE and GNOME.
31. **SteamOS 3.8.**
    - Is uinput already uaccess (`getfacl /dev/uinput`)? Is `deck` in the `input` group (`id deck`)?
    - After a real OS update, do the rule, hwdb and loader persist via the keep-list (and `/var`)?
    - Does the user unit survive Game/Desktop switches and reboots?
    - Is `GAMESCOPE_FOCUSED_APP` readable on gamescope's X display?

## 8. Phased plan with effort

Person-days, optimistic/likely. Item IDs are from code-reuse.md §5.1 unless noted.

| Phase | Item | pd |
|---|---|---|
| **0: Linux build, read-only** | P0.1 cfg widening, `tauri.linux.conf.json` · P0.2 evdev `ReadOnly` reader → `InputFrame` · P0.3 watcher stub, `/proc` `list_windows` · P0.4 CI WebKit deps + Linux `tauri build --no-bundle` · P0.5 frontend platform copy | 2.5 / 4.5 |
| | Group A captures + B spike (checks 1–11) on owner hardware; carved out of A9 | 1 / 2 |
| | **Phase 0 total.** The app runs on any distro with no rule; Test/Axes/Profiles work; `driverInstalled=false` | **3.5 / 6.5** |
| **1: A + daemon + shared** | Daemon/UI split: socket, command proxy, `--daemon` entry (code-reuse.md B2; app-stack.md: 4.5) | 3 / 5 |
| | S1 Linux watcher (`/proc`, Wine cmdline, x11rb, XRes) · S2 `KEY_*` column + uinput keyboard · S3 systemd autostart (replaces tray work) | 3 / 5.75 |
| | `steam:<appid>` entries + tests (app-stack.md §7) | 0.5 / 0.75 |
| | S4 packaging (deb/rpm/AppImage, rule, hwdb, unit, postinst, pkexec helper, SteamOS keep-list) · S5 bundle lane, release-gate split · S6 CONTRACT/docs | 2.25 / 4.5 |
| | A1 `pipeline_math.h` (prototype exists; adopted by the Windows driver too) · A2 BPF program · A3 BPF build | 4.5 / 8 |
| | A4 loader + install + `HID_BPF` detection · A5 `core::hid` Linux descriptor + parser · A6 daemon `Bpf` backend · A8 rule + hwdb | 3.75 / 7.5 |
| | `linux-e2e` uhid lane (distribution.md §4.3) · rest of A9 hardware validation | 1 / 2 |
| | **Phase 1 total** | **18 / 33.5** |
| | **v1 = phase 0 + 1** | **21.5 / 40** |
| **2: fallback + polish** | A7 `hid-bpf-vm` lane | 2 / 4 |
| | B1 uinput joystick (`EVIOCGRAB`, `EVIOCSABS` fuzz 0, clone id copied from the node, Windows name) · B2 rules (no hide rule; twin documented) · B3 e2e assertions (lane exists) · B4 hardware validation | 3.25 / 6.25 |
| | **Phase 2 total** | **5.25 / 10.25** |
| Deferred, on demand | KWin script 1–1.5 · GNOME extension 2–3 (+ per-version upkeep) · Decky plugin 3–5 · KSNI tray 1 [app-stack.md §7] | — |

**If S1 or S2 triggers:**
- Phase 1 drops A1–A6, A8 and the rest of A9 (8.75/16.5) plus the uhid lane (0.5/1).
- It gains B1–B4 (3.75/7.25; B3 brings its own e2e test).
- v1 becomes 16/29.75, and phase 2 becomes optional A.

**Non-goals for v1:**
- Flatpak and Snap.
- A tray.
- Native-Wayland foreground adapters (KWin, GNOME, wlroots).
- A Decky / Game Mode UI.
- C, the uhid daemon.
- Hiding raw devices.
- Game output on non-BPF kernels; they get read-only only.
- The HID-BPF 6.3–6.10 tracing API.
- Applying config before login.
- ARM64 packages.
- apt/dnf repositories and package signing.
- GameMode detection and screensaver inhibit (joystickwake idea) [prior-art.md §1a].

## 9. Open questions and risks, ranked

1. **URB framing and the 2×7 split** (checks 9–10) block A's descriptor. The risk is derived from usbhid code and USB completion rules and has not been tested on hardware [architecture.md §2.3].
2. **Ubuntu `CONFIG_HID_BPF`** (check 1) decides whether B ships in v1. The Launchpad annotations returned 403 [architecture.md §2.8].
3. **The `hid_hw_request` kernel floor.** A CFI-stub load bug and a 2026-03 overflow fix mean SteamOS 3.7's 6.11.11, and LTS kernels, may be too old (check 11; VM matrix) [code-reuse.md §2.5; device.md §1].
4. **Novelty and debuggability.**
   - There is no HID-BPF curve prior art [prior-art.md §2].
   - The verifier imposes a constant size in `hid_bpf_get_data` and a 512-byte stack, so the 240-byte blob must be read through a map pointer [code-reuse.md §2.4].
   - The agent's dev box cannot load BPF (no root, no KVM, `HID_BPF` unset), so kernel-side bugs surface only in the CI VM or on owner hardware.
5. **Proton axis slotting** (checks 17–18). Without the hidraw opt-in, users get the stock positional layout. The fix is a per-game launch option and is unverified.
6. **Wayland foreground blind spot.** Native-Wayland games (SDL3 on fifo-v1, GE-Proton `PROTON_ENABLE_WAYLAND=1`, a future Proton default) are invisible. Mitigation: the `running` default [app-stack.md §8].
7. **Steam interference.** Steam's "Generic gamepad support" or Steam Input acting on c215 is unverified, and would hit A and B equally (check 19).
8. **SteamOS persistence.** It depends on the `/etc` keep-list and on `/var` for the loader. `deck` has no default password, and the Game Mode unit lifecycle is unverified (checks 26, 31).
9. **uinput `uaccess`** lets any process of the active user inject keys. Steam's rule already does the same; document it [distribution.md §6].
10. **joydev dead bands under A.** About 12.5 % in the centre on every axis, throttle included; hwdb cannot remove them. No worse than stock, and only `js*` readers are affected. Fixes: `jscal-restore`, or a Multi-axis application usage as hid-lg uses for wheels (side effects unverified) [device.md §3; architecture.md §2.4].
11. **Rz resolution.** The 8-bit Rz is carried in 12 bits. architecture.md calls this lossless. Strictly, adjacent outputs merge only where the curve slope is below 1/16 (derived, negligible).
12. **WebKitGTK instability:** NVIDIA DMABUF problems, AppImage EGL failures, and a bundler 2.10.0 that is 8 days old. The daemon split keeps games isolated from UI crashes [app-stack.md §1.2].
13. **hidapi `linux-native-basic-udev` maturity** [code-reuse.md §6.1]. Sysfs descriptor detection avoids relying on its per-collection enumeration.
14. **One rdesc fixup per device.** A future distro-shipped c215 fixup would collide with ours [distribution.md §3.1].
15. **Release plumbing.** The SignPath-only `check` job blocks Linux releases until it is split [code-reuse.md §6.8].

## 10. Sources

**Research files** (they hold every citation; not repeated here):
- [`architecture.md`](../research/linux/architecture.md)
- [`code-reuse.md`](../research/linux/code-reuse.md)
- [`app-stack.md`](../research/linux/app-stack.md)
- [`device.md`](../research/linux/device.md)
- [`prior-art.md`](../research/linux/prior-art.md)
- [`distribution.md`](../research/linux/distribution.md)

Also [`CONTRACT.md`](CONTRACT.md) and [`research.md`](research.md).

**Primary sources for the facts the verdict rests on** (read 2026-10-04 by the researchers):
1. **HID-BPF API, struct_ops ≥ 6.11, `hid_hw_request`, one rdesc fixup per device:** https://docs.kernel.org/hid/hid-bpf.html
2. **BPF runs before hidraw and hid-input, and intercepts requests; usbhid sizes the interrupt URB from the descriptor (the 7-byte cap):** https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/tree/drivers/hid/hid-core.c · https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/tree/drivers/hid/usbhid/hid-core.c
3. **Debian enabled `HID_BPF` in 6.16.3-1, not in trixie:** https://bugs.debian.org/1110780
4. **Fedora and Arch `CONFIG_HID_BPF=y`:** https://gitlab.com/cki-project/kernel-ark/-/raw/os-build/redhat/configs/common/generic/CONFIG_HID_BPF · https://gitlab.archlinux.org/archlinux/packaging/packages/linux/-/raw/main/config.x86_64
5. **SteamOS 3.7/3.8 kernel configs (`HID_BPF=y`):** https://steamdeck-packages.steamos.cloud/archlinux-mirror/sources/
6. **Proton 10 `PROTON_ENABLE_HIDRAW` and the VID/PID dedupe:** https://github.com/ValveSoftware/wine/tree/proton_10.0/dlls/winebus.sys
7. **Wine hidraw allowlist (c215 absent), SDL as the default backend:** https://gitlab.winehq.org/wine/wine/-/blob/master/dlls/winebus.sys/main.c
8. **SDL lists c215 as a flight stick; ignore hints match by VID/PID:** https://github.com/libsdl-org/SDL/blob/main/src/joystick/SDL_joystick.c
9. **Joystick evdev `uaccess` by default; no hidraw or uinput rule:** https://github.com/systemd/systemd/blob/main/rules.d/70-uaccess.rules.in
10. **The uinput `uaccess` line Steam already ships:** https://github.com/ValveSoftware/steam-devices/blob/master/60-steam-input.rules
11. **bpffs mounted 0700 (no user-written pins):** https://github.com/systemd/systemd/blob/main/src/shared/mount-setup.c
12. **xHCI rounds the low-speed interval to 8 ms:** https://github.com/torvalds/linux/blob/a90ee4305c4a5df72c11b31dacfdc76e00fcf78a/drivers/usb/host/xhci-mem.c
13. **Axis-order bugs:** Wine bug 55653 https://bugs.winehq.org/show_bug.cgi?id=55653 · Proton #7121 https://github.com/ValveSoftware/Proton/issues/7121
14. **SteamOS `/etc` keep-list:** https://blogs.igalia.com/berto/?p=954
15. **The hide recipe B would need root for:** https://github.com/hhd-dev/hhd (`src/hhd/controller/lib/hide.py`) · https://github.com/ShadowBlip/InputPlumber (`src/udev/mod.rs`)
