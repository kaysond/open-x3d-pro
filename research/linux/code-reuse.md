# Linux port: code reuse, rewrite scope and effort (A = HID-BPF, B = evdev grab + uinput)

Scope: what in the monorepo survives a Linux port, what each architecture adds, and what it costs. Another agent judges the
technical merit of A vs B; this file sizes the code. Everything below was checked against the source at the time of writing
(commit on `main`, 2026-10-04). Facts marked **measured** were observed on the Debian 13 dev box (kernel 6.12.107+deb13).

## 0. Findings that change the plan

1. **Debian 13's stock kernel has `# CONFIG_HID_BPF is not set`** (measured, `/boot/config-6.12.107+deb13-amd64`). A cannot run
   on the dev box's own distro kernel. Kernel-config coverage on other distros is the other agent's job, but A needs a fallback
   anyway. That fallback is B or read-only mode.
2. **Evdev is not lossless as delivered.** `hid-input` sets `fuzz = (max-min)>>8`, `flat = (max-min)>>4` for Joystick/Gamepad
   application axes (verified in `drivers/hid/hid-input.c`). For X/Y (0..1023) that is fuzz 3, so the input core's defuzz drops or
   smooths changes of 1–2 LSB. Rz/Slider (0..255) get fuzz 0. `hid-lg` (which binds 046d:c215 with `LG_NOGET`) exempts only wheels.
   B must reset fuzz to 0 with `EVIOCSABS` on the grabbed node, and then the evdev values are exact.
3. **The exact-rational integer pipeline works, and Q16.16 does not.** A u64-only port of `pipeline.c` (§2) passes the unmodified
   `driver/test` harness: 18 vectors, 360 cases, 0 failures. A fuzz run over 200 000 random valid configs × 64 random reports compared
   it with the double C pipeline: of 51.2 M axis outputs, 1 647 differ, every one by exactly 1 (tie-breaks at .5), and hat/buttons
   never differ. Prototype and fuzzer: `$TMPDIR/x3dint/`, scratch only, not committed.
4. **The bpffs mount is `drwx-----T`** (measured, mode 0700 as systemd mounts it). A non-root app can never reach a pinned map, whatever
   the pin's own permissions. The config channel for A should therefore be a **hidraw feature report intercepted by
   `hid_bpf_ops.hid_hw_request`**, which is the same wire format as Windows report 3/4.
5. **Default permissions** (measured, `/usr/lib/udev/rules.d/70-uaccess.rules`): evdev joysticks get `uaccess`
   (`ENV{ID_INPUT_JOYSTICK}=="?*"`). hidraw for joysticks and `/dev/uinput` get none. Phase 0 (read evdev) therefore needs no
   packaging or root. B needs one uinput rule. A needs a hidraw rule plus the BPF loader.

## 1. Module-by-module matrix

Legend: **AG** = OS-agnostic as-is · **AG\*** = agnostic but with a hidden Windows assumption (noted) · **SIB** = needs a
`cfg(target_os = "linux")` sibling · **WIN** = Windows-only, not needed on Linux · **NEW** = does not exist yet.

