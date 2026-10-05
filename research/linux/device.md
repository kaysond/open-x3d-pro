# Linux: how the Extreme 3D Pro (046d:c215) behaves today, and what the kernel gives for free

Status: desk research 2026-10-04, no stick attached to a Linux box yet. Everything below comes from source, not from a capture. §9 lists what to capture.

Pinned sources (fetched 2026-10-04):
- Kernel: torvalds/linux master `a90ee4305c4a5df72c11b31dacfdc76e00fcf78a`. `K:` = `https://github.com/torvalds/linux/blob/a90ee4305c4a5df72c11b31dacfdc76e00fcf78a/`
- SDL: libsdl-org/SDL `main`. systemd: `main`. Wine: gitlab.winehq.org `master`. Proton: ValveSoftware/wine `proton_10.0`.

## TL;DR

- The stick binds to **`hid-logitech`** (`hid-lg.c`), not `hid-generic`. For c215 that driver does exactly one thing: it sets `HID_QUIRK_NOGET`. It has no descriptor fixup, no event hook and no custom mapping. Since v4.12 the kernel no longer reads reports at probe, so NOGET does almost nothing today.
- Expected evdev device: `ABS_X`/`ABS_Y` 0..1023, `ABS_RZ` 0..255, **`ABS_THROTTLE`** 0..255 (from Slider), `ABS_HAT0X`/`ABS_HAT0Y` −1..1, and 12 buttons `BTN_TRIGGER`(0x120)..`BTN_BASE6`(0x12b).
- **The kernel ships a built-in filter and deadzone metadata.** hid-input sets `fuzz=(max-min)>>8` and `flat=(max-min)>>4` on joystick axes, giving X/Y fuzz 3 / flat 63 and Rz/throttle fuzz 0 / flat 15.
  - The input core applies **fuzz** as a hysteresis/averaging filter for every evdev and joydev reader. hidraw is not filtered.
  - **flat** is only metadata for evdev. **joydev applies it**: a ±63-count centre deadband plus 63 counts of end saturation on X/Y, and the same 12 % centre deadband on the *throttle*.
  - SDL and Wine ignore flat by default.
- Free calibration knobs, no driver needed:
  - `EVIOCSABS`: any process holding the fd can call it. It changes the device for all readers and is lost on replug.
  - Made persistent by systemd's **`60-evdev.hwdb` `EVDEV_ABS_xx=min:max:res:fuzz:flat`**.
  - `jscal`/`JSIOCSCORR` for `/dev/input/js*`, restored on plug by Debian's `60-joystick.rules`.
  - `EVIOCGRAB` gives exclusive evdev access (it also silences joydev, but not hidraw).
- **`usbhid.jspoll` is a no-op on xHCI** (every modern PC). xHCI uses the endpoint descriptor and overwrites the URB interval. On xHCI the low-speed rev is polled every **8 ms (125 Hz)**, not 10 ms, because the interval rounds down to a power of two. The full-speed rev is polled every 1 ms.
- hidraw:
  - Nodes are `root:root 0600`. No systemd or Steam rule grants access for c215, so we must ship a udev rule.
  - hidraw reads run concurrently with evdev.
  - `HIDIOCGFEATURE`/`HIDIOCGINPUT` go straight to the wire, NOGET or not. Expect a ~5 s block then `ETIMEDOUT` (the 2008 quirk commit says the stick "times out"), or an immediate `EPIPE` if it STALLs.
- HID-BPF: whether hid-logitech is bound makes no difference. The descriptor fixup runs before driver matching, and event and `hid_hw_request` hooks run before driver and hidraw.
  - It is the in-kernel Linux analogue of "option B": rewrite the descriptor, process events, and answer feature reports ourselves.
  - It needs `CONFIG_HID_BPF=y` and kernel ≥ 6.11. Arch, Fedora and Debian sid have it. **Debian 13 trixie's stock 6.12 kernel does not** (checked on this box).
- Known user-facing breakage:
  - Old bundled SDL2 (< 2.0.14) treats the stick as a gamepad. SDL fixed this in 2020.
  - Wine ≥ 7 maps Rz→Z and throttle→Rz in DirectInput (Wine bug 55653, open).
  - The throttle is "inverted" (0 = forward) by hardware.

## 1. Driver binding

