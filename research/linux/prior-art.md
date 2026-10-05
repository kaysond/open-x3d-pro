# Linux prior art: joystick remapping, curves, virtual devices, per-app profiles

Research for Open X3D Pro, a possible Linux port. Snapshot taken **2026-10-04**. Metadata comes from the GitHub, GitLab and Codeberg APIs, Repology and crates.io. Behaviour claims come from READMEs, man pages and source files fetched the same day. Anything not confirmed in source is marked *(unverified)*.

Legend: **C** = axis curves, **DZ** = deadzones, **R** = axis/button remap, **P** = saved profiles, **A** = per-app auto-switch. ✔ = yes, ~ = partial or limited, ✘ = no.
**Grab** = `EVIOCGRAB` on the physical evdev node. **uinput** = virtual evdev device. **uhid** = virtual HID device, which also creates a hidraw node.

---

## 1. Remappers and virtual-device tools

### 1a. Joystick and gamepad remappers (the closest analogues)

| Project | Lang / License | Latest release · last push | Architecture | C / DZ / R / P / A | GUI · Packaging · Permissions | Known problems | Reusable for us |
|---|---|---|---|---|---|---|---|
| [input-remapper](https://github.com/sezanzeb/input-remapper) | Python / GPL-3.0 | 2.2.1 (2026-06-12) · 2026-09-28 | Root `input-remapper-service` on the system D-Bus, plus a root reader-service started via pkexec. It grabs the source and emits two uinput devices: "mapped" and "forwarded" (unmapped events) | ~ (expo ∈(-1,1)) / ✔ (`deadzone`, `gain`) / ✔ / ✔ per-device JSON presets with autoload / ✘ | GTK3 (PyGObject) · deb, rpm, AUR, nixpkgs, shipped in Bazzite · root service + pkexec | [#1045](https://github.com/sezanzeb/input-remapper/issues/1045): games see **two gamepads** (mapped + forwarded). [#1082](https://github.com/sezanzeb/input-remapper/issues/1082): per-app request still open, blocked on Wayland. [#324](https://github.com/sezanzeb/input-remapper/issues/324): joystick stops passing through during injection | UX: the record-input → map flow and the analog "expo" parameter. GPL, so ideas only |
| [evsieve](https://github.com/KarsMulder/evsieve) | Rust / GPL-2.0 | v1.4.0 (2023-06) · 2026-08-22 | CLI pipeline: `--input … grab` → `--map/--hook/--toggle` → `--output` (uinput). `persist=reopen` survives replug. Supports systemd `Type=notify` | ✘ (linear only: `abs:x:-x`, `0.5x`) / ✘ / ✔ / ✘ / ✘ | none · AUR, nix · root or rw on `/dev/uinput` + nodes | No curves or deadzones. Doesn't solve duplicates beyond grab (the grabbed node is still enumerable) | Ideas: hook/toggle semantics (shift layer), `persist=reopen`, `create-link=` for stable `/dev/input/by-id` symlinks |
| [MoltenGamepad](https://github.com/jgeumlek/MoltenGamepad) | C++ / **MIT** | v1.2.1 (2020-10) · 2023-10 (dormant) | Daemon with plugin drivers. "gendevices" text files describe generic devices. Virtual pads via uinput, assigned to slots | ✘ / ✘ / ✔ / ✔ per-device / ✘ | CLI/FIFO · source only · udev rules for uinput + event nodes | Unmaintained. No device hiding | MIT: the declarative device-description idea |
| [SC-Controller](https://github.com/C0rn3j/sc-controller) (fork of [kozec](https://github.com/kozec/sc-controller), dormant since 2023) | Python + C / GPL-2.0 | v1.0.5 (2026-09-28) · active | Custom drivers for the Steam Controller, Deck and DS4/5, plus generic evdev (grabs all evdev nodes behind a hidraw). Output through its own `scc/uinput.c`, built as `libuinput.so` and loaded with **ctypes** | ~ / ✔ (per-axis calibration + deadzone) / ✔ / ✔ / **✔ by window, X11 only** | GTK4 + gtk4-layer-shell · Arch extra, AppImage, Gentoo, Void; Flatpak "planned" · udev rules + python-pylibacl | README: autoswitch is "*X11 only for now*" and the OSD doesn't work on GNOME | Lesson: one per-OS window-focus backend isn't enough. The C-uinput-via-FFI approach is unnecessary in Rust (use the `evdev` crate) |
| [xboxdrv](https://github.com/xboxdrv/xboxdrv) | C++ / GPL-3.0 | no GH releases · 2026-08-28 | Userspace driver. `--evdev` mode grabs a generic stick, maps it to an Xbox360 model, then out via uinput. Session D-Bus `org.seul.Xboxdrv` | ✔ (`resp:` piecewise response curve, `sen:`) / ✔ / ✔ (+ **shift buttons** `--ui-buttonmap LB+A=…`) / ✔ (config files) / ✘ | CLI · AUR · root | README: "*mostly obsolete*". `--evdev` is "*limited to the number of axis and buttons that an Xbox360 controller provides*". The man page admits the hidden-node problem: the grabbed "*node may still appear in device lists*" | Ideas: piecewise-linear `resp:` curve syntax and modifier-shift buttons, the same feature set we ship |
| [AntiMicroX](https://github.com/AntiMicroX/antimicrox) | C++ / GPL-3.0 | 3.6.1 (2026-05-24) | SDL2 input → keyboard/mouse/macro output (not a virtual joystick) | ~ (mouse curves) / ✔ / ✔ (to KB/M only) / ✔ / **✔ by window, "not in Wayland"** | Qt6 (Qt5 fallback) · Flatpak, AppImage, deb, rpm, Arch | Per-app is X11-only | Lesson: Qt and Flatpak packaging work for KB/M output; a virtual joystick doesn't fit that model |
| [QJoyPad](https://github.com/panzi/qjoypad) | C++ / GPL-2.0 | v4.3.1 (2019-07) · 2023-05 | Reads joydev `/dev/input/js*`, emits X11 keys | ~ / ✔ / ✔ (KB/M) / ✔ / ✘ | Qt · distro pkgs · user | X11-only. Uses the legacy joydev API. Dormant | none |
| [joystickwake](https://codeberg.org/forestix/joystickwake) (moved from GitHub 2026) | Python / **MIT** | Debian 1.0 · Codeberg 2026-08-05 | Watches udev for `ID_INPUT_JOYSTICK` devices. On activity it runs wake commands and holds an `org.freedesktop.ScreenSaver` **Inhibit** cookie | n/a | none · Debian, AUR, nix · user | n/a | **Yes**: compositors don't count joystick input as user activity, so long flights blank the screen. Our daemon should inhibit idle while the stick moves |
| [joystick / linuxconsole](https://sourceforge.net/projects/linuxconsole/) (`jscal`, `jstest`, `evdev-joystick`) | C / GPL-2.0 | Debian 1.8.1 | `jscal` sets joydev correction (JSIOCSCORR, `/dev/input/js*` only). `evdev-joystick` sets evdev `flat`/`fuzz` via EVIOCSABS | ~ (calibration only) / ~ / ✘ / ✘ / ✘ | CLI · deb/rpm/AUR · root or udev `RUN+=` | **Nobody honours these knobs** (§5 pitfall 3): SDL ignores evdev `flat` unless `SDL_JOYSTICK_LINUX_DEADZONES=1`, and joydev correction never reaches SDL/Wine's evdev path | Lesson: never "configure the kernel axis". Transform the values ourselves |
| [ControllerBuddy](https://github.com/bwRavencl/ControllerBuddy) | Java / GPL-3.0 | 1.10.82 (2026-10-01) | SDL/LWJGL input → own `UinputDevice` (FFI to `/dev/uinput`) on Linux, vJoy on Windows. Flight-sim focused (DCS integration) | **✔** (`AxisToAxisAction`: dead zone + **exponent**) / ✔ / ✔ / ✔ JSON + **modes** (toggle/momentary) / ✘ | Swing + FlatLaf · Flatpak, tgz (jpackage) · udev `KERNEL=="uinput", TAG+="uaccess"` + modules-load | No hiding of the physical device (games see both). Flatpak needs a manual udev rule | The closest feature twin: cross-platform, curve + deadzone + modes, uinput on Linux and vJoy on Windows. GPL, so ideas only |
| [MMVJ](https://github.com/leosat/MMVJ) | Rust (egui) / **All rights reserved** | 2026.10.2 | evdev/uinput, transformation pipelines (curves, filters, Luau scripts), FFB | ✔ / ✔ / ✔ / ✔ / ✘ | egui · AppImage · `input` group | Licence not open. PRs not accepted | Ideas only: per-step live signal graphing in the GUI is a good curve-editor UX |
| [inputtino](https://github.com/games-on-whales/inputtino) | C++ (Rust/Python bindings) / **MIT** | n/a | Virtual input library. Keyboard/mouse via uinput; **joypads via uhid** so a hidraw node exists (DualSense features) | n/a | lib | n/a | **MIT** reference for uhid device creation, needed if we ever want a hidraw-visible virtual stick |
| [OpenTabletDriver](https://github.com/OpenTabletDriver/OpenTabletDriver) (non-joystick) | C# / LGPL-3.0 | active | Cross-platform userspace driver: hidraw/libusb in, uinput out. Linux **blacklists kernel modules** (`wacom`, `hid_uclogic`) to stop duplicates. Daemon is a **systemd `--user` unit** | n/a (pressure curves) | Avalonia · deb/rpm/AUR · udev uaccess rules | Blacklisting isn't an option for us: the stick is on `hid-generic` | Best precedent for "same app on Windows and Linux, userspace driver, user-level daemon" |
| [Oversteer](https://github.com/berarma/oversteer) (wheels) | Python / GPL-3.0 | AUR 0.8.3 · 2026-02 | Sysfs/evdev wheel settings; combines pedal axes ("useful for flight") | ~ / ✔ / ~ / ✔ / ✘ | GTK3 · AUR, Flatpak · README: the Flatpak "*permission files have to be installed manually*" | Flatpak can't ship udev rules | Lesson: a Flatpak-only release forces a manual root step |

### 1b. Gaming daemons: InputPlumber and Handheld Daemon (study closely)

| Project | Lang / License | Latest | Architecture | C / DZ / R / P / A | API · Packaging · Permissions | Known problems | Reusable for us |
|---|---|---|---|---|---|---|---|
| [InputPlumber](https://github.com/ShadowBlip/InputPlumber) (Bazzite/ChimeraOS) | Rust / GPL-3.0 | v0.81.0 (2026-09-14) · daily | **Composite devices**: YAML `devices/*.yaml` match sources (evdev, hidraw, iio) into one logical device, with **capability maps** per source. Targets: uinput (keyboard, mouse, xpad, unified_gamepad), **uhid** (DualSense, `steam_deck_uhid`), and **USB/IP vhci** ([virtual-usb-rs](https://github.com/ShadowBlip/virtual-usb-rs)) for a "real" USB Steam Deck. **Hides sources** by writing `/run/udev/rules.d/50-…-early.rules` + `96-…-late.rules` (MODE 0000, `setfacl -b`, optional move to `/dev/inputplumber/sources/`), then re-triggering. Deps: `evdev` (emberian), `udev`, `zbus`, `uhid-virt`, `hidapi`, tokio | ✘ / ~ (DZ only as axis→button threshold) / ✔ (`DeviceProfile` YAML) / ✔ / ~ (an external app loads profiles over D-Bus) | System D-Bus `org.shadowblip.InputPlumber` (`LoadProfilePath`, `InterceptMode`, `SendKey`) · Arch, AUR, nix, openSUSE · **root systemd `Type=dbus` service**, polkit rules allow group `inputplumber` or `wheel`, hwdb autostart (`USE_INPUTPLUMBER=1` → `SYSTEMD_WANTS`) | No flight-stick target type; gamepad targets only. No BPF. No curves | **Architecture blueprint**: root daemon + D-Bus + polkit, a profile loaded per composite device, runtime hide/unhide rules in `/run`, hwdb-triggered autostart. GPL, so reimplement |
| [hhd (Handheld Daemon)](https://github.com/hhd-dev/hhd) | Python / LGPL-2.1+ (some files MIT-dual) | v4.1.12 (2026-07-10) | Root daemon (`hhd --user …`). Emulates DualSense/Steam pads via uhid/uinput. **Origin of the hide technique**: `hide.py` writes `/run/udev/rules.d/95-hhd-devhide-*.rules` that set `ENV{ID_INPUT_JOYSTICK}="0"` (and every other `ID_INPUT_*`), `MODE:="000"`, `TAG-="uaccess"` on evdev **and** on the hidraw/hiddev of the same USB device, then `udevadm trigger --action remove/add`. Optional `EVIOCREVOKEALL` (custom kernel ioctl `_IOW('E',0x94,4)`) revokes fds already open | ✘ / ✔ (sticks) / ✔ / ✔ / ~ (gamescope overlay) | gamescope overlay + desktop app · install script; AUR/COPR "not recommended" · root; optional user rules set uinput/uhid to **0666** (the file itself says "*FIXME: These udev rules are too permissive*") | Breaks on mainline kernels per its README. The over-permissive rules are a security smell | The hide rule (`hide.py`) is the reference design. Comment in source: clearing `ID_INPUT_*` exists to "*Keep SDL from falling back to probing the hidden device's sysfs capabilities*" |
| [OpenGamepadUI](https://github.com/ShadowBlip/OpenGamepadUI) | GDScript + Rust ext / GPL-3.0 | v0.46.1 (2026-09-03) | Godot 4 launcher/overlay. **Per-game profiles** pushed to InputPlumber over D-Bus. Focus detection via gamescope X atoms (`GAMESCOPE_FOCUSED_APP`, `STEAM_GAME`, `GAMESCOPE_FOCUSABLE_APPS`) through [gamescope-x11-client](https://github.com/ShadowBlip/gamescope-x11-client) | via InputPlumber / ✔ **per game** | Godot · gamescope session only | Only works inside a gamescope session | Idea: when running under gamescope (Steam Deck, Bazzite game mode), read `STEAM_GAME`/`GAMESCOPE_FOCUSED_APP` for an exact AppID |

#### InputPlumber / hhd deep dive

Flow, from the InputPlumber `docs/usage.md`, `rootfs/` and `src/udev/mod.rs`:

1. **Autostart**:
   - `60-inputplumber-autostart.hwdb` matches DMI strings of known handhelds and sets `USE_INPUTPLUMBER=1`.
   - `90-inputplumber-autostart.rules` adds `TAG+="systemd", ENV{SYSTEMD_WANTS}+="inputplumber.service"`.
   - So the daemon starts only on matching hardware. The same pattern works for a VID/PID match on `046d:c215`.
2. **Service**: `Type=dbus`, `BusName=org.shadowblip.InputPlumber`, `ProtectSystem=full`, running as root.
3. **Discovery**: the Manager watches udev. When a source matches a `CompositeDevice` YAML, it creates `/org/shadowblip/InputPlumber/CompositeDeviceN` and its target devices.
4. **Hiding**:
   - `hide_device()` saves the node's mode, writes an early (50) rule with `MODE:="0000", GROUP:="root"` + `chmod 000` + `setfacl -b` and a `SYMLINK+="inputplumber/by-hidden/%k"`, plus a late (96) rule that wins over later `uaccess` tagging.
   - It then re-triggers udev.
   - `unhide_all()` deletes every `*-inputplumber-hide-*` rule and restores the saved modes.
5. **Translation**: source events → capability map (normalises vendor quirks) → optional `DeviceProfile` (user remap) → targets.
6. **Control**:
   - D-Bus methods `LoadProfilePath`, `InterceptMode` (0 none / 1 pass-until-Guide / 2 all / 3 gamepad-only, used to steal input for overlays) and `SendKey`.
   - polkit actions `org.shadowblip.Input*`/`Output*` are granted to groups `inputplumber`/`wheel`.
7. **Suspend**: `inputplumber-suspend.service` exists because virtual devices and grabbed sources misbehave across sleep.

hhd's reference hide rule (abridged from `src/hhd/controller/lib/hide.py`):

```
SUBSYSTEMS=="input", KERNELS=="inputN", ATTRS{id/vendor}=="046d", ATTRS{id/product}=="c215", GOTO="hhd_valid"
GOTO="hhd_end"
LABEL="hhd_valid"
KERNEL=="js[0-9]*|event[0-9]*", SUBSYSTEM=="input", ENV{ID_INPUT}="0", ENV{ID_INPUT_JOYSTICK}="0", …, MODE:="000", GROUP:="root", TAG-="uaccess", RUN+="/bin/chmod 000 /dev/input/%k"
LABEL="hhd_end"
SUBSYSTEM=="hidraw", KERNELS=="<usb-port>", ATTRS{idVendor}=="046d", ATTRS{idProduct}=="c215", MODE:="000", GROUP:="root", TAG-="uaccess"
SUBSYSTEM=="usb", KERNEL=="hiddev[0-9]*", <same match>, MODE:="000", GROUP:="root", TAG-="uaccess"
```

hhd then runs `udevadm control --reload-rules` and `udevadm trigger --action remove` then `--action add -b <usb parent>`.

What it gets right: it covers evdev, joydev, hidraw and hiddev, and it neutralises SDL's udev classification.
What it misses: fds opened before hiding stay valid. hhd needs the out-of-tree `EVIOCREVOKEALL` for those; upstream offers only `EVIOCREVOKE` per fd, held by the opener.

### 1c. Keyboard-only daemons (design references for daemon, permissions and per-app)

| Project | Lang / License | Latest | Daemon / permission design | Per-app focus detection | Reusable |
|---|---|---|---|---|---|
| [keyd](https://github.com/rvaiya/keyd) | C (+Python mapper) / **MIT** | v2.6.0 (2025-12-19) | Root `keyd.service`; grabs keyboards → one uinput device. IPC socket `/var/run/keyd.socket` gated by the `keyd` group (`sysusers.d: g keyd`). Docs recommend a libinput quirk for the virtual device | `keyd-application-mapper` (user process → IPC). KDE: injects a **KWin script** via D-Bus. wlroots: hand-parsed **`zwlr_foreign_toplevel_manager_v1`** (uses the `activated` state). COSMIC: `zcosmic_toplevel_info_v1`. X11: Xlib. GNOME: **ships its own Shell extension** | **MIT**: the socket-plus-group alternative to polkit, and the per-compositor focus backends |
| [xremap](https://github.com/xremap/xremap) | **Rust** / **MIT** | v0.15.14 (2026-09-27) | User + `input` group, or root. evdev → uinput | Cargo features `gnome` (zbus → extension `com.k0kubun.Xremap`), `kde`, `x11` (x11rb), `hypr`, `wlroots` (wayland-protocols-wlr), `niri`, `cosmic`, `pantheon`, `socket`. Code in `src/client/*_client.rs` | **MIT Rust code we can vendor** for focused-app detection |
| [makima](https://github.com/cyber-sushi/makima) | Rust / GPL-3.0 | v0.10.3 (2026-01-31) | systemd daemon, evdev → uinput, udev rule + uaccess. Handles **gamepad sticks** (stick→mouse, `*_DEADZONE`, sensitivity) | Per-app config files `Name::window_class.toml`: Hyprland, Sway, Niri native; Plasma via `kdotool` (perf complaints); X11. **GNOME Wayland unsupported** | Idea: profile-per-window-class filename convention |
| [kanata](https://github.com/jtroo/kanata) | Rust / LGPL-3.0 | v1.12.0 (2026-07-05) | One core, per-OS backends: Linux evdev/uinput, Windows LLHOOK or Interception driver, macOS Karabiner driver | layer-based, no app focus | The cross-platform core/backend split our Rust app needs |
| [evdevremapkeys](https://github.com/philipl/evdevremapkeys), [evremap](https://github.com/wez/evremap) | Python / MIT; Rust / MIT | 1.0.3 (2026-05); evremap 2024-08 | grab + uinput, YAML/TOML | ✘ | Minimal MIT examples of the grab → uinput loop |

### 1d. Kernel-module drivers: the anti-pattern

| Project | License | Latest | Why it's the anti-pattern |
|---|---|---|---|
| [xpadneo](https://github.com/atar-axis/xpadneo) | GPL-ish (NOASSERTION) | v0.10.4 (2026-07-10) | DKMS out-of-tree module. README: Secure Boot needs `openssl` + `mokutil` and MOK enrollment. Rebuilds on every kernel update. No HID-BPF migration stated |
| [xone](https://github.com/dlundqvist/xone) (fork of [medusalix](https://github.com/medusalix/xone), stale since v0.3, 2022) | GPL-2.0 | v0.5.8 (2026-03-17) | Same DKMS and Secure Boot story, plus a firmware download. Needed a community fork when upstream stalled |
| Upstream alternative | GPL-2.0 | — | A ~50-line HID-BPF `.bpf.c` loaded by udev ([FR-TEC Raptor Mach 2](https://gitlab.freedesktop.org/libevdev/udev-hid-bpf/-/blob/main/src/bpf/stable/0010-FR-TEC__Raptor-Mach-2.bpf.c)) fixes a joystick **without a module**. Our Windows side already avoids kernel drivers for HVCI; keep the same rule on Linux |

---

## 2. HID-BPF ecosystem

| Item | Facts (verified 2026-10-04) | Relevance |
|---|---|---|
| Kernel [HID-BPF](https://docs.kernel.org/hid/hid-bpf.html) | Kernel ≥ 6.3 (`hid_bpf_attach_prog`); **struct_ops** attach since 6.11 (`HID_BPF_OPS`). Hooks: `hid_rdesc_fixup` (descriptor rewrite at bind), `hid_device_event` (rewrite each input report), `hid_bpf_hw_request`, `hid_bpf_input_report` (inject). Docs: "*HID programs need to be GPL*". Programs persist via **bpffs pin**. BPF maps (ringbuf, arrays) are allowed. The kernel doc's own use case: "*dead zone of joysticks … apply this filtering in the kernel directly*" | Curves/deadzones/remap could run **in-kernel on the real device**: no virtual device, no duplicates |
| Where the data goes (hid-core.c, read today) | `__hid_input_report()` calls `dispatch_hid_bpf_device_event()` **before** `hid_report_raw_event()` → `hidraw_report_event()`. `HIDIOCGRDESC` returns `hid->rdesc`, which is derived from `bpf_rdesc` | evdev, joydev, **hidraw** (Wine hidraw, Steam, SDL hidapi) all see the BPF-modified stream. The only approach that covers the hidraw path without uhid |
| [udev-hid-bpf](https://gitlab.freedesktop.org/libevdev/udev-hid-bpf) (bentiss, whot) | Rust loader (libbpf-rs) / **GPL-2.0**. 2.3.0-20260703. Meson-built. `81-hid-bpf.rules`: `IMPORT{builtin}="hwdb --subsystem=hid --lookup-prefix=hid-bpf:"`, then runs `udev-hid-bpf add $sys$devpath`. The hwdb maps modalias `hid:b0003g0001v…p…` to `HID_BPF_xxx=<file>.bpf.o`. Metadata via `HID_BPF_CONFIG(HID_DEVICE(bus,group,vid,pid))`. A `probe()` syscall prog filters interfaces. **Config**: globals named `UDEV_PROP_*` are filled from udev properties at load, or from `udev-hid-bpf add --property KEY=VALUE` (documented as the config route for user hacks). `udev-hid-bpf install foo.bpf.o` → `/etc/udev-hid-bpf/` + a rule | Runtime + packaging for a `.bpf.o`. Config is **load-time only**: a profile switch means reload, unless we add our own pinned map |
| Catalogue | `stable/` (upstreamed, distros should ship), `testing/`, `userhacks/` ("*user-subjective preference … will never be upstreamed*", e.g. `mouse_invert_y`, which negates a value in `hid_device_event`). Joystick-class entries: **FR-TEC Raptor Mach 2** (hat logical max 239→7, rdesc), **Thrustmaster TCA Yoke Boeing** (kills a bogus `ABS_MISC` axis, rdesc), Xbox Elite 2, SpaceNavigator, open MR !253 "SKT TM Evo wheel adapter Game Pad → Joystick" | Our transform is a **userhack**: never upstream, so we ship and load our own object. Templates for rdesc + event rewriting exist |
| In-tree [`drivers/hid/bpf/progs/`](https://github.com/torvalds/linux/tree/master/drivers/hid/bpf/progs) | 30 programs (the same stable set: Huion, XPPen, Wacom ArtPen, Mistel MD770, FR-TEC, TCA Yoke …). The README says to load them with udev-hid-bpf | Reference code, all `GPL-2.0-only` |
| Curves/DZ in HID-BPF with userspace maps | **None found.** Checked udev-hid-bpf issues/MRs ("deadzone", "curve", "joystick"), the in-tree progs and web search. Only the kernel doc's deadzone example (ringbuf to userspace) | We'd be first. No prior art means design risk. Must work within the verifier's rules: no floats, so a **LUT in an array map** (1024 entries for 10-bit X/Y, 256 for twist/throttle) |
| Distro kernels: `CONFIG_HID_BPF` | Fedora kernel-ark `=y`; Arch `=y`; Debian `debian/latest` (sid) `=y`. **Debian 13 trixie 6.12: not set** (salsa `debian/6.12/trixie` config, and confirmed on this machine's `/boot/config-6.12.107+deb13`). Ubuntu: *(unverified)* | A HID-BPF-only design **excludes Debian stable**, so it needs a uinput fallback |
| udev-hid-bpf packages | Arch 2.3.0, Debian sid 2.3.0, Fedora 40–44, Ubuntu 26.04/26.10, Gentoo (Repology) | Can be a dependency rather than vendored (GPL stays separate) |
| Bazzite / SteamOS | Bazzite's `Containerfile` references `input-remapper` and `inputplumber` but **not udev-hid-bpf or hhd**. SteamOS: nothing found | No gaming distro ships HID-BPF userhacks yet |
| Who-T posts | [udev-hid-bpf quickstart](https://who-t.blogspot.com/2024/04/udev-hid-bpf-quickstart-tooling-to-fix.html) (2024-04); [hidreport + hut crates](https://who-t.blogspot.com/2024/11/hidreport-and-hut-two-crates-for.html) (2024-11); [HIDIOCREVOKE in 6.12](https://who-t.blogspot.com/2024/10/hiocrevoke-merged-for-kernel-612.html) (logind can revoke hidraw fds, and an aside that HID-BPF could "firewall" devices); [libinput Lua plugins](https://who-t.blogspot.com) (2025-05; libinput ignores joysticks, so not applicable) | **`hidreport` + `hut` are MIT Rust crates**: parse the X3D Pro descriptor and extract fields in shared Windows/Linux code |

HID-BPF constraints to design around:
- **Loading needs root** (CAP_BPF/CAP_SYS_ADMIN). In practice the udev rule loads it.
- **Runtime reconfiguration** (per-app switching) means writing a pinned map. That needs a root helper, or bpffs pin permissions plus a kernel where the unprivileged `bpf()` check applies only to map/prog creation (≈6.5+) *(unverified)*.
- **Descriptor changes apply only at bind.** Adding shift-layer buttons or hat-as-buttons changes the layout, and rebinding looks like an unplug to running games. So fix one superset layout at bind and switch profiles through map data only.
- **Licence**: the `.bpf.c` must carry a GPL-compatible licence string (`"GPL"`, or `"Dual MIT/GPL"`). Upstream helpers are GPL-2.0-only, so ship it as a separately licensed GPL-2.0 file. The MIT app only loads it, which is plain aggregation.

### Sketch: what an X3D Pro HID-BPF userhack would need (design input, not code)

Assembled from the patterns above. Nothing here exists anywhere today.

- **Match**: `HID_BPF_CONFIG(HID_DEVICE(BUS_USB, HID_GROUP_GENERIC, 0x046D, 0xC215))`, plus a `probe()` that checks the descriptor size/signature, as `mouse_invert_y` does.
- **`hid_rdesc_fixup`**: emit one fixed superset descriptor.
  - Keep X/Y (10-bit), Rz (8-bit) and throttle (8-bit) at their native widths, so curves map 1:1 onto the native resolution.
  - Keep the 12 physical buttons, then add 12 "shifted" buttons and 4–8 hat-as-button bits.
  - Keep the native hat.
  - It is never changed at runtime.
- **`hid_device_event`**:
  - read the raw fields;
  - look up the axis LUTs (`BPF_MAP_TYPE_ARRAY`, `u16` values, one table per axis, so deadzone, curve, inversion and saturation are all baked into the LUT);
  - apply the button remap table and the shift bit (the shift button's state is kept in a global);
  - write the expanded report. The buffer is sized from the fixed descriptor, so growing the report is allowed *(confirm against the Huion/XPPen programs that change report layout)*.
- **Profile switch**: the MIT userspace daemon rewrites the maps atomically, e.g. two map slots plus an "active" index. No reload, no rebind, games see no replug.
- **Loading**: the distro's `udev-hid-bpf` plus our hwdb entry, or `udev-hid-bpf install`. The `.bpf.o` is built with clang at package-build time. Kernel ≥6.11 needs a struct_ops build; 6.3–6.10 needs a legacy build (udev-hid-bpf ships both).
- **Fallback when `CONFIG_HID_BPF` is missing** (Debian 13): run the same transform in the daemon on `evdev` input, output via uinput, and apply hhd-style hiding.

---

## 3. Flight-sim-specific Linux tooling

| Item | What exists | Notes / evidence |
|---|---|---|
| Vendor config tools (VKBDevCfg, Virpil VPC Configurator, Winwing SimAppPro) | **No native Linux versions.** Wine: [Bug 57030](https://list.winehq.org/hyperkitty/list/wine-bugs@list.winehq.org/thread/NGOXNUX3OTDHUAWIMID2X66ZE3DXYNEB/): "*VKBDevCfg-C detects device but no application functions available*" (missing USB IOCTLs). Forum consensus: configure on Windows, the firmware keeps the curves | Flight-stick owners on Linux tune curves **in-game** or with ControllerBuddy/input-remapper. There's a gap for a native GUI |
| Wine/Proton backend choice ([winebus `main.c`](https://gitlab.winehq.org/wine/wine/-/blob/master/dlls/winebus.sys/main.c)) | SDL is the default (when SDL inits, the evdev "input" bus is disabled). `is_hidraw_enabled()` **prefers hidraw** for DS4/DS5, ThrustMaster T-Rudder/TWCS/T.16000M, Simucube, Fanatec pedals, **all VKB (VID 0x231d)**, **all Virpil (0x3344)**, some Atmel/VPC, Winwing Orion, Nintendo. Extra devices via registry `EnableHidraw` or Proton `PROTON_ENABLE_HIDRAW=0xVID/0xPID` | Logitech `046d:c215` is **not** on the list, so it takes the SDL/evdev path by default and a uinput virtual device works. If a user enables hidraw for it, any uinput remap is bypassed. HID-BPF still applies |
| SDL knowledge of our stick ([SDL_joystick.c](https://github.com/libsdl-org/SDL/blob/main/src/joystick/SDL_joystick.c)) | `initial_flightstick_devices[]` contains `MAKE_VIDPID(0x046d, 0xc215) // Logitech Extreme 3D` | A virtual device with a **different VID/PID loses the flight-stick type** unless `SDL_JOYSTICK_FLIGHTSTICK_DEVICES` is set |
| SDL hints (SDL2 and SDL3) | `SDL_JOYSTICK_BLACKLIST_DEVICES=0x046d/0xc215` hides a device from SDL (and so from Wine-on-SDL) **without root**. `SDL_GAMECONTROLLER_IGNORE_DEVICES` covers the gamepad API only (Steam sets it for its virtual pads). `SDL_JOYSTICK_LINUX_DEADZONES` (SDL3; SDL2: `SDL_LINUX_JOYSTICK_DEADZONES`) defaults to **"0: Return unfiltered joystick axis values"**. `SDL_JOYSTICK_LINUX_CLASSIC` switches to joydev | A per-game Steam launch option hides the physical stick: a cheap fallback for people without root. It also explains why `evdev-joystick --deadzone` does nothing |
| `SDL_GAMECONTROLLERCONFIG` | Maps joysticks to the *gamepad* API ([Arch wiki](https://wiki.archlinux.org/title/Gamepad)) | Irrelevant for flight sims (they use the joystick/DInput API). Don't make our virtual stick look like a gamepad |
| Steam Input | Standard advice for HOTAS: "*Steam Input Per Game → Forced OFF*" so Steam doesn't add a translated Xbox pad (double inputs) | Document this. Our virtual device must not use `BTN_SOUTH…` codes, or SDL may auto-map it as a gamepad |
| [DCS-on-Linux wiki](https://github.com/ChaosRifle/DCS-on-Linux/wiki/Troubleshooting#joystick-issues) | "*joysticks report as xinput / only partially work … PID/VID has not explicitly been recognized*". Fix: `wine control` → Game Controllers → **Override**. Zero-button pedals get misclassified (as an accelerometer), and the suggested workaround is "*input remapper to add a phantom button to a cloned virtual device*". Buttons beyond ~80 are dropped (`KEY_MAX 0x2ff`) | Virtual-device identity and capability choices decide whether DCS sees a DInput joystick. Shift layer + hat-buttons (12×2+8 = 32) stay well under the limit |
| Wine deadzone | Registry `HKCU\Software\Wine\DirectInput\DefaultDeadZone` (0–10000, all axes) | Global, crude, DInput only |
| JoystickGremlin | [Windows-only](https://github.com/WhiteMagic/JoystickGremlin) (vJoy, PyWin32); the SC-Open fork too | No Linux equivalent of Gremlin/vJoy+HidHide except ControllerBuddy and MMVJ |
| "vJoy on Linux" | The kernel provides it: **uinput** (evdev only) and **uhid** (full HID incl. hidraw). USB/IP `vhci_hcd` gives a full USB device (InputPlumber's virtual-usb-rs) | No driver to install, which matches our HVCI-style constraint |

Takeaways for flight sims:
- The Linux flight-sim community (DCS-on-Linux matrix, hoggit wiki, Proton issue [#1722](https://github.com/ValveSoftware/Proton/issues/1722)) spends its effort on **getting devices seen at all**: udev rules, hidraw opt-in, joy.cpl overrides. Curves are configured in-game. There's no Linux-native tool aimed at budget sticks like the X3D Pro, so the niche is open.
- The SDL path is what matters for `046d:c215`. Supporting hidraw-path users needs HID-BPF or uhid, not uinput.
- Rebinding cost is real (DCS binds per device GUID and name). Keep the virtual identity stable across releases and, ideally, identical to the Windows virtual device.

---

## 4. GUI precedents and per-app switching on Linux

| Tool | GUI stack | Per-app mechanism | Wayland behaviour |
|---|---|---|---|
| input-remapper | GTK3 / PyGObject, pkexec for root parts | none; per-device preset + autoload | n/a (issue #1082 open: "*Wayland doesn't natively support retrieving the currently active window*") |
| SC-Controller | GTK4 + layer-shell | Window-title/class autoswitch via X11 | "*X11 only for now*" |
| AntiMicroX | Qt6 | Auto profiles by active window | "*not in Wayland*" |
| keyd / xremap / makima | none (CLI) | Per-compositor backends: KWin script, GNOME extension, wlr-foreign-toplevel, Hyprland/Niri IPC, COSMIC protocol, X11 | Works only where a backend exists. GNOME always needs an extension (makima: unsupported) |
| OpenGamepadUI + InputPlumber | Godot 4 | gamescope atoms `STEAM_GAME` / `GAMESCOPE_FOCUSED_APP` → D-Bus `LoadProfilePath` | gamescope session only |
| hhd | own UI + gamescope overlay | overlay-driven | gamescope-centric |
| ControllerBuddy | Swing + FlatLaf | manual profile load | n/a |
| MMVJ | egui | none | n/a |
| Tauri-based Linux input tools | **No mature one found** (only toy projects, e.g. LaptopControllerV2) | — | We'd be first. Tauri on Linux means WebKitGTK |

Focus/game detection options found in the wild, plus two **unused ideas**:
- X11 `_NET_ACTIVE_WINDOW` + `WM_CLASS`/`_NET_WM_PID` (x11rb). Most Proton games are XWayland windows, so reading the XWayland root could cover games on GNOME/KDE Wayland too *(unverified: depends on the compositor's X WM setting the atom; the Wine Wayland driver will erode this)*.
- KWin script over D-Bus (keyd, xremap); GNOME Shell extension exporting D-Bus (keyd, xremap: `com.k0kubun.Xremap.ActiveWindow`); `zwlr_foreign_toplevel_manager_v1` with `activated` state (sway, Hyprland, labwc, river); `zcosmic_toplevel_info_v1`; Hyprland/Niri IPC. The standard `ext-foreign-toplevel-list-v1` exposes app_id/title but **no activated state**.
- gamescope atoms (exact Steam AppID in Steam Deck / Bazzite game mode).
- **Idea, no tool found**: [GameMode](https://github.com/FeralInteractive/gamemode) D-Bus signals `GameRegistered(i pid, o path)` / `GameUnregistered`, plus a `Game.Executable` property. Users add `gamemoderun %command%` and get compositor-agnostic, opt-in game detection.
- **Idea, matches Windows "exe running" semantics**: scan `/proc/*/environ` for `SteamAppId` or the cmdline `reaper SteamLaunch AppId=N`. No focus needed, so it works on every Wayland compositor.

Takeaways for the GUI:
- Every mature tool splits a **privileged daemon** from an **unprivileged GUI**: input-remapper (D-Bus + pkexec), InputPlumber (D-Bus + polkit), keyd (socket + group), OpenTabletDriver (user daemon + udev uaccess). Our Tauri app should be the unprivileged GUI talking to a daemon. Tauri doesn't need root, and `zbus` gives it D-Bus.
- None of the surveyed GUIs can draw a live curve against real stick input on Wayland *without* the daemon relaying events. The GUI can read the virtual or physical node only while it isn't hidden or grabbed. input-remapper's "*Stop the injection to record input*" friction comes from exactly this. **Design**: the daemon streams raw and processed values to the GUI over D-Bus signals.
- Per-app support is always partial. Show the active detector and the matched app in the UI so users can see why a profile did or didn't switch. No surveyed tool does this.

---

## 5. Cross-cutting: how others avoid duplicate devices

| Technique | Who | Hides from evdev readers? | From hidraw readers? | Root? | Caveat |
|---|---|---|---|---|---|
| `EVIOCGRAB` only | input-remapper, evsieve, xboxdrv, SC-Controller, keyd, makima | events only; the **node still enumerates** as a dead joystick | ✘ | no (joystick nodes are already `uaccess` via systemd `70-uaccess.rules`: `ENV{ID_INPUT_JOYSTICK}=="?*", TAG+="uaccess"`) | Games list two sticks; DCS may bind the dead one |
| Runtime udev rule: `ID_INPUT_*=0`, MODE 000, `TAG-="uaccess"` on evdev + hidraw + hiddev, then re-trigger | hhd, InputPlumber | ✔ | ✔ | **yes** (writes `/run/udev/rules.d`) | Brief remove/add event. Must clean up on crash (InputPlumber `unhide_all`) |
| Move the node (`mv /dev/input/eventN /dev/inputplumber/sources/`) | InputPlumber (flag) | ✔ | ✔ | yes | Some tools rescan `/dev/input` via inotify |
| Revoke already-open fds (`EVIOCREVOKEALL`, custom) / `HIDIOCREVOKE` (6.12, logind) | hhd + Bazzite kernel / logind | ✔ | ✔ (HIDIOCREVOKE) | yes | Custom ioctl isn't upstream. HIDIOCREVOKE is per-fd, held by the opener |
| SDL env `SDL_JOYSTICK_BLACKLIST_DEVICES` | Steam (gamepad variant) | SDL/Wine-SDL only | ✘ | **no** | Per-game launch option. Fails if the virtual device reuses the same VID/PID |
| Blacklist/unbind the kernel driver, read via hidraw/libusb | OpenTabletDriver | ✔ | ✔ | yes | Impossible for a `hid-generic` device without collateral |
| **Modify in place with HID-BPF** | nobody (for curves) | n/a: only one device exists | n/a | load: yes | Kernel config (Debian 13 lacks it), novel |

---

## 6. Verdict

### Top 5 reusable things (licence vs our MIT)

1. **MIT/Apache Rust crates, direct dependencies**:
   - [`evdev`](https://crates.io/crates/evdev) (Apache-2.0 OR MIT; InputPlumber uses it) for read/grab/uinput.
   - [`hidreport`](https://crates.io/crates/hidreport) + [`hut`](https://crates.io/crates/hut) (MIT, by whot) to parse the X3D Pro report descriptor. The same code can serve the Windows driver-side tooling.
   - `udev` / `tokio-udev` (MIT) for hotplug; `zbus` (MIT) for D-Bus; `x11rb` (MIT/Apache); `wayland-client` + `wayland-protocols-wlr` (MIT).
   - [`aya`](https://crates.io/crates/aya) (MIT/Apache) or `libbpf-rs` (LGPL-2.1 OR **BSD-2**: take BSD) for loading BPF and writing maps.
   - Licence-compatible, no copyleft.
2. **xremap's `src/client/*` focus backends** (MIT, Rust; gnome, kde, x11, hypr, wlroots, niri, cosmic) and **keyd's application-mapper backends** (MIT; KWin script injection, the wlr-foreign-toplevel parser, the GNOME extension). Both can be vendored with attribution. Add the two unused ideas: GameMode D-Bus `GameRegistered` (BSD-3 daemon; we only talk D-Bus) and `/proc` scanning for `SteamAppId`. Together they give the closest match to Windows per-exe switching.
3. **The device-hiding recipe from hhd/InputPlumber**: runtime `/run/udev/rules.d` rule clearing `ID_INPUT_JOYSTICK`, MODE 000, `TAG-="uaccess"` on evdev **and** hidraw/hiddev, re-trigger, `unhide_all` on start and exit. Source is LGPL-2.1 / GPL-3.0, so **reimplement from the description** (a ~20-line rule template). Plus the **daemon shape**: root systemd `Type=dbus` service, a system-bus API, and polkit with a group rule (InputPlumber), or a socket plus a `sysusers.d` group (keyd, MIT).
4. **udev-hid-bpf as the runtime** (GPL-2.0; depend on the distro package, never vendor). Use the kernel's FR-TEC/TCA-Yoke/`mouse_invert_y` programs as templates for a GPL-2.0 `X3DPro.bpf.c`: superset descriptor at bind, LUT curves, deadzones, inversion and shift state in pinned maps, with the MIT daemon writing the maps. It's the only route where evdev, joydev **and hidraw** consumers all see one transformed device. Pair it with a **uinput fallback** for kernels without `CONFIG_HID_BPF`. The uhid route has an MIT reference in inputtino.
5. **Behaviour and UX references (ideas only, all GPL)**:
   - curve maths: input-remapper `deadzone`/`gain`/`expo`, ControllerBuddy dead zone + exponent + toggle/momentary modes, xboxdrv piecewise `resp:` curves and modifier-shift buttons;
   - evsieve `persist=reopen`, `create-link=` and systemd `Type=notify`;
   - MMVJ's live per-step signal graph;
   - joystickwake (MIT): hold an `org.freedesktop.ScreenSaver.Inhibit` while the stick is active.

### Top 5 pitfalls to design around

1. **Duplicate devices.** `EVIOCGRAB` leaves a dead but enumerable joystick (xboxdrv man page; input-remapper #1045 ships *two* virtual devices). Hidraw readers bypass grabs entirely: Wine's hidraw allowlist covers VKB/Virpil/Winwing/Thrustmaster, plus `PROTON_ENABLE_HIDRAW`, Steam, and SDL hidapi. Containerised SDL (Steam Runtime) falls back to sysfs probing, so hiding must strip permissions, not just udev tags. **Design**: one visible device, either HID-BPF in place or uinput plus hhd-style hide rules, and a crash-safe unhide.
2. **Per-app switching on Wayland has no standard API.** Every surveyed tool is X11-only (SC-Controller, AntiMicroX), per-compositor with GNOME needing an extension (keyd, xremap, makima), or gamescope-only (OpenGamepadUI). input-remapper still has none. **Design**: a pluggable detector stack (process/AppID scan → gamescope atoms → GameMode → compositor backends → X11/XWayland), with a manual/hotkey override always available.
3. **Kernel calibration knobs are ignored.** `jscal` only touches joydev. SDL ignores evdev `flat` by default (`SDL_JOYSTICK_LINUX_DEADZONES=0`). Wine's `DefaultDeadZone` is global and DInput-only. **Design**: transform the values ourselves (BPF LUT or daemon), and never rely on `EVIOCSABS`.
4. **Kernel-side delivery is fragile.** DKMS modules (xpadneo, xone) break on updates and need MOK signing under Secure Boot; the xone upstream stalled. HID-BPF needs ≥6.3 (struct_ops ≥6.11) **and** `CONFIG_HID_BPF`, which is missing in Debian 13's stock 6.12 kernel. It needs root to load, GPL code, no floats, and descriptor changes only at bind. **Design**: no kernel module; HID-BPF when available, uinput fallback otherwise; one fixed superset layout so profile switches never force a re-enumeration.
5. **Virtual-device identity, classification and permissions.**
   - A new VID/PID means rebinding in every sim, losing SDL's built-in flight-stick entry for `046d:c215`, and Wine's "*reports as xinput*" misclassification (DCS-on-Linux).
   - `BTN_SOUTH`-style codes invite SDL gamepad auto-mapping, and `KEY_MAX` drops buttons beyond ~80.
   - On permissions: uinput/uhid `uaccess`, or hhd's 0666 "FIXME" rules, let any session process inject input.
   - Flatpak can't install udev rules or system services (Oversteer, ControllerBuddy).

   **Design**:
   - Mirror the physical capabilities (`BTN_TRIGGER…BTN_BASE6`, ABS X/Y/RZ/THROTTLE, HAT0) and keep the same name/VID/PID as the Windows virtual device where possible.
   - Ship native packages (deb/rpm/AUR) carrying the udev, systemd and polkit pieces. Treat Flatpak as GUI-only on top of a host daemon.
   - Keep the root surface to one small daemon.

### What this implies for our architecture (one-paragraph summary)

The prior art points to one shape:
- an **unprivileged Tauri GUI**;
- a **small root daemon** (systemd `Type=dbus`, polkit group rule, hwdb/udev autostart on `046d:c215`) that owns device hiding, profile state and per-app detection;
- a **transform engine with two backends**:
  - preferred: a GPL-2.0 HID-BPF program with LUT maps, so one device is seen by evdev, joydev and hidraw alike;
  - fallback: evdev grab → uinput, plus hhd-style hide rules, for kernels without `CONFIG_HID_BPF`.

Per-app switching is a stack of detectors rather than one API. Nobody has shipped exactly this; every piece exists separately in the projects above.

---

## Sources (fetched 2026-10-04)

Repos and metadata (GitHub API: language, licence, `pushed_at`, latest release):
- https://github.com/sezanzeb/input-remapper (readme/usage.md, issues #1045, #1082, #324)
- https://github.com/KarsMulder/evsieve · https://github.com/jgeumlek/MoltenGamepad
- https://github.com/C0rn3j/sc-controller (`scc/uinput.py`, `scc/drivers/evdevdrv.py`) · https://github.com/kozec/sc-controller
- https://github.com/xboxdrv/xboxdrv (`doc/xboxdrv.1`: EVDEV OPTION, AXIS FILTER `resp`, D-Bus interface)
- https://github.com/AntiMicroX/antimicrox · https://github.com/panzi/qjoypad · https://codeberg.org/forestix/joystickwake
- https://manpages.debian.org/trixie/joystick/evdev-joystick.1.en.html
- https://github.com/atar-axis/xpadneo · https://github.com/dlundqvist/xone · https://github.com/medusalix/xone
- https://github.com/rvaiya/keyd (`scripts/keyd-application-mapper`, `data/sysusers.d`, Makefile)
- https://github.com/jtroo/kanata · https://github.com/philipl/evdevremapkeys · https://github.com/wez/evremap
- https://github.com/xremap/xremap (`Cargo.toml` features, `src/client/`)
- https://github.com/cyber-sushi/makima
- https://github.com/ShadowBlip/InputPlumber (`docs/usage.md`, `rootfs/usr/lib/udev/*`, `rootfs/usr/share/polkit-1/*`, `src/udev/mod.rs`, `Cargo.toml`, profile schema)
- https://github.com/ShadowBlip/virtual-usb-rs · https://github.com/ShadowBlip/gamescope-x11-client (`src/atoms.rs`)
- https://github.com/hhd-dev/hhd (`readme.md`, `src/hhd/controller/lib/hide.py`, `ioctl.py`, `usr/lib/udev/rules.d/83-hhd-user.rules`)
- https://github.com/ShadowBlip/OpenGamepadUI
- https://github.com/bwRavencl/ControllerBuddy (`AxisToAxisAction.java`, `UinputDevice.java`)
- https://github.com/WhiteMagic/JoystickGremlin · https://github.com/SC-Open/JoystickGremlin
- https://github.com/leosat/MMVJ · https://github.com/games-on-whales/inputtino
- https://github.com/OpenTabletDriver/OpenTabletDriver · https://opentabletdriver.net/Wiki/Documentation/RequiredPermissions
- https://github.com/berarma/oversteer
- https://github.com/FeralInteractive/gamemode (`daemon/gamemode-dbus.c`)
- https://github.com/ublue-os/bazzite (`Containerfile`)
- https://github.com/ValveSoftware/steam-devices (`60-steam-input.rules`: uinput `uaccess`)
- https://github.com/systemd/systemd (`rules.d/70-uaccess.rules.in`: joysticks `uaccess`)

HID-BPF:
- https://gitlab.freedesktop.org/libevdev/udev-hid-bpf (README, `81-hid-bpf.rules.in`, `doc/how-it-works.rst`, `doc/udev-properties.rst`, `doc/device-matches.rst`, `src/bpf/{stable,testing,userhacks}`, issues/MRs search)
- https://docs.kernel.org/hid/hid-bpf.html
- https://github.com/torvalds/linux/tree/master/drivers/hid/bpf/progs (README)
- `drivers/hid/hid-core.c`, `drivers/hid/hidraw.c`
- https://who-t.blogspot.com/2024/04/udev-hid-bpf-quickstart-tooling-to-fix.html
- https://who-t.blogspot.com/2024/11/hidreport-and-hut-two-crates-for.html
- https://who-t.blogspot.com/2024/10/hiocrevoke-merged-for-kernel-612.html
- Kernel configs: https://gitlab.archlinux.org/archlinux/packaging/packages/linux (`config.x86_64`) · https://gitlab.com/cki-project/kernel-ark (`redhat/configs/common/generic/CONFIG_HID_BPF`) · https://salsa.debian.org/kernel-team/linux (`debian/latest` vs `debian/6.12/trixie`, `debian/config/config`)
- Local check: `/boot/config-6.12.107+deb13-amd64`

Flight sim / Wine / SDL:
- https://gitlab.winehq.org/wine/wine/-/blob/master/dlls/winebus.sys/main.c (`is_hidraw_enabled`, bus init)
- https://github.com/libsdl-org/SDL (`include/SDL3/SDL_hints.h`, SDL2 `include/SDL_hints.h`, `src/joystick/SDL_joystick.c`)
- https://github.com/ChaosRifle/DCS-on-Linux/wiki/Troubleshooting
- https://axel.hajslunddamgaard.dk/2026/03/28/128-buttons.html
- https://list.winehq.org/hyperkitty/list/wine-bugs@list.winehq.org/thread/NGOXNUX3OTDHUAWIMID2X66ZE3DXYNEB/
- https://wiki.archlinux.org/title/Gamepad (raw wikitext)

Packaging: https://repology.org (udev-hid-bpf, input-remapper, inputplumber, evsieve, hhd, sc-controller, antimicrox, keyd, kanata, xremap, makima, joystickwake, joystick, oversteer, steam-devices). Crate licences: https://crates.io (evdev, evdev-rs, input-linux, hidreport, hut, udev, tokio-udev, zbus, x11rb, wayland-client, wayland-protocols-wlr, aya, libbpf-rs, hidapi).