| File | Class | Linux notes |
|---|---|---|
| `core/src/config.rs` | AG | Serde types, validation. No paths. |
| `core/src/blob.rs` | AG | A sends the 240-byte blob byte-for-byte as feature report 3. B uses `CompiledConfig` in-process. |
| `core/src/pipeline.rs` | AG | B calls `process()` unchanged. `RawReport::to_bytes()` (already `pub`) rebuilds the 7-byte report from evdev values. |
| `core/src/hid.rs` | AG (+NEW for A) | B uses `VID`/`PID` only. A needs `REPORT_DESCRIPTOR_BPF` (§2.5) and a ~10-line parser for the combined report. `DriverInfo`, `config_report` and `VendorInput` layouts are reused. |
| `core/src/vectors.rs`, `bin/gen-vectors.rs`, `testdata/vectors.json` | AG | Drive the BPF host test (§2.6) and B's uinput end-to-end test (§6.5). |
| `driver/pipeline/pipeline.{c,h}` | WIN today | Doubles are fine in UMDF user mode. A needs an integer core (§2). Single-source recommendation in §2.7. |
| `driver/test/{test_pipeline.c,Makefile}` | AG | The harness only calls the 4-function API, so it links the integer core unchanged (proven in scratch). `check-descriptor` grows a third copy for A. |
| `driver/openx3d/*`, `driver/scripts/*.ps1`, INF, `.rc` | WIN | Not needed. A copies `descriptor.h` bytes into the BPF program. |
| `app/src-tauri/src/calibration.rs` | AG | Works on `RawReport::raw16()`. |
| `app/src-tauri/src/state.rs` | AG | Shape unchanged; Linux semantics of `driver_installed` below. |
| `app/src-tauri/src/switcher.rs` | AG\* | `normalize()` lowercases and maps `/`→`\`, so Unix paths compare as `\usr\bin\foo` and Wine paths (`Z:\home\…\Game.exe`, `C:\…`) as-is. `basename()` is right for both. Hidden assumptions: case-insensitive matching on a case-sensitive FS (false positive only for case-only differences, accepted); a literal `\` in a Linux filename splits wrongly (negligible); generic Linux basenames (`java`, `python3`, `Main`) over-match through the basename fallback, so users should store full paths for those. `take_push`/CRC logic is reused by A. |
| `app/src-tauri/src/profiles.rs` | AG | `write_atomic` (write + `rename`) is atomic on POSIX. Only the doc comment says `%APPDATA%`. Key validation goes through `keys::scan_code`, so it works with either key column. |
| `app/src-tauri/src/settings.rs` | AG | Same. |
| `app/src-tauri/src/keys.rs` | AG\* + SIB | `Chords` is agnostic. The `KEYS` values are Windows set-1 scan codes with an `0xE0xx` extended convention. Of the 117 entries, 85 equal Linux `KEY_*` codes (Esc…F12, 0x01–0x58) and 32 differ: Pause (Windows 0x45 is Linux `KEY_NUMLOCK`), F13–F24 (Linux 183–194; Windows 0x64 = Linux `KEY_RIGHTALT`) and all 19 `0xE0xx` keys. Needs a third column plus a uinput-keyboard `send()`. |
| `app/src-tauri/src/device.rs` | AG + SIB | `InputFrame`, `fallback_frame` and `Coalescer` are agnostic. `imp` (hidapi Col01/Col02, `windows-native`) is Windows-only. Linux needs an evdev input path (phase 0 / B) and, for A, a hidraw path that mirrors `imp` closely. |
| `app/src-tauri/src/watcher.rs` | WIN (logic reusable) | `foreground_worker`, `scan_worker` and the debounce are agnostic except for calls to `process_path()` and `snapshot()`. Move those two plus the hook into per-OS files and share the workers. Linux sibling in §4.1. |
| `app/src-tauri/src/tray.rs` | AG\* | Compiles on Linux (libayatana-appindicator at runtime). `tooltip` and `show_menu_on_left_click` are unsupported there (Tauri docs), and the left-click handler never fires, but the existing "Open" menu item covers it. On GNOME without the AppIndicator extension the icon is invisible, while autostart uses `--minimized`, so the app becomes unreachable (§4.3). |
| `app/src-tauri/src/commands.rs` | AG | Only the cfg gate widens. `list_windows` → watcher sibling. `apply_autostart` → the plugin writes `~/.config/autostart/*.desktop`. |
| `app/src-tauri/src/lib.rs` | AG\* | Gates widen to `any(windows, target_os="linux")`. `path().data_dir().join("OpenX3DPro")` resolves to `$XDG_DATA_HOME/OpenX3DPro` (≈`~/.local/share/OpenX3DPro`). That is **not** `app_data_dir` (`…/dev.kaysond.openx3dpro`). Fine as-is; document it in CONTRACT §3. Single-instance uses D-Bus on Linux and works. |
| `app/src-tauri/src/main.rs`, `build.rs` | SIB (cfg only) | `cfg(not(windows))` exit → `not(any(windows, linux))`. `tauri_build::build()` gate likewise. `windows_subsystem` is inert on Linux. |
| `app/src-tauri/Cargo.toml` | SIB | Move `tauri`, the plugins, `env_logger` and `tauri-build` to `cfg(any(windows, target_os="linux"))`. Linux adds `evdev = "0.13"` (pure Rust) and `x11rb` (pure-Rust X11). A adds `hidapi` with `linux-native-basic-udev` in its own target table. |
| `app/src-tauri/tauri.conf.json` | AG + NEW sibling | Keep as-is. Add `tauri.linux.conf.json` (merge patch): `bundle.targets` deb/rpm/appimage, `resources: []` (no driver), `bundle.linux.deb.{depends,files,postInstallScript}`, rpm equivalents. |
| `app/src-tauri/capabilities/default.json` | AG | — |
| `app/src-tauri/windows/hooks.nsh` | WIN | Linux counterpart: `app/src-tauri/linux/` with udev rules and postinst (NEW). |
| `app/src/lib/ipc.ts`, `app/src/types.ts` | AG | Contract unchanged (see below). |
| `app/src/lib/{mock,pipeline}.ts` | AG | — |
| `app/src/pages/{SettingsPage,ProfilesPage}.tsx`, `App.tsx` | AG\* | Windows copy: the "Code signing… Windows asks once" card, `placeholder="C:\Games\game.exe"`, and the "driver" wording. Gate on `navigator.userAgent.includes('Linux')`; no contract field needed. |
| `.github/workflows/ci.yml` | extend | `rust-linux` must install the WebKitGTK stack now that the shell compiles on Linux. New uinput e2e job (B) and BPF build job (A). |
| `.github/workflows/build.yml`, `nightly.yml`, `release.yml` | WIN + extend | Add a Linux bundle job. SignPath does not apply to Linux, but the `release.yml` `check` job requires SignPath secrets and would also block Linux releases unless that gate is split. |

**Contract impact.**
- `InputFrame`, all §5 commands and events, and §3 types stay **unchanged** on Linux. In B, `seq` is an app-side counter (as in today's
  fallback path). In A, it comes from the report tail.
- `AppState.driverInstalled` keeps one meaning on every OS: *games see processed output*.
  - Phase 0: `false`. The existing "no driver (preview only)" text stays correct.
  - B: `true` once EVIOCGRAB and the uinput device are both up. `driverVersion` = app version plus a `uinput` suffix.
  - A: `true` when the fixed descriptor is detected. `driverVersion` comes from feature report 4.
- `preview_config`:
  - Phase 0: affects only the in-app processed view, exactly as CONTRACT §5 says for "driver absent".
  - B: affects games immediately, because the device loop reads `switcher.compiled()` on every report.
  - A: `take_push()` → SET_FEATURE 3, the same code as Windows.
- CONTRACT §3 (paths), §6 (Linux device access, udev rules, packaging) and §8 (CI lanes) need Linux paragraphs.

## 2. Pipeline port for HID-BPF (A)

### 2.1 Why not Q16.16

BPF has no FPU but does have native 64-bit ALU and unsigned 64-bit `DIV`. Q16.16 quantises `m'` to 2⁻¹⁶. Two things amplify that error:
- the deadzone rescale, `1/(1−dzc−dzEnd)`, up to **1000×** (validation allows a 1-permille live range);
- the LUT slope, up to 65535 per 1/16 of input (sensitivity 4 saturates in one segment), which is about **16 LSB** of output per Q16 quantum.

Q16.16 therefore breaks the ±1 contract in corner cases. Use **exact rational arithmetic in u64**: carry every quantity as a
numerator over one denominator and divide once at the end. The result is the mathematically exact rounding of the reference
formula. The only deviation from the float reference is the tie-break at exact .5, which is ≤ 1 LSB (confirmed by the fuzz run).

### 2.2 Formulas (per axis; all unsigned, no signed division anywhere)

```text
inputs: r = raw16 (≤ 65535), mn/c/mx = cal (u16), dzc/dzmin/dzmax permille (≤ 500, pairwise sum ≤ 999), lut[17] u16

lerp(n, dn):            # t·dn for m' = n/dn, 0 ≤ n ≤ dn
  i  = min(16·n / dn, 15)            # u64 div
  rr = 16·n − i·dn                   # 0..dn  (= dn only when n = dn)
  P  = lut[i]·(dn − rr) + lut[i+1]·rr      # convex combination, P ≥ 0, P ≤ 65535·dn

SYMMETRIC:
  neg = (r ≤ c);  num = neg ? c−r : r−c;  den = neg ? c−mn : mx−c;  num = min(num, den)     # clamp |v| ≤ 1
  if invert: neg = !neg
  dze = neg ? dzmin : dzmax
  if 1000·num ≤ dzc·den: out = 32768                                           # m ≤ dzc, exact compare
  N  = 1000·num − dzc·den;  Dn = den·(1000 − dzc − dze);  N = min(N, Dn)
  P  = lerp(N, Dn)
  out = neg ? (65537·Dn − P) / (2·Dn) : (65537·Dn + P) / (2·Dn);  out = min(out, 65535)
        # = floor(32768 ∓ P/(2Dn) + ½) = round-half-up of 32768 ∓ t/2

FULL:
  den = mx − mn;  num = clamp(r − mn, 0, den);  if invert: num = den − num
  N  = sat0(1000·num − dzmin·den);  Dn = den·(1000 − dzmin − dzmax);  N = min(N, Dn)
  out = min((2·lerp(N, Dn) + Dn) / (2·Dn), 65535)
```

Buttons, hat and target slots are plain integer code already (`pipeline.c` lines 126–172) and port verbatim.

### 2.3 Overflow analysis (worst case, validated config)

| Quantity | Max | Bits |
|---|---|---|
| `den`, `num` | 65 535 | 16 |
| `1000·num`, `N`, `Dn` | 65 535 000 | < 2²⁶ |
| `dzc·den` | 32 767 500 | < 2²⁵ |
| `16·N` | 1 048 560 000 | < 2³⁰ |
| `P` | 65 535 · 65 535 000 ≈ 4.29·10¹² | < 2⁴² |
| `65537·Dn + P` / `2P + Dn` | ≈ 8.6·10¹² | < 2⁴⁴ |
| divisor `2·Dn` | 131 070 000 | < 2²⁷ |

Everything fits u64 with ≥ 20 bits of headroom. `P` needs 42 bits, so a u32 path is impossible without precomputed reciprocals,
and those are not worth adding. There are two divisions per axis, both u64 unsigned. That avoids `BPF_SDIV`, which needs
`-mcpu=v4` and kernel ≥ 6.6, and clang refuses signed division for older CPU targets. `Dn ≥ den ≥ 1` after validation. BPF
defines `x/0 = 0` without trapping, so a corrupt blob cannot crash the program, but validate anyway (§2.4).

### 2.4 BPF program shape, loops, state and config channel

- **Loops.** All bounds are constant: 4 axes, 12 buttons, 17-point LUT indexed by the clamped `i ≤ 15`, and 228×8 CRC iterations on
  config load. All are well under verifier limits. The verifier forces the range guards the C code already has (`t < 4`,
  `map[i] ≤ 32`, `shift ≤ 12`, `hat ≤ 7`).
- **Stack.** Work through a pointer into the map value. Never copy the 240-byte struct onto the 512-byte BPF stack. All u16 fields
  are at even offsets (axis base 12 + 50·i), so packed access is aligned.
- **State map.** Use `BPF_MAP_TYPE_ARRAY`, `max_entries = 1`, i.e. a `.bss` global:
  `struct { X3D_CONFIG cfg[2]; u32 active; u16 seq; u8 raw_last[7]; u8 flags; }` (~500 B). An ARRAY over a HASH because the key is
  fixed, the value is preallocated, nothing allocates in the IRQ path, and the lookup cannot fail. `cfg[2]` plus the `active` flip
  (write the inactive slot, then a single aligned u32 store) prevents torn reads between `hid_hw_request` (process context) and
  `hid_device_event` (IRQ context). Initialise identity in `hid_rdesc_fixup`, which runs once at probe.
- **Config channel (pick).** The app writes **feature report 3 via hidraw** (`HIDIOCSFEATURE`) and reads report 4 (and report 3
  readback). `hid_bpf_ops.hid_hw_request` intercepts them: positive return = handled, buffer filled for GET, never forwarded to
  the device, which matters because the device stalls on GET (CONTRACT header, kernel `LG_NOGET`). On SET 3: magic, version, size,
  CRC32 and field ranges (as `blob::parse`), then copy into the inactive slot and flip. Reject every other report number so nothing
  reaches the device.
  - Permission is one udev rule: `SUBSYSTEM=="hidraw", ATTRS{idVendor}=="046d", ATTRS{idProduct}=="c215", TAG+="uaccess"`.
  - The app needs **no CAP_BPF**.
  - Rejected alternative, a pinned map: since 6.5 `unprivileged_bpf_disabled` gates only `MAP_CREATE`/`PROG_LOAD`, so
    `BPF_OBJ_GET` plus `MAP_UPDATE_ELEM` on a pin is file-permission-governed. But bpffs is 0700 (measured), so a user never
    reaches the pin. This box also has `unprivileged_bpf_disabled=2`.
- **No persistence before login.** The program starts with identity at plug time, and the app pushes on connect, as today (`set_device_crc` + `take_push`).
  Plug-time preload would need a root-written udev property (`UDEV_PROP_*` globals, which udev-hid-bpf fills at load). Not worth it.
- **Detecting A from the app.** Never send GET_FEATURE 4 to a device that might be stock (it stalls). Detect the fixed descriptor
  first, via hidapi `get_report_descriptor()` / sysfs `report_descriptor`, or via a per-TLC enumeration that shows usage page
  0xFF00. The second option is the same test `device.rs::open()` uses on Windows. Whether hidapi's Linux backend lists one entry
  per top-level collection needs checking.

### 2.5 Report growth and the raw channel

`hid_device_event` receives the 7-byte unnumbered report and returns a larger size (positive return = new size; must be ≤
`ctx->allocated_size`). **Dependencies for the kernel agent:** `allocated_size` ≥ 24 for input events (sized from the fixed
descriptor?), `hid_bpf_get_data(ctx, 0, 24)` with a constant size, and the minimum kernel version where the `hid_hw_request`
struct_ops member actually loads (a CFI-stub bug made it fail initially).

Because the rewrite happens in place, the raw report is gone for every consumer (hidraw and evdev both see the rewritten report).
Options:

| Option | Verdict |
|---|---|
| **Append the raw tail to report 1**: `[1][x y rz slider u16][hat|pad][buttons u32]` + `[raw 7][seq u16][flags u8]` under Usage Page 0xFF00 = 24 B | **Pick.** One read gives `InputFrame.raw` + `processed`. `hid-input` maps page 0xFF00 to `HID_UP_MSVENDOR` → `goto ignore` (verified), so games' evdev axis count is unchanged. Collection 2 keeps only feature reports 3/4. |
| Separate input report 2 (Windows layout) | Needs two reports per device event. `hid_bpf_try_input_report` is documented for syscall/sleepable progs; re-entry from `device_event` is unproven. No. |
| BPF ringbuf | Needs map-fd access, blocked by bpffs 0700. No. |
| Read the physical evdev/hidraw | Impossible (already rewritten). |

`core::hid` gains `REPORT_DESCRIPTOR_BPF` (Windows bytes, report 1 extended, input report 2 removed) plus a parser that returns
`(JoyReport, VendorInput)`. The existing `walk()` test covers it with one more call, and `check-descriptor` diffs the BPF copy.
Note: the processed device gets `fuzz = 65535>>8 = 255` from `hid-input`, which drops |Δ| < 128 (about 2 raw LSB at identity, the same
relative filtering as stock). Ship a hwdb `EVDEV_ABS_00/01/05/06` override with fuzz 0.

### 2.6 Host test reuse

`driver/test/test_pipeline.c` is API-only. Linking it against the integer core needs **zero harness changes** (done in scratch:
`cc test_pipeline.c pipeline_int.c` → 360/360). On Linux CI: `make -C driver/test` builds both variants, or only one if §2.7 is
adopted. Optional `make fuzz` (int vs double, about 3 s) only while both exist.

### 2.7 Should Windows switch to the integer pipeline? Yes, but only when A starts

Put the math in `driver/pipeline/pipeline_math.h` (`static inline`, fixed-width ints only, no libc). Include it from `pipeline.c`
(UMDF + host test) and from the `.bpf.c`, and delete the double code and `ratio()` special-casing.

- Why: one C source of truth for both device-side implementations, and closer to the reference than doubles (ties only). The API is
  unchanged, vectors already pass, and the driver has never run on hardware, so there is no field regression to fear. MSVC handles
  `uint64_t` division fine.
- Why not now: without A there is no second consumer, so the swap is churn.

## 3. Userspace daemon (B) in Rust

### 3.1 Crates

| Need | Crate | Notes |
|---|---|---|
| evdev read, EVIOCGRAB, uinput out | `evdev 0.13.2` (Sep 2025, pure Rust) | `VirtualDevice::builder()?.name().input_id().with_keys().with_absolute_axis(&UinputAbsSetup)…build()`. If no `EVIOCSABS` wrapper exists, a 10-line raw ioctl on `as_raw_fd()`. Alternatives: `evdevil`, `input-linux`. |
| poll with timeout | `libc` (already transitive) | `poll()` 10 ms on the fd; the coalescer must tick on timeouts. **No tokio.** |
| hidapi | **not used by B** | Only A needs hidraw. |
| udev | not needed | Enumerate `/dev/input/event*` and match `input_id()` vendor 0x046d / product 0xc215, 1 s reconnect poll as in `device.rs`. |

### 3.2 Input: evdev, not hidraw (recommendation)

- **evdev (pick).** Joystick event nodes already have `uaccess`. We must open the node anyway for EVIOCGRAB. After `EVIOCSABS` fuzz=0
  the values are exact:
  - X/Y 0..1023, Rz `ABS_RZ` 0..255, Slider `ABS_THROTTLE` 0..255 (hid-input maps usage 0x36 → code 6);
  - buttons 1–12 → `BTN_TRIGGER`(0x120)…`BTN_BASE6`(0x12b);
  - hat → `ABS_HAT0X/Y` ∈ {−1,0,1}; the reverse of hid-input's `hid_hat_to_axis` table gives 0..7, (0,0) → 8 (null states
    8–15 collapse to 8, as the pipeline does).

  At each `SYN_REPORT`, build `RawReport{..}.to_bytes()` → `x3d_core::pipeline::process(s.switcher.compiled(), &bytes)`. Core is
  unchanged. Handle `SYN_DROPPED` by re-reading abs/key state (the crate caches state; confirm in the spike).
- **hidraw (rejected for B).** It gives the exact 7 bytes, and `device.rs`'s fallback branch is nearly reusable. But it needs a hidraw
  `uaccess` rule, and that rule also lets Wine/Proton's winebus hidraw backend open the **physical** device and bypass the grab. B
  wants the physical hidraw left root-only. This is a policy conflict with A, which needs hidraw readable (§5.3).

### 3.3 Output device: mirror the Windows descriptor exactly as hid-input would map it

| Field | Setup |
|---|---|
| Axes | `ABS_X`, `ABS_Y`, `ABS_RZ`, `ABS_THROTTLE`: 0..65535, **fuzz 0, flat 0** (better than A's hid-input default) |
| Hat | `ABS_HAT0X/Y` −1..1, from the 0..8 table above |
| Buttons 1–32 | 1–16 → 0x120…0x12f (`BTN_TRIGGER`…`BTN_DEAD`); 17–32 → `BTN_TRIGGER_HAPPY1..16` (0x2c0…0x2cf). This is hid-input's `HID_GD_JOYSTICK` rule (verified), so A and B present identical capability sets. |
| ID | `BUS_USB`, 046d:c215, version = physical `bcdDevice`, name = physical name. SDL GUIDs and Proton DInput GUIDs then match bindings users made on the stock device. The downside is a silent twin (the grabbed physical) with the same GUID; mitigate with udev (§5.2). One string either way. |

The loop stays a **sync thread exactly like `device.rs::session`**:
`poll(10 ms) → read events → on SYN: rebuild raw, lock, calibration.update, process, chords.update → uinput emit (+SYN) → coalescer → emit input`.
Added latency is one wake plus two syscalls, about 20–100 µs against a 10 ms device poll (1 ms on the full-speed revision).
CPU use is negligible (≪ 0.1 % of a core at 100–1000 Hz). The lock is held for microseconds per report, the same pattern as today.

### 3.4 Where B lives: inside the Tauri process (B1), with a seam to split later (B2)

- **B1 (recommend for v1).** The device thread inside the tray app, as on Windows today.
  - No IPC.
  - The switcher is in-process.
  - `preview_config` works for free.
  - Failure is benign: if the app dies, the grab is released and the uinput device is destroyed, so games fall back to the raw stick.
  - Needs `/dev/uinput` access for the GUI user, which key bindings need anyway.
- **B2 (split when GUI-lifetime coupling bites).** `openx3d-daemon` with a systemd `--user` unit and a Unix socket at
  `$XDG_RUNTIME_DIR/openx3d.sock`; the app becomes a client.
  - The daemon *is* the Linux "driver", so the protocol is the Windows HID channel over a socket, and the `x3d_core::hid`
    parsers are reused verbatim:

    | Direction | Message | Windows analogue |
    |---|---|---|
    | client→daemon | `[3]+255 B` set config (from `switcher.take_push`) | SET_FEATURE 3 |
    | client→daemon | `[4]` → `[4]+31 B` info | GET_FEATURE 4 |
    | daemon→client | `[1]+13 B` processed, `[2]+10 B` raw/seq/flags per report | Col01/Col02 input |

  - **No CONTRACT §5 command moves to the daemon.** Profiles, settings, switching, `list_windows`, calibration (fed by the raw
    stream), import/export and keys all stay in the app, because they are per-user and per-session (foreground window, uinput keyboard).
  - A system service (not `--user`) would also allow hiding the physical node (root-only via udev) and applying config at boot.
    The cost is socket auth plus a privileged service. Accept only the 240-byte blob there, never key bindings, which would be a
    keystroke-injection vector.
  - Cost: +3/+5 days (protocol both sides about 300 lines, second binary, unit, packaging).

## 4. Shared parts needing change for both A and B

### 4.1 `watcher.rs` Linux sibling

- **Foreground.** `x11rb`: watch `PropertyNotify` on root `_NET_ACTIVE_WINDOW` → `_NET_WM_PID` → pid → the existing channel and
  250 ms debounce worker. This works on X11 and for XWayland windows under Wayland; for native-Wayland windows it is None → default.
  Most Proton and SDL games are XWayland. There is no generic Wayland API; GNOME/KDE/sway-specific backends are out of scope.
- **Exe identity**, `process_path(pid)`:
  1. If the `readlink /proc/pid/exe` basename is `wine`, `wine64`, `wine-preloader` or `wine64-preloader`, use argv[0] from
     `/proc/pid/cmdline`. Wine rewrites it to the Windows path, `C:\…\DCS.exe` or `Z:\home\…\DCS.exe`.
  2. Otherwise use the `/proc/pid/exe` target. Inside Steam pressure-vessel the path may be container-relative, but the basename is still right.
- **Running mode.** Scan `/proc` every 2 s; only own-uid processes are readable, which covers games. `snapshot()` must return the
  **Windows basename for Wine processes**, otherwise `scan_worker`'s prefilter never matches. Do not use `/proc/pid/comm`: it is
  truncated to 15 chars.
- **`list_windows`.** `_NET_CLIENT_LIST` + `_NET_WM_PID` + `_NET_WM_NAME` (X11/XWayland only); fall back to own-uid `/proc`
  entries. Payload unchanged. Profiles become cross-platform: a Windows full path still matches under Proton through the basename fallback.

### 4.2 `keys.rs` → uinput keyboard

Add a Linux column to `KEYS` (32 entries differ, §1), then create one uinput keyboard (all keys in the table) lazily on first
`send()`. A separate device from the joystick, so libinput and SDL classify both correctly. Works on X11 and Wayland, and is
layout-independent because `KEY_*` codes are positions, like scan codes. Permission: the same uinput rule as B. Steam's
`steam-devices` ships the identical rule, so many gamers already have it.

### 4.3 Tray, paths, settings

- **Tray.** Package dependency `libayatana-appindicator3-1`. On GNOME without the AppIndicator extension, skip `--minimized` (or
  open the window when no StatusNotifierWatcher is on D-Bus), otherwise autostart leaves the app invisible.
- **WebKitGTK on NVIDIA.** Blank-window reports are common; set `WEBKIT_DISABLE_DMABUF_RENDERER=1` before Tauri init if needed.
- **Paths.** `~/.local/share/OpenX3DPro/{settings.json,profiles/}` via `data_dir()`. Fine, no code change. Strict XDG would put
  settings in `~/.config`; not worth diverging from Windows.

### 4.4 Packaging (`tauri.linux.conf.json` + `app/src-tauri/linux/`)

| Item | B | A |
|---|---|---|
| deb/rpm `files` → `/usr/lib/udev/rules.d/70-open-x3d-pro.rules` | `KERNEL=="uinput", SUBSYSTEM=="misc", TAG+="uaccess", OPTIONS+="static_node=uinput"` (+ optional strip of `ID_INPUT_JOYSTICK` on the physical node with an explicit `uaccess`; this hides it from host SDL but not from containerised SDL) | uinput rule (keys) + hidraw `uaccess` rule + hwdb fuzz override |
| postinst | `udevadm control --reload && udevadm trigger -s misc -s input` | + `systemd-hwdb update`, install `.bpf.o` for udev-hid-bpf (or our loader) |
| depends | webkit2gtk-4.1, libayatana-appindicator3-1 | + udev-hid-bpf (Fedora packages it; availability elsewhere unverified) |
| AppImage | Cannot install udev rules, so it is degraded to phase-0 read-only until the user installs the rule; show instructions | Same |
| systemd unit | none (B1); `--user` unit for B2 | none (udev-triggered loader) |

### 4.5 CI lanes

- `rust-linux`: `apt-get install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev`, since `cargo clippy --workspace`
  now compiles the shell. Adds a few minutes cold, and keeps CONTRACT §8's promise.
- `linux-e2e` (B): uinput end-to-end (§6.5).
- `driver-host-test`: unchanged, plus the integer variant (A).
- `bpf-build` (A): clang-18 (Ubuntu 24.04 default) `-target bpf -O2 -g` against vendored udev-hid-bpf headers. Optional VM job (§6.5).
- `linux-bundle` (nightly/release): `npm run tauri -- build --bundles deb,rpm,appimage`. AppImage built on 24.04 needs glibc ≥ 2.39
  (Debian 13 has 2.41). Unsigned, with SHA256SUMS. The release `check` gate needs splitting so missing SignPath secrets do not block Linux.

## 5. Effort, phases, coexistence

### 5.1 Effort (person-days, optimistic / likely)

| # | Component | A | B |
|---|---|---|---|
| P0.1 | Cargo/cfg widening, build.rs/main.rs/lib.rs, `tauri.linux.conf.json` | 0.5 / 1 | same (shared) |
| P0.2 | evdev read-only input → `InputFrame` (enumerate, reconstruct, reconnect) | 1 / 1.5 | shared |
| P0.3 | watcher stub (no switching; `/proc` `list_windows`) | 0.25 / 0.5 | shared |
| P0.4 | CI WebKitGTK deps + Linux `tauri build --no-bundle` | 0.5 / 1 | shared |
| P0.5 | frontend platform copy | 0.25 / 0.5 | shared |
| | **Phase 0 subtotal** | **2.5 / 4.5** | (shared) |
| S1 | watcher Linux (X11 foreground, `/proc`, Wine cmdline, shared workers) | 2 / 3.5 | shared |
| S2 | keys Linux column + uinput keyboard | 0.75 / 1.5 | shared |
| S3 | tray quirks / GNOME fallback | 0.25 / 0.75 | shared |
| S4 | packaging: deb/rpm/AppImage, udev, postinst | 1 / 2 | shared |
| S5 | CI Linux bundle lane, release gate split | 0.75 / 1.5 | shared |
| S6 | CONTRACT/docs | 0.5 / 1 | shared |
| | **Shared subtotal** | **5.25 / 10.25** | (shared) |
| B1 | uinput joystick out, EVIOCGRAB, EVIOCSABS fuzz 0, `driverInstalled` | — | 1.5 / 2.5 |
| B2 | uinput rule + physical-node hide rule | — | 0.25 / 0.75 |
| B3 | uinput e2e CI test (§6.5) | — | 1 / 2 |
| B4 | hardware validation (evtest, SDL, Proton, Steam) | — | 1 / 2 |
| A1 | `pipeline_math.h` integer core + Makefile variant (prototype exists) | 0.5 / 1 | — |
| A2 | BPF program: descriptor, probe, rdesc_fixup, device_event, hw_request SET/GET 3/4, CRC/validate, double buffer | 3 / 5 | — |
| A3 | BPF build: clang, vendored headers, Makefile/CI | 1 / 2 | — |
| A4 | loader/install: udev-hid-bpf dependency **or** own libbpf-rs loader; kernel/`CONFIG_HID_BPF` detection + fallback | 2 / 4 | — |
| A5 | `core::hid` BPF descriptor + parser + tests, descriptor sync check | 0.5 / 1 | — |
| A6 | app hidraw backend (hidapi `linux-native-basic-udev`, detect by descriptor, combined report, push/info) | 1 / 2 | — |
| A7 | uhid-in-VM integration test (virtme-ng, kernel with `CONFIG_HID_BPF`) | 2 / 4 | — |
| A8 | hidraw rule + hwdb fuzz override | 0.25 / 0.5 | — |
| A9 | hardware validation, both bcdDevice revisions, descriptor dump, kernel matrix | 1.5 / 3 | — |
| | **Backend-specific subtotal** | **11.75 / 22.5** | **3.75 / 7.25** |
| | **Total path (P0 + shared + backend)** | **19.5 / 37.25** | **11.5 / 22** |
| | A *with* B as the required fallback for kernels without HID-BPF (Debian 13 stock) | **23.25 / 44.5** | — |
| | Optional B1 → B2 daemon split | — | +3 / +5 |

### 5.2 Phases

- **Phase 0 (2.5/4.5 d): Linux build, read-only.**
  - The Tauri app runs on Linux; the Test, Axes and Profiles pages work against evdev raw input with in-app processing.
  - `driverInstalled=false`; no effect on games.
  - Needs no udev rule (joystick uaccess is default), so `npm run tauri dev` works on the dev box.
  - Minimum set: P0.1–P0.5. Everything else (`profiles`, `settings`, `switcher`, `calibration`, `state`, `commands`, the frontend) compiles as-is.
  - Fuzz=3 smoothing of 1–2 LSB is accepted here. Do not change device settings from a viewer.
- **Phase 1 (9/17.5 d): B1 plus the shared parts.** Ships Linux.
- **Phase 2 (11.75/22.5 d): A as an optional preferred backend**, only if the technical evaluation shows that B's silent-twin
  device or winebus-hidraw bypass hurts in practice. Adopt §2.7 at the same time.

### 5.3 One seam or one backend?

- **Ship B.** Phase 0 is a strict subset of B (same evdev reader, minus grab and uinput), so B and the fallback share code.
- **If A is built, coexist, but not behind a trait.** Use one detection point in `device.rs` `session()`, shaped like today's
  `Conn { driver }`: `enum Backend { Bpf(HidDevice), Uinput { src, out }, ReadOnly(src) }`, chosen at connect time in that order.
- **Costs of coexistence:** two test matrices, two packaging paths, and a **hidraw-permission policy conflict**. A needs the user to
  read hidraw; B needs physical hidraw root-only, or Wine's hidraw path bypasses the grab. udev cannot reliably tell whether the BPF
  program is attached, so one default has to win.

## 6. Code risks

1. **hidapi Linux backend (A only).** `linux-static-hidraw` (default) builds C hidapi and links libudev (`libudev-dev` at build).
   `linux-native` uses the `udev` crate (libudev). **`linux-native-basic-udev`** is pure Rust and mirrors the Windows "no C" choice.
   Its risk is that it is younger. Open questions: does it enumerate per-TLC usage pages, and does it support `get_report_descriptor`?
   Declare it in its own `[target.'cfg(target_os="linux")'.dependencies]` table so feature unification with `windows-native` never happens.
2. **`evdev` crate maturity.** 0.13.2 is widely used. Confirm in the spike: SYN_DROPPED resync semantics, whether an `EVIOCSABS`
   wrapper exists (else raw ioctl), and that `emit()` appends SYN.
3. **BPF toolchain.**
   - **aya 0.14 has no struct_ops support** (docs.rs program list), and HID-BPF ≥ 6.11 is struct_ops-only. Loading from Rust means
     libbpf-rs, which is what udev-hid-bpf itself uses, or just shipping a `.bpf.o` for udev-hid-bpf.
   - Clang 18 on 24.04 is fine; pin it.
   - Kernel ≥ 6.11 for struct_ops; the `hid_hw_request` CFI fix version is to be confirmed.
   - `CONFIG_HID_BPF` is absent on Debian 13 stock (measured).
4. **Kernel header pinning.** Compile against vendored udev-hid-bpf `vmlinux.h`/`hid_bpf_helpers.h` (CO-RE) rather than build-host
   headers. Support only the struct_ops API, not the 6.3–6.10 tracing API (it churned).
5. **Missing fixture.** The stock 122-byte descriptor "is published nowhere" (research.md). A's probe check and uhid fixture need it.
   Dump `/sys/bus/hid/devices/*046D:C215*/report_descriptor` on first Linux contact. B needs nothing.
6. **Silent twin in B.** The grabbed physical evdev is still listed, and games may bind to it. The udev `ID_INPUT_JOYSTICK` strip
   helps host SDL only. A has no twin.
7. **Testing without hardware.**
   - **B end-to-end in CI** (`ubuntu-24.04`): `sudo modprobe uinput`, then a CI-only udev rule (`KERNEL=="event*", MODE="0666"`;
     `/dev/uinput` 0666) and `udevadm control --reload`.
   - The Rust integration test creates a **fake source** via uinput that clones the physical device: 046d:c215, its name,
     `ABS_X/Y` 0..1023 fuzz 3 flat 63, `ABS_RZ/THROTTLE` 0..255, hat −1..1, `BTN_TRIGGER..BTN_BASE6`.
   - It then starts the B backend (it matches by VID/PID like the real stick), replays every `vectors.json` raw case as evdev events,
     reads the **virtual output** device, and asserts ±1 on axes and exact hat/buttons.
   - Further assertions: a second `EVIOCGRAB` on the source fails with `EBUSY` (grab works); fuzz is reset (send ±1 LSB steps and
     expect exact output); unplug (destroy the source) → reconnect.
   - uinput sources pass through the same input-core defuzz as hid-input devices, so the fuzz path is tested realistically. The test is
     `#[ignore]` unless `/dev/uinput` is writable, so the dev-box `cargo test` stays green.
   - Confirm uinput availability on hosted runners in a 30-minute spike.
   - **A:** compile-only plus the host vector test on stock runners. A real verifier and behaviour test needs a kernel with
     `CONFIG_HID_BPF` (virtme-ng VM job), using `/dev/uhid` to impersonate 046d:c215 (bus USB, so `hid-lg` binds) — the approach of
     the kernel's own HID-BPF selftests.