| Fact | Source |
|---|---|
| `#define USB_DEVICE_ID_LOGITECH_EXTREME_3D 0xc215` | `K:drivers/hid/hid-ids.h#L937` |
| `lg_devices[]`: `{ HID_USB_DEVICE(LOGITECH, EXTREME_3D), .driver_data = LG_NOGET }` | `K:drivers/hid/hid-lg.c#L874-L875` |
| `lg_probe`: `if (quirks & LG_NOGET) hdev->quirks \|= HID_QUIRK_NOGET;`, then a plain `hid_parse` + `hid_hw_start(HID_CONNECT_DEFAULT)` | `hid-lg.c#L750-L792` |
| `lg_report_fixup`: only `LG_RDESC`/`LG_RDESC_REL_ABS` checks plus a `switch(product)` over WingMan FG/FFG, DF, MOMO, MOMO2, FV, DFP, Wii wheels. **No c215 case** | `hid-lg.c#L430-L533` |
| `lg_input_mapping`: only acts for Ultra-X remote, `LG_WIRELESS`, `LG_EXPANDED_KEYMAP`. c215 falls through to the default mapping | `hid-lg.c#L632-L679` |
| `lg_input_mapped`: "Ensure that Logitech wheels are not given a default fuzz/flat value" (it rewrites `field->application` to MULTIAXIS for wheels only). **c215 is not in the list, so it keeps joystick fuzz/flat** | `hid-lg.c#L695-L716` |
| `lg_event`/`lg_raw_event`: only `LG_INVERT_HWHEEL`/`LG_FF4`. No-ops for c215 | `hid-lg.c#L722-L748` |

**NOGET semantics.**
- `__usbhid_submit_report()` drops IN-direction requests on the *queued* path (`hid_hw_request(...GET_REPORT)`) when NOGET is set (`K:drivers/hid/usbhid/hid-core.c#L531`).
- It does **not** touch `hid_hw_raw_request()`. That path is `usbhid_get_raw_report()` → `usb_control_msg(GET_REPORT)` with a 5000 ms timeout (`hid-core.c#L873-L905`, `USB_CTRL_SET_TIMEOUT` in `include/linux/usb.h`), and it is what hidraw ioctls use.
- History: commit `7cea465f9bf3` (Jiri Kosina, 2008-01-17) "Logitech Extreme 3D needs NOGET quirk, otherwise it times out at the time of connect". At that time usbhid read every input and feature report at probe.
- Commit `9143059fafd4` (Benjamin Tissoires, 2017-03-08, v4.12) "HID: remove initial reading of reports at connect" moved that read into hiddev only.
- hiddev is never created for this stick. `hiddev_connect()` bails when every application collection is an input application (`IS_INPUT_APPLICATION` covers GD 0x01..0x08), `K:drivers/hid/usbhid/hiddev.c#L875-L890`.
- **Net effect on ≥ 4.12: NOGET is vestigial for c215.** hid-generic would work identically.