8. **Release plumbing.** The SignPath-only `check` job blocks Linux releases until it is split.

Sources: [hid-input.c](https://raw.githubusercontent.com/torvalds/linux/master/drivers/hid/hid-input.c) ·
[hid-lg.c](https://raw.githubusercontent.com/torvalds/linux/master/drivers/hid/hid-lg.c) ·
[hid.h](https://raw.githubusercontent.com/torvalds/linux/master/include/linux/hid.h) ·
[HID-BPF docs](https://docs.kernel.org/hid/hid-bpf.html) ·
[unpriv checks moved to map_create/prog_load](https://kernsec.org/pipermail/linux-security-module-archive/2023-April/037156.html) ·
[udev-hid-bpf udev properties](https://libevdev.pages.freedesktop.org/udev-hid-bpf/udev-properties.html) ·
[aya programs (0.14)](https://docs.rs/aya/latest/aya/programs/index.html) ·
[evdev VirtualDeviceBuilder (0.13.2)](https://docs.rs/evdev/latest/evdev/uinput/struct.VirtualDeviceBuilder.html) ·
[hidapi features (2.6.7)](https://docs.rs/crate/hidapi/latest/features) ·
[Tauri TrayIconBuilder](https://docs.rs/tauri/latest/tauri/tray/struct.TrayIconBuilder.html) ·
[HID-BPF struct_ops conversion series](https://lkml.iu.edu/hypermail/linux/kernel/2406.1/00069.html)