**Special-driver deferral.**
- c215 is listed in `hid_have_special_driver[]` under `#if IS_ENABLED(CONFIG_HID_LOGITECH)` (`K:drivers/hid/hid-quirks.c#L512-L519`). That list sets `HID_QUIRK_HAVE_SPECIAL_DRIVER` (`#L1359`).
- `hid_generic_match()` then returns false (`K:drivers/hid/hid-generic.c#L46`). The device stays unbound until `hid_logitech.ko` loads. udev loads it through the modalias, `alias hid:b0003g*v0000046Dp0000C215 hid_logitech` (from this box's `modules.alias`).
- `CONFIG_HID_LOGITECH`: `=m` on Arch (`config.x86_64`), Fedora (kernel-ark `common/generic`) and Debian 13 (`/boot/config-6.12.*+deb13`). If it is built `=n` the list entry compiles out and hid-generic binds, which is harmless.
- Forcing hid-generic: `hid.ignore_special_drivers=1`. Only cosmetic difference: hid-generic sets `HID_QUIRK_INPUT_PER_APP` (`hid-generic.c#L64`). With one application collection that still yields a single input node, and Joystick gets no name suffix (`K:drivers/hid/hid-input.c#L2068-L2130`).

**HID-BPF.**
- The BPF `hid_rdesc_fixup` result is computed in `__hid_device_probe()` before `hid_check_device_match()` (`K:drivers/hid/hid-core.c#L2799-L2820`). The driver's own `report_fixup` then runs on the BPF output (`#L1359-L1376`), and that is a no-op for c215.
- `dispatch_hid_bpf_device_event()` runs before the driver's `raw_event` and before `hid_report_raw_event()` → `hidraw_report_event()` (`#L2150`, `#L2103`). **hidraw readers see BPF-modified reports.**
- `__hid_hw_raw_request()` calls `dispatch_hid_bpf_raw_requests()` first (`#L2536-L2557`). A `hid_hw_request` struct_op that returns >0 answers `HIDIOCGFEATURE` itself, so the device is never asked (`K:include/linux/hid_bpf.h#L135-L161`). That gives us a Linux config channel without a GET_REPORT stall.
- In-tree precedents under `drivers/hid/bpf/progs/`:
  - `Logitech__SpaceNavigator.bpf.c`: a device that *also* has a hid-lg `LG_RDESC_REL_ABS` entry, so BPF and hid-lg coexist.
  - `FR-TEC__Raptor-Mach-2.bpf.c`: a flight stick, with rdesc fixup and event fixup.
  - `Thrustmaster__TCA-Yoke-Boeing.bpf.c`.
- Loading: root through `udev-hid-bpf` (progs `README`).
- API: struct_ops since v6.11 (`ebc0d8093e8c`, 2024-06-14). A recent fix, `2b658c1c442e` (2026-03-16) "prevent buffer overflow in hid_hw_request", argues for current kernels.
- `CONFIG_HID_BPF=y`: Arch yes, Fedora yes (`common/generic/CONFIG_HID_BPF`), Debian `debian/latest` (sid/forky) yes. **Debian 13 trixie stock: `# CONFIG_HID_BPF is not set`** (this box, 6.12.107/6.12.111).

## 2. Expected evdev device

Name: usbhid builds it from the strings (`K:drivers/hid/usbhid/hid-core.c#L1410-L1416`), giving **`Logitech Logitech Extreme 3D`**.

Persistent links (systemd `60-persistent-input.rules`):
- `/dev/input/by-id/usb-Logitech_Logitech_Extreme_3D-event-joystick`
- `/dev/input/by-id/usb-Logitech_Logitech_Extreme_3D-joystick`
- On rev 57.11 the serial is appended after `_3D`.

**`id.version` is bcdHID (0x0110), not bcdDevice.** `usbhid_probe` sets `hid->version = bcdDevice` (`#L1402`), but `hid_add_device()` then calls `->parse` (`hid-core.c#L3046`), and `usbhid_parse` overwrites it with `bcdHID` (`usbhid/hid-core.c#L1037`). hid-input copies that value into `input_dev->id.version` (`hid-input.c#L2147`).
- So all three revisions present the same evdev ID `0003:046d:c215:0110`, and the SDL GUID is `030000006d04000015c2000010010000`.
- Revisions are distinguishable only via the USB sysfs attributes (`bcdDevice`, `speed`, `serial`).

| HID item (from WebHID parse) | Mapping rule (`hid-input.c`) | evdev code | min | max | fuzz | flat | res |
|---|---|---|---|---|---|---|---|
| GD X, 10 bit, 0..1023 | `map_abs_clear(usage & 0xf)` `#L920-L926` | `ABS_X` 0x00 | 0 | 1023 | 3 | 63 | 0¹ |
| GD Y, 10 bit | same | `ABS_Y` 0x01 | 0 | 1023 | 3 | 63 | 0¹ |
| GD Rz, 8 bit | same | `ABS_RZ` 0x05 | 0 | 255 | 0 | 15 | 0² |
| GD Slider (0x36), 8 bit | `map_abs(usage & 0xf)` `#L952-L956`: 0x36 & 0xf = 6 | **`ABS_THROTTLE` 0x06** (not `ABS_MISC`; there is no `ABS_SLIDER`) | 0 | 255 | 0 | 15 | 0 |
| GD Hat, 4 bit, 0..7, null | `hat_min/max` + `map_abs(ABS_HAT0X)` `#L959-L963`; then both axes are forced to `-1,1,0,0` `#L1492-L1500` | `ABS_HAT0X` 0x10, `ABS_HAT0Y` 0x11 | −1 | 1 | 0 | 0 | 0 |
| Button 1..12 (app = Joystick) | `code = usage-1; if code<=0xf: += BTN_JOYSTICK` `#L801-L806` | 0x120..0x12b | | | | | |
| Vendor feature FF00:0001, 4-bit pad | ignored by hid-input | none | | | | | |

¹ `hidinput_calc_abs_res()` returns a resolution for X/Y only when the unit is cm or inch (`#L246-L286`).
² For Rz, only if the Unit global is degrees or radians. Win11 HID caps show degrees on the hat. Whether that Unit global leaks onto the following Rz field depends on the raw descriptor bytes, so capture it.

The fuzz/flat formula, quoted: `if (application == HID_GD_GAMEPAD || application == HID_GD_JOYSTICK) input_set_abs_params(input, code, a, b, (b - a) >> 8, (b - a) >> 4);` (`hid-input.c#L1480-L1482`).

Buttons, in index order: `BTN_TRIGGER` 0x120, `BTN_THUMB`, `BTN_THUMB2`, `BTN_TOP`, `BTN_TOP2`, `BTN_PINKIE`, `BTN_BASE`, `BTN_BASE2`, `BTN_BASE3`, `BTN_BASE4`, `BTN_BASE5`, `BTN_BASE6` 0x12b (`include/uapi/linux/input-event-codes.h#L366-L378`).
- If we ever republish more buttons: 13..16 → 0x12c..0x12f (0x12c-0x12e are unnamed, 0x12f is `BTN_DEAD`).
- 17+ → `BTN_TRIGGER_HAPPY1` 0x2c0+. The `code<=0xf` guard means button 17 never lands on `BTN_GAMEPAD` 0x130.

Hat decode (`hid-input.c#L1604-L1611`, table `#L49`):
- `dir = (v - 0)*8/8 + 1`. Values 0..7 map to N, NE, E, SE, S, SW, W, NW, i.e. (0,−1), (1,−1), (1,0), (1,1), (0,1), (−1,1), (−1,0), (−1,−1).
- **The null value 8 gives dir 9, which is >8, so it becomes 0, i.e. (0,0).**
- The hat path returns before the null-state range check, so 8 is never dropped.

Slider orientation is raw: `ABS_THROTTLE` = 0 when the lever is fully forward (research.md). The kernel never inverts anything.

Report pacing: usbhid sends `SET_IDLE(0)` unconditionally (`usbhid/hid-core.c#L1053`), so the stick only reports on change. A motionless stick produces no evdev events.

## 3. Built-in filtering: fuzz and flat

**fuzz (applied in-kernel, to evdev and joydev, not hidraw).**
- `input_handle_abs_event()` → `input_defuzz_abs_event(value, old, fuzz)` (`K:drivers/input/input.c#L71-L85`, `#L191`). If the result equals `old`, the event is dropped.
- With X/Y fuzz = 3 and integer math (3/2 = 1):
  - |Δ| ∈ {1, 2} → `(3·old+new)/4`, which truncates. +1 or +2 is **dropped**; −1 or −2 moves by −1. **The behaviour is asymmetric.**
  - |Δ| ∈ {3, 4, 5} → `(old+new)/2`.
  - |Δ| ≥ 6 → passes through.
- Result: slow movements lag and come out quantised. A calibration UI must read hidraw, or first set fuzz to 0 via `EVIOCSABS`.
- Rz and throttle have fuzz 0, so they are unfiltered.

**flat (metadata, honoured only by joydev).** Evdev delivers raw values and is not clamped to min/max. Who uses flat:

| Consumer | Uses `flat`? | Scaling |
|---|---|---|
| evdev raw readers | no (they can read it with `EVIOCGABS`) | none |
| joydev `/dev/input/js*` | **yes**, at connect time | `JS_CORR_BROKEN`, see below |
| SDL3 Linux backend | **no** by default. Only with `SDL_JOYSTICK_LINUX_DEADZONES=1` (`SDL_sysjoystick.c` `ConfigJoystick`) | linear from `absinfo.minimum..maximum` to −32768..32767, read once at open |
| Wine winebus evdev (`lnxev`) | no | caches `abs.minimum/maximum` at device creation (`bus_udev.c#L1256-L1262`) |
| Wine winebus SDL backend (default) | inherits SDL's default (no) | SDL |

**joydev default correction** (`K:drivers/input/joydev.c#L966-L986`, applied in `joydev_correct` `#L68-L85`):
- `coef0/1 = mid ∓ flat`, `coef2/3 = 2^29 / ((max-min)/2 − 2·flat)`, output clamped to ±32767.
- Computed for this stick:

| js axis | evdev | centre deadband (→ 0) | saturates at |
|---|---|---|---|
| 0 | ABS_X | raw 448..574 (127 counts ≈ 12 %) | ≤ 63 and ≥ 959 (≈ 6 % per end) |
| 1 | ABS_Y | same | same |
| 2 | ABS_RZ | 112..142 (31 ≈ 12 %) | ≤ 15, ≥ 239 |
| 3 | ABS_THROTTLE | **112..142 → 0. A deadband in the middle of the throttle** | ≤ 15, ≥ 239 |
| 4, 5 | HAT0X/Y | none (flat 0) | ±1 → ±32767 |

js button order is 0..11 = `BTN_TRIGGER`..`BTN_BASE6` (keybit order starting at `BTN_JOYSTICK`, `#L953-L965`).

joydev computes `corr` once, inside `input_register_device()`, before udev runs. **Later `EVIOCSABS` or hwdb changes do not reach js.** Only `JSIOCSCORR` (`jscal`) changes it.

## 4. Free runtime knobs

- **`EVIOCSABS(axis)`** (`K:drivers/input/evdev.c#L1225-L1251`):
  - No capability check. Any fd on the event node works, and udev `uaccess` gives the seat user one.
  - It replaces the whole `input_absinfo` for every reader.
  - It is volatile: a new `input_dev` on replug or rebind resets it.
  - SDL and Wine only read it when they open the device, so set it before the game starts.
- **Persistence without code:** systemd `60-evdev.rules` → builtin `keyboard` → `override_abs()` issues `EVIOCSABS` on every add (`systemd: src/udev/udev-builtin-keyboard.c#L112-L135`). Format: `EVDEV_ABS_<hex>=min:max:res:fuzz:flat`, and an empty field keeps the current value (`hwdb.d/60-evdev.hwdb` header). Example (untested) that kills the filter and the deadzone metadata and sets calibrated X/Y ends:

  ```
  # /etc/udev/hwdb.d/61-x3dpro.hwdb ; then: systemd-hwdb update && udevadm trigger -s input
  evdev:input:b0003v046DpC215*
   EVDEV_ABS_00=12:1010::0:0
   EVDEV_ABS_01=8:1015::0:0
   EVDEV_ABS_05=:::0:0
   EVDEV_ABS_06=:::0:0
  ```
  This is the cheapest "calibration that SDL/Wine honour" on Linux: SDL rescales linearly from min/max and clamps. It cannot fix centre offset or curves; that needs the BPF, uinput or hidraw tiers.
- **`evdev-joystick`** (Debian/Arch `joystick`/`linuxconsole`): `--showcal`, `--evdev DEV [--axis N] --minimum/--maximum/--deadzone(=flat)/--fuzz`. It is a CLI around `EVIOCSABS` (`utils/evdev-joystick.c`, man page `evdev-joystick(1)`). Arch wiki *Gamepad* §"evdev API deadzones" recommends a udev rule to re-apply it.
- **`jscal`** (`JSIOCSCORR`, `-c` interactive, `-s` set, `-p` print), `jscal-store`, and `jscal-restore`. Debian ships `/usr/lib/udev/rules.d/60-joystick.rules`, which runs `jscal-restore %E{DEVNAME}` on every `js*` add (`debian/joystick.udev`). joydev also offers remapping via `JSIOCSAXMAP`/`JSIOCSBTNMAP` (`joydev.c#L444-L507`).
- **`EVIOCGRAB`** (`evdev.c#L1089`, `input.c#L520-L533`):
  - It sets `dev->grab`, so only the grabbing client gets events. Other evdev clients **and joydev** go silent. hidraw is unaffected.
  - This is the standard Linux remapper pattern (grab + `uinput` virtual stick). But the grabbed original still *enumerates* in SDL as a dead joystick, so hide it with permissions or `SDL_JOYSTICK_BLACKLIST_DEVICES`.
- **`usbhid.jspoll=N`** (`usbhid/hid-core.c#L55-L57`, applied at probe `#L1132-L1143` when `hid->collection->usage == HID_GD_JOYSTICK`, which is true for c215):
  - It sets `urb->interval` only.
  - On xHCI, `check_interval()` overwrites `urb->interval` with the endpoint-context value derived from the **descriptor** `bInterval` (`K:drivers/usb/host/xhci-ring.c#L3453-L3480`). **No effect on xHCI.**
  - It works only on EHCI/OHCI/UHCI.
  - The Arch wiki *Mouse polling rate* says the same (kernel bugzilla 82571) and points to `usb_oc-dkms`, which patches `bInterval`, as the xHCI workaround.
- **xHCI rounding** (`K:drivers/usb/host/xhci-mem.c#L1271-L1276`, `#L1286-L1330`):
  - LS/FS interrupt endpoint: `exp = clamp(fls(bInterval*8)-1, 3, 10)`.
  - Rev 02.04/35.00, `bInterval 10`: 80 µframes → exp 6 → **64 µframes = 8 ms (125 Hz)**.
  - Rev 57.11, `bInterval 1`: 8 µframes → 1 ms.
  - usbcore accepts LS `bInterval` 1..255 ("overclocked devices", `K:drivers/usb/core/config.c#L397-L403`).
  - research.md's "100 Hz" therefore needs a measurement (§9). Windows' xHCI stack probably rounds too (unverified).

## 5. Consumers on Linux

- **udev**:
  - `input_id` builtin: abs axes + `BTN_JOYSTICK`-range buttons → `ID_INPUT_JOYSTICK=1` (`systemd: src/udev/udev-builtin-input_id.c#L211-L248`).
  - `70-uaccess.rules`: `SUBSYSTEM=="input", ENV{ID_INPUT_JOYSTICK}=="?*", TAG+="uaccess"`.
  - `50-udev-default.rules`: input nodes `GROUP="input"`, `js*` `MODE="0664"`.
  - `70-joystick.rules` + `70-joystick.hwdb` only set `ID_INTEGRATION` (external for USB on an external port). Nothing c215-specific.
- **libinput**: `evdev.c` "device is a joystick or a gamepad, ignoring". `libinput record` is useless here.
- **SDL3** (`src/joystick/linux/SDL_sysjoystick.c`, `SDL_joystick.c`):
  - It opens `/dev/input/event*`; `SDL_JOYSTICK_LINUX_CLASSIC=1` switches to js.
  - c215 is in `initial_flightstick_devices[]` (`SDL_joystick.c#L467`), so `SDL_JOYSTICK_TYPE_FLIGHT_STICK`.
  - No automatic gamepad mapping: `LINUX_JoystickGetGamepadMapping` requires `BTN_GAMEPAD`.
  - The hat is treated as digital because its range is −1..1.
  - Axes: linear scaling, flat ignored, kernel fuzz already applied.
  - History: SDL bug 5326. Commit `76bd6cd2d9eb` (2020-11-11, SDL ≥ 2.0.14) "Logitech Extreme 3D joystick is listed as gamepad in linux section of SDL_gamecontrollerdb.h" removed the entry. `f398d8a42422` (2022-09-07) "Note that the Logitech Extreme 3D is a flight stick".
  - Community `gamecontrollerdb.txt`: entries were added in 2017 (`6912486de27f`) and 2018 (`1a6de3a9a8c8`), then removed by `019753034e15` (2020-12-13, "Removes Logitech joystick"). **The current file has no `6d04000015c2` line** (checked 2026-10-04). `SDL_gamepad_db.h` has none either.
  - Games that bundle an older SDL2 still show it as a gamepad (Steam X4 thread, 2021). Workaround to try: `SDL_GAMECONTROLLER_IGNORE_DEVICES=0x046d/0xc215` (the hint exists in current SDL; check the bundled version).
- **Wine** (`dlls/winebus.sys/main.c`):
  - Default is the SDL backend (`Enable SDL`=1). A successful SDL init sets `disable_input` (`main.c#L1277`), so the evdev path is skipped.
  - hidraw is opt-in for c215: it is not in the `is_hidraw_enabled()` VID/PID list (`#L476-L573`).
  - Enable it with registry `HKLM\System\CurrentControlSet\Services\winebus` `EnableHidraw` multi-string `046D:C215`, or `...\winebus\Devices\046D\C215` `Hidraw`=1.
  - Proton `proton_10.0` adds env `PROTON_ENABLE_HIDRAW=0x046D/0xC215` (or `1` for all) and `PROTON_DISABLE_HIDRAW` (`main.c#L556-L584`).
  - hidraw mode also needs a hidraw permission rule (§6).
  - **Wine bug 55653** (2023-09, UNCONFIRMED, last touched 2026-08): "Logitech Extreme 3d Joystick has axes mapped incorrectly". Wine ≥ 7.1 shows evdev Rz as DirectInput Z and the throttle as Rz; Wine 6.23 was fine. A commenter traces it to slot-number == function-number assumptions for axes [0,1,5,6,16,17]. Workarounds in the thread: evsieve remap, joymap.
- **Steam Input**: no primary source found that it grabs c215 by default. User reports describe manual "add controller" mapping in Big Picture, which turns it into a gamepad config. Treat it as unverified.

## 6. hidraw

- **Node permissions:** `hidraw_class` has no `devnode` callback, so devtmpfs creates the node `root:root 0600` (`K:drivers/base/devtmpfs.c#L144`). systemd `70-uaccess.rules` has no hidraw joystick rule; its hidraw grants are only for `ID_SECURITY_TOKEN` and similar. Valve's `steam-devices/60-steam-input.rules` grants Valve/Sony/Nintendo etc. but **not 046d:c215**. We would need to ship something like:

  ```
  KERNEL=="hidraw*", SUBSYSTEM=="hidraw", ATTRS{idVendor}=="046d", ATTRS{idProduct}=="c215", TAG+="uaccess"
  ```
- **Concurrency:**
  - `hidraw_report_event()` copies each report to every open hidraw client (`K:drivers/hid/hidraw.c#L570-L590`).
  - It runs alongside hid-input in `hid_report_raw_event()`, so evdev, joydev and hidraw all get every report. `EVIOCGRAB` does not affect it.
  - There is no exclusive hidraw open.
  - Reads are 7 bytes with no report ID, and they are the unfiltered values.
- **Descriptor access:** both `HIDIOCGRDESC` and sysfs `report_descriptor` (0444) return `hdev->rdesc`, i.e. after BPF and driver fixups (`hid-core.c#L2266-L2282`, `hidraw.c#L403-L425`). For c215 with no BPF loaded, that equals the device bytes. **Dump it before loading any BPF.**
- **GET_REPORT via hidraw:**
  - Path: `HIDIOCGFEATURE(len)` → `hidraw_get_report()` → `__hid_hw_raw_request()` → `usbhid_get_raw_report()` → control GET_REPORT `wValue=0x0300|id`, 5000 ms timeout.
  - **NOGET is not consulted.**
  - Expected outcome: the 2008 commit points to NAKs, which give `ETIMEDOUT` (errno 110) after ~5 s. A protocol STALL would instead give an immediate `EPIPE` (errno 32).
  - `HIDIOCGINPUT` is the same story.
  - Measure once (§9). Never call it from a hot path.
  - `HIDIOCSFEATURE` on the vendor 4-byte feature report: unknown semantics, so do not send it blindly.

## 7. The hardware revisions on Linux

There is no revision-specific kernel code: `HID_USB_DEVICE` matches any bcdDevice, and the evdev `id.version` is identical (§2). Differences:

| | 02.04 (owner) / 35.00 | 57.11 |
|---|---|---|
| USB speed (`/sys/bus/usb/devices/X/speed`) | 1.5 (low) | 12 (full) |
| desc `bInterval` | 10 | 1 |
| effective xHCI poll | **8 ms** | 1 ms |
| `usbhid.jspoll` on xHCI | ignored | ignored |
| serial / by-id path | none: `usb-Logitech_Logitech_Extreme_3D-…` | serial appended |

Whether the 57.11 descriptor or behaviour differs (GET_REPORT, feature report) is unknown.

## 8. Known Linux quirks (summary)

| Symptom | Cause | Status / workaround |
|---|---|---|
| "Throttle inverted" | hardware: slider 0 = forward | invert in the app or game; the kernel passes it raw |
| Throttle has a dead spot mid-travel in js-API games | joydev applies flat 15 as a *centre* deadband on `ABS_THROTTLE` | `jscal -s` with no deadband for axis 3; or make the app use evdev |
| Tiny stick movements ignored / sticky | evdev fuzz 3 on X/Y | `EVDEV_ABS_00=:::0:` / `evdev-joystick --fuzz 0` |
| Detected as gamepad, half the axes missing | bundled SDL2 < 2.0.14 db entry | newer SDL; `SDL_GAMECONTROLLER_IGNORE_DEVICES` |
| Twist shows as Z, throttle as Rz (DirectInput under Wine/Proton) | Wine ≥ 7 winebus/dinput axis slotting, bug 55653 | open; evsieve remap; try `PROTON_ENABLE_HIDRAW` + udev hidraw rule (untested) |
| `usbhid.jspoll=1` does nothing | xHCI ignores urb interval | `usb_oc-dkms` (third party); a LS device may not deliver faster anyway |
| Device unbound on a custom kernel | `HAVE_SPECIAL_DRIVER` set, but `hid_logitech.ko` not installed | install the module or boot with `hid.ignore_special_drivers=1` |

## 9. Capture list (Linux box + stick, ideally all three revisions)

1. `lsusb -v -d 046d:c215` as root: bcdDevice, bInterval, speed, strings. For the full report descriptor dump, unbind first, or use `usbhid-dump -m 046d:c215`.
2. `for f in /sys/bus/hid/devices/0003:046D:C215.*; do readlink $f/driver; xxd $f/report_descriptor; done`. Expect driver `.../logitech`. **This gets the missing 122-byte hex.**
3. `sudo cat /sys/kernel/debug/hid/0003:046D:C215.*/rdesc` (parsed items + mapping), and `.../events` while moving each control (raw→usage→code).
4. `evtest /dev/input/by-id/usb-Logitech_Logitech_Extreme_3D-event-joystick`: the header gives the caps and absinfo table to diff against §2. Then 10 s of stirring the stick to get timestamps (inter-event Δ: expect 8 ms multiples on LS, 1 ms on FS).
5. `evdev-joystick --showcal <same>` and `cat /sys/class/input/event*/device/id/{vendor,product,version}`. Expect version `0110`.
6. `jstest --normal /dev/input/js0` (axis/button order; throttle deadband at mid-travel) and `jscal -p /dev/input/js0` (the default corr, compare with §3).
7. `udevadm info /dev/input/eventN`, `udevadm info /dev/input/js0`, `udevadm info /dev/hidrawN`: `ID_INPUT_JOYSTICK`, `ID_SERIAL`, `ID_INTEGRATION`, tags. Also `getfacl /dev/input/eventN /dev/hidrawN` and `ls -l /dev/hidraw*`.
8. Poll truth: `sudo modprobe usbmon` + Wireshark on `usbmonN`, or `cat /sys/kernel/debug/usb/usbmon/Nu`, to check whether IN tokens arrive every 8 ms and whether the stick NAKs between changes. Run once with `usbhid.jspoll=1` to confirm it is ignored on xHCI. Record the host controller (`lspci -k | grep -A2 USB`).
9. Feature/input GET_REPORT timing (hidraw, needs the udev rule or root), e.g. a Python `fcntl.ioctl(fd, (3<<30)|(5<<16)|(ord('H')<<8)|0x07, bytearray(5))` for `HIDIOCGFEATURE(5)`, and `0x0A` with 8 bytes for `HIDIOCGINPUT`. Record errno and elapsed time, and check that interrupt-IN reports keep flowing during the 5 s.
10. Initial state: `evtest` right after plugging, before touching anything. Does `absinfo.value` start at 0 until the first move? `SET_IDLE(0)` means the device only reports on change.
11. Kernel/config: `uname -r`, `grep -E 'HID_BPF|HID_LOGITECH|HIDRAW' /boot/config-$(uname -r)`, `cat /sys/module/usbhid/parameters/jspoll`.

## 10. Open questions

- Does Rz inherit the hat's degrees Unit, which would give a nonzero `ABS_RZ` resolution? (needs the raw descriptor)
- GET_REPORT on Linux: NAK timeout or STALL? Is it the same on 57.11?
- Does the LS stick's firmware produce new data every 8 ms poll, or only at an internal 10 ms tick, i.e. beat-pattern jitter?
- Does `PROTON_ENABLE_HIDRAW=0x046D/0xC215` fix the bug-55653 axis order (raw descriptor X, Y, Rz, Slider straight to DirectInput)?
- Which delivery tier do we pick for a Linux build: hwdb-only (min/max, fuzz/flat), HID-BPF (full pipeline in-kernel, root install, no trixie), or uinput + EVIOCGRAB (userspace daemon, duplicate device to hide)?

## Sources

- Kernel (pinned `a90ee4305c4a`): `drivers/hid/{hid-lg.c,hid-ids.h,hid-quirks.c,hid-generic.c,hid-input.c,hid-core.c,hidraw.c}`, `drivers/hid/usbhid/{hid-core.c,hiddev.c}`, `drivers/hid/bpf/{hid_bpf_dispatch.c,progs/}`, `include/linux/hid_bpf.h`, `include/uapi/linux/{input-event-codes.h,hidraw.h}`, `drivers/input/{input.c,evdev.c,joydev.c}`, `drivers/usb/host/{xhci-mem.c,xhci-ring.c}`, `drivers/usb/core/config.c`, `drivers/base/devtmpfs.c`, `Documentation/hid/hid-bpf.rst`, via `https://github.com/torvalds/linux/blob/a90ee4305c4a5df72c11b31dacfdc76e00fcf78a/<path>`
- Commits: https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/commit/?id=7cea465f9bf3ed84ed67337cd57fc97e25625771 · https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/commit/?id=9143059fafd4eebed2d43ffb5455178d4010e60a · https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/commit/?id=ebc0d8093e8c97de459615438edefad1a4ac352c
- SDL: https://github.com/libsdl-org/SDL/blob/main/src/joystick/linux/SDL_sysjoystick.c · https://github.com/libsdl-org/SDL/blob/main/src/joystick/SDL_joystick.c · https://github.com/libsdl-org/SDL/commit/76bd6cd2d9ebd0d139e04036978742c8fc02c0b6 · https://github.com/libsdl-org/SDL/commit/f398d8a42422c049d77c744658f1cd2bb011ed4a · https://bugzilla.libsdl.org/show_bug.cgi?id=5326
- SDL_GameControllerDB: https://github.com/mdqinc/SDL_GameControllerDB/commit/019753034e153aef8fa904f7110c23b57a9d66c5
- systemd: `rules.d/{50-udev-default.rules.in,60-evdev.rules,60-persistent-input.rules,70-joystick.rules,70-uaccess.rules.in}`, `hwdb.d/{60-evdev.hwdb,70-joystick.hwdb}`, `src/udev/{udev-builtin-input_id.c,udev-builtin-keyboard.c}` at https://github.com/systemd/systemd
- libinput: https://gitlab.freedesktop.org/libinput/libinput/-/blob/main/src/evdev.c
- Wine: https://gitlab.winehq.org/wine/wine/-/blob/master/dlls/winebus.sys/main.c · `bus_udev.c`; Proton: https://github.com/ValveSoftware/wine/blob/proton_10.0/dlls/winebus.sys/main.c
- Wine bug: https://bugs.winehq.org/show_bug.cgi?id=55653 · Arch BBS https://bbs.archlinux.org/viewtopic.php?id=306688
- Valve udev: https://github.com/ValveSoftware/steam-devices/blob/master/60-steam-input.rules
- Debian `joystick` 1:1.8.1-2: https://packages.debian.org/trixie/amd64/joystick/filelist · https://sources.debian.org/src/joystick/1:1.8.1-2/debian/joystick.udev · https://manpages.debian.org/trixie/joystick/evdev-joystick.1.en.html
- Distro configs: https://gitlab.archlinux.org/archlinux/packaging/packages/linux/-/raw/main/config.x86_64 · https://gitlab.com/cki-project/kernel-ark/-/raw/os-build/redhat/configs/common/generic/CONFIG_HID_BPF · https://salsa.debian.org/kernel-team/linux/-/raw/debian/latest/debian/config/config · local `/boot/config-6.12.107+deb13-amd64`
- Arch wiki (raw): https://wiki.archlinux.org/title/Gamepad · https://wiki.archlinux.org/title/Mouse_polling_rate
- Steam forum (old SDL2 gamepad): https://steamcommunity.com/app/392160/discussions/0/3003304734671903031/
