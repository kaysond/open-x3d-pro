# Linux input architecture for Open X3D Pro

Research only (no code). All sources read **2026-10-04**. Kernel references are to mainline master, which is **7.3-rc6** (stable 7.2.9, LTS 6.18/6.12/6.6) per <https://www.kernel.org/releases.json>. Kernel files were read from git.kernel.org (`/plain/…?h=<tag>`), or from the github.com/torvalds/linux mirror where git.kernel.org refused WebFetch. Line numbers are master and will drift. Anything not verified from a primary source is marked **UNVERIFIED**.

The question: how do we make the processed stick (calibration → invert → deadzones → 17-point LUT → axis remap → button remap/shift/hat-as-buttons, CONTRACT §4) the one device Linux games see, and hide or replace the raw 046d:c215, without an out-of-tree module, without disabling security features, and without permanent root at runtime?

---

## 0. Answer in one paragraph

**Use HID-BPF (option A), struct_ops API only, kernel ≥ 6.11 with `CONFIG_HID_BPF=y`.**

- A BPF program attached to the real USB HID device rewrites the report descriptor and every input report inside hid-core. This happens *before* hidraw, hid-input/evdev and joydev see anything.
  - Every consumer (SDL, Wine/Proton on all three winebus backends, Steam, X-Plane, FlightGear) sees exactly one device.
  - That device keeps VID/PID 046d:c215, and it can be renamed.
- The program also intercepts hidraw feature-report requests. The app can therefore push the same 240-byte blob through the same feature report ID 3 as on Windows.
- Root is needed only at plug time (a udev rule runs the loader). There is no daemon, nothing runs as root afterwards, and it works under Secure Boot and lockdown.
- The one serious engineering caveat is §2.3: input reports must stay ≤ 7 bytes, the device's wMaxPacketSize. The Linux descriptor therefore splits the joystick state into two 7-byte reports.
- Option B (evdev grab + uinput) cannot hide the raw device from games running as the same user without a separate privileged daemon, and it can never offer a hidraw node.

---

## 1. What has to be reproduced (from `docs/CONTRACT.md` §2, §4, §6 and `driver/pipeline/pipeline.c`)

**Device.** Each raw report is 7 bytes with no report ID:
- X and Y: 10-bit;
- hat: 4-bit;
- Rz: 8-bit;
- buttons 1–8 in byte 4;
- slider: byte 5;
- buttons 9–12 in the low nibble of byte 6.

The device is low-speed, has interrupt-IN endpoint 0x81 with **wMaxPacketSize 7**, bInterval 10 (100 Hz), and a 122-byte report descriptor (docs/research.md, probe of bcdDevice 0x0204). It **stalls GET_REPORT**: never forward GET_FEATURE to the device.

**Windows output.**
- Report ID 1 (14 bytes):
  - X, Y, Rz, Slider as u16 0..65535;
  - hat 0..7 with a null state;
  - 32 buttons.
- Vendor collection:
  - input ID 2: raw[7], seq, flags;
  - feature ID 3: 256 bytes, SET/GET of the blob;
  - feature ID 4: 32 bytes, GET of status including `raw_last[7]`.

**Pipeline.** `pipeline.c` uses doubles. CONTRACT §4.1 explicitly allows Q16 integer arithmetic within ±1 LSB of the Rust float reference, checked by `core/testdata/vectors.json`.

**Config.** The 240-byte `X3D_CONFIG` blob: magic, version, size, CRC32 over bytes 12..239.

**App.** The app does per-application profile switching and pushes the blob only when its CRC changes (§6). Key bindings are done by the app (SendInput on Windows), not by the driver. Both are outside this dimension, except that on Linux the app needs a write path to the "driver".

---

## 2. Option A: HID-BPF

### 2.1 Where the program runs (data path)

All quotes below are from `drivers/hid/hid-core.c`, master (https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/tree/drivers/hid/hid-core.c).

1. `__hid_input_report()` calls `dispatch_hid_bpf_device_event(hid, type, data, &bufsize, &size, …)` first (L2150).
2. Only then does it call the HID driver's `raw_event`, and then `hid_report_raw_event()`.
3. `hid_report_raw_event()` calls `hidraw_report_event(hid, data, size)` (L2103) and `hidinput_report_event()` (L2116) on the *post-BPF* buffer.
   - joydev and evdev both sit on the input device created by hid-input.
   - So **hidraw, evdev (`/dev/input/event*`) and joydev (`/dev/input/js*`) consumers only ever see the modified data**.
   - Documentation/hid/hid-bpf.rst puts it this way: "the rest of the HID stack will work on the modified data". A program that returns a negative value drops the event: "Clients (hidraw, input, LEDs) will **not** see this event."
4. `__hid_device_probe()` (L2803-2824) calls `call_hid_bpf_rdesc_fixup()` before driver matching, and re-runs `hid_set_group()` when the descriptor changed.
   - hidraw serves the *fixed* descriptor (`HIDIOCGRDESC` / sysfs `report_descriptor`).
   - hid-input builds evdev capabilities from it.
5. The feature/output request path is `__hid_hw_raw_request()` (L2537-2558). It calls `dispatch_hid_bpf_raw_requests(…, source, …)` *before* `ll_driver->raw_request`. If the BPF program returns non-zero, the request never reaches the USB device.
   - hidraw passes `source = (u64)file` for `HIDIOCSFEATURE` and `HIDIOCGFEATURE` (drivers/hid/hidraw.c L161, L242).
6. hid-lg stays bound. c215 is in its table with `.driver_data = LG_NOGET` only (drivers/hid/hid-lg.c L874-875). It has no `report_fixup` and no wheel fuzz override for c215. `HID_QUIRK_NOGET` only gates `__usbhid_submit_report()` IN requests (drivers/hid/usbhid/hid-core.c L531), not hidraw `raw_request`.
   - So without our intercept, a hidraw GET_FEATURE *would* reach the stalling device. With the intercept, it never does.

### 2.2 API generations and minimum kernels

| Kernel | What exists | Evidence |
|---|---|---|
| ≤ 6.2 | no HID-BPF | `include/linux/hid_bpf.h?h=v6.2` → 404 |
| **6.3 – 6.10** | "tracing" API: `SEC("fmod_ret/hid_bpf_device_event")`, `fmod_ret/hid_bpf_rdesc_fixup`, attach with the `hid_bpf_attach_prog()` kfunc from a `SEC("syscall")` program; `HID_BPF_PROG_TYPE_*` enum | `Documentation/hid/hid-bpf.rst?h=v6.10` L140-142, L299; `hid_bpf.h?h=v6.10` L77-95 |
| **6.11+** | **struct_ops API**: `struct hid_bpf_ops { hid_id; flags; hid_device_event; hid_rdesc_fixup; hid_hw_request; hid_hw_output_report }`, attached with `bpf_map__attach_struct_ops()` after setting `hid_id`. The tracing API is **removed** (no `HID_BPF_PROG_TYPE` in `hid_bpf.h?h=v6.11`). | `include/linux/hid_bpf.h` L92-176; hid-bpf.rst L152-176, L321-323 |
| 6.11+ | `hid_hw_request` / `hid_hw_output_report` callbacks (intercept hidraw feature/output requests) | absent from `hid_bpf.h?h=v6.10`, present in v6.11 |
| 6.11+ | `hid_bpf_try_input_report()` kfunc, non-sleepable: "can be safely used in IRQ context" | `hid_bpf_dispatch.c` L528-542, L587; absent in v6.10 |
| 6.11+ | writable `hid_device.name/uniq/phys` from struct_ops (`WRITE_RANGE`) | `drivers/hid/bpf/hid_bpf_struct_ops.c` L81-86 (present in v6.11) |

**Must both APIs be supported? No.** udev-hid-bpf does support both: it compiles the same source twice, with and without `HID_BPF_TRACING` (src/bpf/stable/meson.build: "tracing_sources are compatible with kernel v6.3+"; "'sources' are … only compatible with struct_ops (kernel v6.11+)"), and its loader picks `HidBPFTrace` or `HidBPFStructOps` per object (src/bpf.rs `get_bpf_loader`). But our design needs three things that only exist from 6.11:
- `hid_hw_request` for the config channel and to protect the device from GET_FEATURE;
- `hid_bpf_try_input_report` for the ≤ 7-byte split, see §2.3;
- `hid_set_name`.

Every distro that has HID-BPF at all now ships ≥ 6.11 (§2.8). **Target struct_ops only.**

### 2.3 Report sizes: what can grow and what must not

**Descriptor.**
- The `hid_rdesc_fixup` buffer is `HID_MAX_DESCRIPTOR_SIZE` = **4096** bytes (hid-bpf.rst L296-299; `call_hid_bpf_rdesc_fixup()` allocates 4096 and then `krealloc`s to the returned size, hid_bpf_dispatch.c L163-194).
- Returning a larger size than the original 122 bytes is allowed, so a 2-collection descriptor is fine.
- Only one rdesc fixup per device is allowed (a second returns `-EINVAL`).
- Attaching one makes the kernel "immediately disconnect the HID device and do a reprobe" (hid-bpf.rst L304-310). At plug time the input nodes therefore appear, vanish and reappear within the udev run.

**Event buffer.**
- `hid_bpf_ctx.allocated_size` = largest report of the (fixed) descriptor, rounded **up to a multiple of 64** (`__hid_bpf_allocate_data()`, hid_bpf_dispatch.c L228-262). It is allocated once, at first attach. For us that is ≥ 64 bytes.
- A program returns `0` (keep size), a positive value (the new size), or a negative value (drop).
- After all programs run, `size > allocated_size` gives `-EINVAL` (L71-79).
- `ctx.size` is read-write. `ctx.retval` is "return value of the previous program" (hid-bpf.rst).
- So, as far as hid-core is concerned, **a 7-byte raw report can be turned into a 14-byte report**. hid-core then pads short reports (`memset`, hid-core.c L2094-2097) and enforces `bufsize ≥ rsize`.

**The catch (decision-relevant): usbhid sizes the interrupt URB from the *fixed* descriptor.**
- `usbhid_start()` does `hid_find_max_report(hid, HID_INPUT_REPORT, &insize)` and then `usb_fill_int_urb(usbhid->urbin, …, insize, …)` (drivers/hid/usbhid/hid-core.c L1103-1160). There is no clamp to wMaxPacketSize.
- This is deliberate. Commit bf0964dc (2005, "Input: HID - handle multi-transaction reports") sizes buffers "large enough to store any report on the endpoint" so that reports longer than one packet are reassembled.
- Our device sends 7-byte packets with wMaxPacketSize 7, so every packet is full-size, never short.
- If the fixed descriptor declares any input report > 7 bytes, the host controller will most likely keep filling the URB across two 10 ms intervals. Each completion would then carry **two raw reports glued together**: 50 Hz, +10 ms latency, and the second report hidden.
- **UNVERIFIED on hardware**, but it follows from the code and from USB interrupt-transfer completion rules (a transfer ends on a short packet or a full buffer). This is the first thing to test (§8).

**Design that sidesteps it:** keep every *input* report ≤ 7 bytes including the report ID, and emit two reports per raw report.
- In `hid_device_event`, inject report A with `hid_bpf_try_input_report()`, then rewrite the in-place buffer as report B and `return 7`.
  - The kernel selftest `hid_test_multiply_events` (tools/testing/selftests/hid/progs/hid.c) does exactly this from a non-sleepable `hid_device_event`.
- Injected reports re-enter `__hid_input_report` with `source = (u64)ctx` (hid_bpf_dispatch.c L517). Device reports have `source = 0` (`hid_input_report()`, hid-core.c L2200-2205). The program must therefore pass `source != 0` events through untouched.
  - Raw reports have no ID and their byte 0 is X's low byte, so content cannot be used to tell them apart.
- Feature reports are not constrained, because the BPF intercept answers them and they never touch the wire. IDs 3 and 4 can stay byte-identical to Windows.

**Proposed Linux descriptor (for the driver owner to decide):**

| Report | Bytes incl. ID | Content |
|---|---|---|
| Input ID 1 | 7 | X u16, Y u16, Rz u12, Hat u4 (0..7, Null) |
| Input ID 5 | 7 | Slider u16, Buttons 1–32 |
| Feature ID 3 | 256 | blob, SET/GET, identical to Windows |
| Feature ID 4 | 32 | status incl. `raw_last[7]`, identical to Windows |

- Rz at 12 bits loses nothing: the raw value is 8-bit.
- The Windows vendor *input* ID 2 (raw passthrough) does not fit in 7 bytes. The Linux app polls feature ID 4 for raw values instead.
- If hardware testing shows the URB concern is unfounded, the Windows 14-byte ID 1 can be used unchanged.

### 2.4 Identity

- The BPF struct_ops verifier lets programs write only `hid_bpf_ctx.retval`, `hid_device.name`, `.uniq` and `.phys` (hid_bpf_struct_ops.c L81-86).
  - **VID/PID/version cannot change.** That is what we want: SDL's flight-stick list and Wine's identity both key on 046d:c215.
  - udev-hid-bpf wraps the name write as `hid_set_name(hdev, "…")`, struct_ops only (src/bpf/hid_bpf.h L13).
  - Done in `hid_rdesc_fixup` (probe time), the name reaches the input device and `HIDIOCGRAWNAME`.
- **Fuzz/flat.** For Joystick/GamePad applications hid-input sets `fuzz = range>>8, flat = range>>4` (drivers/hid/hid-input.c L1480-1481).
  - The input core's `input_defuzz_abs_event()` (drivers/input/input.c L71-85) then holds the value for changes < fuzz/2 and smooths changes < 2·fuzz. For a u16 axis that is fuzz 255, flat 4095.
  - The stock stick already gets fuzz 3 and flat 63 on 0..1023, so this is not a regression. It is still not what Windows does.
  - Fix without a daemon: a hwdb entry `evdev:input:b0003v046DpC215*` with `EVDEV_ABS_00/01/05/06=:::0:0` (format `min:max:res:fuzz:flat`, systemd hwdb.d/60-evdev.hwdb L46-56).
  - Precedent: hid-lg forces `HID_GD_MULTIAXIS` on its wheels to avoid this (hid-lg.c L696-716).
  - flat matters only to joydev (B fork: joydev.c L975-983) and to SDL with `SDL_JOYSTICK_LINUX_DEADZONES=1`, which is off by default.

### 2.5 Verifier constraints, and what they mean for the pipeline port

- **No floating point.** The BPF ISA defines only integer ALU/JMP ops (Documentation/bpf/standardization/instruction-set.rst has no float instructions).
  - Division by zero is defined: "the destination register is instead set to zero" (L351). Signed `SDIV` needs cpu v4, kernel ≥ 6.6.
  - The pipeline must be integer (Q16 / u64 intermediates). Replicate `ratio()`'s ±1 saturation explicitly rather than relying on div-by-0 → 0.
  - CONTRACT §4.1 already permits this (±1 LSB, checked by vectors.json).
- **Loops.** Bounded loops are fine; the 4 axes × 12 buttons are trivially bounded. The complexity limit is `BPF_COMPLEXITY_LIMIT_INSNS` = 1,000,000 (include/linux/bpf.h L2430).
  - `hid_bpf_get_data(ctx, offset, size)` needs a compile-time-constant size.
- **Maps.** `bpf_map_lookup_elem` / `update_elem` come from `bpf_base_func_proto` (hid_bpf_struct_ops.c L150). An array map holding the 240-byte blob, read on every event, is the normal pattern.
  - To avoid torn reads during an update, use two slots plus an index word.
- **License.** kfunc calls require a GPL-compatible program ("cannot call kernel function from non-GPL compatible program", kernel/bpf/verifier.c L2836). `"Dual MIT/GPL"` is accepted (include/linux/license.h). The `.bpf.c` must carry that string, which is compatible with the MIT repo.
- **Sharing code.** udev-hid-bpf compiles its `.bpf.c` files as native C for pytest (src/bpf/uhid-bpf-test-wrappers.h maps `BPF_PROG`, `bpf_for` and map helpers to plain C). The same integer pipeline source can therefore be unit-tested against `vectors.json` in CI with no kernel. It could optionally replace the doubles in the Windows driver too.

### 2.6 Config channel (app → program without root)

| Option | Kernel | Privilege at runtime | Verdict |
|---|---|---|---|
| **(a) Intercept hidraw feature reports** in `hid_hw_request`. SET ID 3 → validate → copy into the map → return the length (never forwarded). GET ID 3/4 → fill from the map. | 6.11+ | Only rw on `/dev/hidrawN`. We ship a udev rule `SUBSYSTEM=="hidraw", ATTRS{idVendor}=="046d", ATTRS{idProduct}=="c215", TAG+="uaccess"`. By default hidraw is root-only: systemd 70-uaccess.rules.in has no joystick-hidraw rule. | **Recommended.** It mirrors CONTRACT §2.1 exactly, so the app code is the same `hidapi` `send_feature_report(3)` / `get_feature_report(4)` on both OSes, and it also shields the device from GET_FEATURE stalls. Selftest precedent: `hid_test_hidraw_raw_request` (struct_ops.s/hid_hw_request with a `source` check). |
| (b) Pinned BPF map with file permissions | 6.3+ | `BPF_OBJ_GET` needs only path permission (kernel/bpf/inode.c L563 `path_permission`). The unprivileged sysctl gates only map/prog *creation* (syscall.c L1475-1481: "other actions depend on fd availability and access to bpffs"). But systemd mounts `/sys/fs/bpf` with `mode=0700` (src/shared/mount-setup.c L225-230; locally `1700 root`), and udev-hid-bpf pins under `/sys/fs/bpf/hid/<dev>/<obj>/` (src/bpf.rs `BPFFS_ROOT`). A separate bpffs mount with `uid=`/`gid=`/`mode=` (inode.c L983-985) would be needed. | Workable, but it needs an extra root-owned mount and doesn't protect against GET_FEATURE. No. |
| (c) Reload the program with the config baked in | any | root on every profile switch | No. |

- **Persistence.** Maps die with the device. On unplug the HID device is destroyed and the pinned links go stale; udev-hid-bpf's `remove` rule unpins them.
  - Simplest approach: identity until the (autostarted) app sees the hidraw node appear and pushes the active profile. That is the same moment the Windows app would push.
  - The Windows driver persists to the registry. A Linux equivalent would need a root-writable file read by the loader, which is not worth it.

### 2.7 Who loads it, with what privileges; Secure Boot and lockdown

- **Capabilities.**
  - A struct_ops map needs `CAP_BPF` (syscall.c L1514-1519).
  - A struct_ops program is a "perfmon" program type, so it needs `CAP_PERFMON` on top of `CAP_BPF` (syscall.c L2901, L3007).
  - In practice this means root, at load time only.
- **udev-hid-bpf mechanism** (gitlab.freedesktop.org/libevdev/udev-hid-bpf, GPL-2.0-only, releases 2.0.0-20240624 through 2.3.0-20260703):
  - `81-hid-bpf.rules`: on `SUBSYSTEM=="hid"` add, a hwdb lookup `hid-bpf:` → `IMPORT{program}="udev-hid-bpf add $sys$devpath"`; on remove → `udev-hid-bpf remove`.
  - The loader opens the object, injects udev properties and the report descriptor, runs an optional `SEC("syscall") probe`, then attaches and pins the link and maps (src/bpf.rs L1205-1250).
  - `udev-hid-bpf install prog.bpf.o` installs the object into `/etc/udev-hid-bpf` with a rule in `/etc/udev/rules.d`; `--install-exe` also installs the binary (udev-hid-bpf.man).
  - Root is used only during the udev event. Nothing stays resident: the pinned link keeps the program attached.
- **Our options:**
  - (1) depend on the distro's `udev-hid-bpf` and ship only `openx3d.bpf.o` plus a hwdb/rule; or
  - (2) ship our own small loader (Rust, libbpf-rs), called from our own udev rule. Choose (2) where udev-hid-bpf is not packaged, e.g. SteamOS.
- **Lockdown / Secure Boot.**
  - Integrity mode, which Secure Boot enables on Fedora and Ubuntu, blocks only `LOCKDOWN_BPF_WRITE_USER` among BPF features.
  - Confidentiality mode adds `LOCKDOWN_BPF_READ_KERNEL`, `LOCKDOWN_TRACEFS` and `LOCKDOWN_KPROBES` (include/linux/security.h L128-158).
  - HID-BPF uses none of these, so it is unaffected. Nothing needs module signing because there is no module.
  - Only `bpf_printk` → `trace_pipe` debugging is lost under confidentiality mode.
  - Kernel config `CONFIG_LOCK_DOWN_IN_EFI_SECURE_BOOT=y` is the norm (locally on Debian 13).

### 2.8 Distro reach (all checked 2026-10-04)

| Distro | Kernel | `CONFIG_HID_BPF` | udev-hid-bpf package |
|---|---|---|---|
| Fedora (kernel-ark `redhat/configs/common/generic/CONFIG_HID_BPF`) | current 6.x/7.x | **=y** (common, so RHEL-family probably inherits it: **UNVERIFIED**) | f43 2.1.0, f44 2.2.0 (updates-testing), rawhide 2.2.0 (mdapi.fedoraproject.org) |
| Arch (`linux` 7.2.9.arch1 `config.x86_64`) | 7.2 | **=y** | extra 2.3.0.20260703 |
| **Debian 13 trixie** (6.12; local `/boot/config-6.12.107+deb13-amd64`) | 6.12 | **not set** | not in trixie |
| Debian sid/forky; trixie-backports 7.2.6 | 7.x | =y since `linux 6.16.3-1`, "drivers/hid/bpf: Enable HID_BPF (Closes: #1110780)" | forky/sid 2.3.0 |
| Ubuntu | 26.04 resolute / 26.10 | **UNVERIFIED** (launchpad annotations returned 403). Indirect evidence: Ubuntu 25.04's `linux` needed a fix for HID-BPF CVE-2025-38016, which implies the code is built. | resolute 2.1.0, stonking 2.3.0 (Launchpad API) |
| **SteamOS 3.7** (`linux-neptune-611` 6.11.11.valve29 config) | 6.11 | **=y** | not shipped |
| **SteamOS 3.8** (`linux-neptune-618` 6.18.50.valve2 config) | 6.18 | **=y** (also `DEBUG_INFO_BTF=y`, needed for struct_ops) | not shipped |

- SteamOS sources: steamdeck-packages.steamos.cloud/archlinux-mirror/sources/jupiter-3.7 and jupiter-3.8. Secondary sources date 3.8.10 (6.16) to 2026-06-17 and 3.8.28 (6.18.50) to 2026-09-25 (tbreak.com, borncity.com).
- `CONFIG_HID_BPF` depends on `BPF_JIT`, `BPF_SYSCALL` and `DYNAMIC_FTRACE_WITH_DIRECT_CALLS` (drivers/hid/bpf/Kconfig). That last dependency excludes some non-x86/arm64 configs (irrelevant here).
- Upstream already carries flight-stick fixups: FR-TEC Raptor Mach 2 (hat fix) and Thrustmaster TCA Yoke Boeing (drivers/hid/bpf/progs/).

### 2.9 Debugging, dev loop, latency

- **Manual attach.** `sudo udev-hid-bpf add /sys/bus/hid/devices/0003:046D:C215.NNNN openx3d.bpf.o` and `… remove` (man page). `bpftool struct_ops`/`link` and `udev-hid-bpf list-loaded` show state.
  - `bpf_printk` output appears in `/sys/kernel/tracing/trace_pipe` (root).
  - Verifier errors are printed at load. The most common trap is the constant size in `hid_bpf_get_data`.
- **Without the stick.** Run the native-C build of the `.bpf.c` against `vectors.json` (udev-hid-bpf's test-wrapper approach). Kernel-level replay is possible with hid-tools `hid-recorder`/`hid-replay` over uhid, **but** see §5.1: a uhid replay with bus USB 046d:c215 ends up with no driver bound. Replay with a non-USB bus or a test VID/PID.
- **Latency.** The program runs inline in the URB completion ("we are in IRQ context", hid-bpf.rst L163-165). It is JIT-compiled and runs a few hundred instructions. It adds no thread hop and no copy to userspace. Effectively zero against the 10 ms frame (estimate; not measured).

---

## 3. Option B: userspace evdev grab + uinput virtual joystick

(Findings from the B sub-investigation. Kernel from the torvalds/linux master mirror. Crates from crates.io/docs.rs. Projects from their repos.)

### 3.1 Mechanics

- **EVIOCGRAB.** `input_pass_values()` (drivers/input/input.c L111-134) delivers to `dev->grab` *only*. The input core documents that "all events generated by the device are delivered only to this handle".
  - So **joydev `/dev/input/js*`, other evdev readers, mousedev and kbd get nothing** while our daemon holds the grab.
  - Closing the fd releases the grab (evdev.c L442), so the stick fails open if the daemon dies.
  - **The grab does not affect hidraw**: `hidraw_report_event()` is called independently, before hid-input (hid-core.c L2102-2116).
- **uinput.**
  - `UI_DEV_SETUP` takes `struct uinput_setup { struct input_id id; char name[80]; u32 ff_effects_max }`; `UI_ABS_SETUP` takes `struct uinput_abs_setup { code; struct input_absinfo }` (include/uapi/linux/uinput.h). Both exist since uinput protocol 0.5, Linux 4.5.
  - The id is copied verbatim (uinput.c L498), so a clone can be 046d:c215 with the same bus, version and name. That gives SDL the same GUID (bus, CRC16(name), vendor, product, version; SDL_joystick.c `SDL_CreateJoystickGUID`).
  - A uinput device is a plain `input_dev`. **It has no hid_device and therefore no hidraw node.**
  - Set `fuzz = 0` and `flat = 0` explicitly.
- **Codes for 32 buttons.** Mirror hid-input:
  - buttons 1–16 → `BTN_JOYSTICK + n` (0x120–0x12f);
  - buttons 17–32 → `BTN_TRIGGER_HAPPY1..16` (0x2c0–0x2cf) (hid-input.c L795-805).
  - joydev and SDL2/SDL3 both enumerate `BTN_JOYSTICK..KEY_MAX` first, then `0..BTN_JOYSTICK` (joydev.c L953-965; SDL sysjoy L1244-1263). Wine evdev scans from `BTN_MISC` up. So button n is index n−1 everywhere, which matches DirectInput order.
  - Axes: X → `ABS_X`, Y → `ABS_Y`, Rz → `ABS_RZ`, Slider → `ABS_THROTTLE`, hat → `ABS_HAT0X/Y` in -1..1 (hid-input.c L920-962).
- **Rust.**
  - `evdev` 0.13.2 (2025-09-15, pure Rust, libc + nix only) covers `grab()`, `input_id()`, and `VirtualDeviceBuilder` with `input_id`, `with_keys`, `with_absolute_axis(UinputAbsSetup)`, plus a tokio stream.
  - `evdev-rs` 0.6.3 wraps libevdev. The `uinput` crate (0.1.3, 2018) is stale.
- **Reuse.** The pipeline can be `x3d_core::pipeline` as-is: floats are fine in userspace. **There is no integer port.**

### 3.2 The duplicate-device problem (the reason B loses)

- SDL2/SDL3 never check `EVIOCGRAB`, and nor does winebus. SDL opens nodes `O_RDONLY` and lists anything with `ID_INPUT_JOYSTICK`, so **a grabbed raw node is still listed as a frozen joystick**.
  - Under Proton's default SDL path the game sees two "Logitech Extreme 3D" with the same VID/PID, one of them dead.
- SDL's ignore hints match **VID/PID only**: `SDL_GAMECONTROLLER_IGNORE_DEVICES` (it applies to all joysticks via `SDL_ShouldIgnoreJoystick` → `SDL_ShouldIgnoreGamepad`, SDL3 SDL_joystick.c L3736-3753) and `SDL_JOYSTICK_BLACKLIST_DEVICES`. They cannot drop the raw device and keep a same-ID clone.
- Proton 10 dedupes by VID/PID in the opposite direction from what we want. If a hidraw device with that VID/PID exists, it unlinks or refuses the non-hidraw one (ValveSoftware/wine proton_10.0 main.c L446-455, L1031-1052).
  - So a user who has enabled hidraw for c215 gets the *raw* stick, and our clone disappears.
- The only real fix is to make the raw `event*`, `js*` and `hidraw*` nodes **unopenable by the user**. InputPlumber, a Rust root daemon, does exactly this: it writes `/run/udev/rules.d/*-inputplumber-hide-*.rules` with `MODE:="0000"`, `TAG-="uaccess"`, `chmod 000` and `setfacl -b` (src/udev/mod.rs L38-160).
  - **Games run as the same user as any user-session daemon.** So hiding the raw node from games also hides it from our daemon, unless the daemon runs as root or as a dedicated system user with a group ACL.
  - That means B requires a **permanent privileged system service** plus an IPC channel from the app. That violates the brief's "no permanent root" spirit, and adds a service, a socket, polkit and replug logic.
- **Permissions.**
  - `/dev/uinput` is 0600 root by default (systemd 50-udev-default.rules.in has no rule). Valve's steam-devices `60-steam-input.rules` L5 adds `KERNEL=="uinput", … TAG+="uaccess"`; which distros and SteamOS install it by default is **UNVERIFIED**.
  - Event nodes are `GROUP="input"` plus uaccess for `ID_INPUT_JOYSTICK` (70-uaccess.rules.in L72-73). `js*` is `0664`, world-readable.
  - The clone gets `ID_INPUT_JOYSTICK` and uaccess automatically. udev rules can target the raw device via `SUBSYSTEMS=="usb", ATTRS{idVendor}=="046d", ATTRS{idProduct}=="c215"`; uinput nodes have no USB parent.
- **Robustness.**
  - The virtual device can outlive a replug, as evsieve does with `persist=reopen`, so games keep their handle. That is a real plus.
  - Suspend/resume re-enumeration behaviour is **UNVERIFIED**.
- **Latency.** One `read`, the pipeline, and one `write` plus SYN per 10 ms frame: tens of µs (estimate, unmeasured).
- **Containers.** pressure-vessel bind-mounts host `/dev` (steam-runtime-tools pressure-vessel/bwrap.c L397-400), so clones and permissions are visible inside. In containers SDL skips udev and probes nodes directly, so hiding must be done with file modes, not udev properties. Flatpak needs `--device=input` (≥ 1.15.6) or `--device=all`, and cannot create `/dev/uinput` devices without host permissions.
- **Precedents.**
  - input-remapper: grab + uinput, a root D-Bus service behind pkexec.
  - evsieve: grab + uinput, "most likely needs to run as root".
  - InputPlumber: root, with the hide rules above.
  - evmapd: old.
  - No maintained flight-stick curve tool exists on Linux.

---

## 4. How Linux games consume joysticks (part C findings)

### 4.1 Wine / Proton `winebus.sys`

- **Backends.** SDL (dlopen libSDL2), udev **hidraw**, udev **evdev**. Upstream: if SDL initialises, the evdev backend is disabled (`main.c` L1277). Proton runs udev first and defers every evdev node to SDL except 28de:11ff (bus_udev.c L1738-1750).
- **hidraw vs not**, decided per device by `is_hidraw_enabled()` (wine master main.c L476-573):
  - Non-Generic-Desktop collections go hidraw.
  - Joysticks and gamepads go hidraw only if allowlisted: DS4/DS5, some Thrustmaster, all VKB/Virpil, Winwing, … or added via the `EnableHidraw` registry key or the per-device `Devices\VID/PID\Hidraw` key.
  - **046d:c215 is on no list.**
  - History: wine-9.1 (2024-01-18) `EnableHidraw`; 9.5 (2024-03-19) non-game-controllers prefer hidraw; 9.22 (2024-11-18) HOTAS list; 10.13 (2025-08-05) per-device key; 11.4 (2026-03-03) all VKB.
- **Proton 10 (ValveSoftware/wine `proton_10.0`).**
  - Adds `PROTON_ENABLE_HIDRAW` / `PROTON_DISABLE_HIDRAW` (value `1` or a list like `0x046D/0xC215`, main.c L556-583).
  - Also `WINEBUSCONFIG="046d/c215=hidraw"` (main.c L1305-1325).
  - `PROTON_PREFER_SDL` **does not exist** in the Proton 10 script or winebus.
  - Default for c215 on Proton 9/10: **the SDL path**.
- **What the game sees.**
  - *SDL path.* VID/PID and name come from evdev. Axes are **positional**: SDL axis i → X, Y, Z, Rx, Ry, Rz, Slider, Dial (bus_sdl.c L260-269). So the X3D (ABS_X, ABS_Y, ABS_RZ, ABS_THROTTLE) appears in DirectInput as **X, Y, Z, Rx**, not Windows' X, Y, Rz, Slider.
    - Proton issue #7121 (2023-09-22, MSFS: "throttle shows as R-Axis X").
    - SDL's flight-stick list contains `0x046d, 0xc215` (SDL3 SDL_joystick.c L467), so winebus adds a Flight Simulation collection and does not XInput-map it.
    - **Trap:** winebus treats any device with exactly 6 axes and ≥ 14 buttons (≥ 10 with a hat) as a gamepad (bus_sdl.c L988, bus_udev.c L1287). Never expose exactly 6 axes.
  - *evdev path* (upstream without SDL). ABS_RZ → Rz; ABS_THROTTLE → Simulation Throttle → DirectInput Slider. This is closer to Windows.
  - *hidraw path.* The kernel descriptor is passed through verbatim: **Windows-identical layout**, unsigned ranges, the vendor collection as a separate HID device. Under A that descriptor is ours.
- **DCS, IL-2, Elite, MSFS** are Windows-only and run under Proton: DirectInput 8 → wine dinput → HID → winebus. Field issues: Proton #3188 (Elite sees the X3D as a gamepad), #4579 (open, "prevent joysticks from being seen as gamepads"), #7121, #9572 (2026, signed range breaks HidP readers).

### 4.2 SDL2 / SDL3 (native games, and Proton's SDL path)

- evdev backend by default; `SDL_JOYSTICK_LINUX_CLASSIC=1` selects `js*`.
- Enumeration via udev `ID_INPUT_JOYSTICK`; inotify plus capability sniffing inside containers.
- Unreadable nodes are silently skipped. This is the lever for hiding.
- The only "virtual device" exclusion is the xow Xbox pad (sysjoy L221-235).
- HIDAPI is on by default, but its drivers are gamepad/wheel-only (SDL_hidapijoystick.c L40-114; Lg4ff wheels only). **c215 is never claimed by HIDAPI.**

### 4.3 Steam client / Steam Input

- steam-devices grants hidraw uaccess only to Valve/Sony/Nintendo-class pads; no 046d. systemd grants none for joystick hidraw. So Steam cannot open the X3D's hidraw on stock systems.
- Steamworks "Steam Input gamepad emulation – best practices": games with flight-stick support should leave Generic/DirectInput controllers unchecked, "because Steam doesn't support remapping those devices".
- **UNVERIFIED:** whether Steam ever puts 046d/c215 into `SDL_GAMECONTROLLER_IGNORE_DEVICES` when "Generic gamepad support" is on. If it does, A and B are hit equally, because matching is by VID/PID.

### 4.4 Native titles

- **X-Plane** uses evdev `/dev/input/event*`: Laminar dev blog, 2012-09-28 (10.10 moved from the joystick to the input interface) and 2017-10-20 (11.10+ treats any axis/button/hat device as a joystick); post URLs not captured. X-Plane 12 specifics are **UNVERIFIED**.
- **FlightGear** uses joydev `js*` (gitlab.com/flightgear/flightgear `3rdparty/joystick/jsLinux.cxx` L109).
- libinput explicitly ignores joysticks: "device is a joystick or a gamepad, ignoring" (libinput src/evdev.c L1937-1940). It is not part of the problem.

### 4.5 Per-consumer outcome

| Consumer | A: HID-BPF | B: grab + uinput clone |
|---|---|---|
| Native SDL2/3 (evdev) | one device, processed, 046d:c215 | clone plus a **dead raw duplicate**, unless the raw node is made unopenable |
| Proton, SDL path (default) | one device; X, Y, Z, Rx layout (same as stock today) | same layout for the clone, plus a dead duplicate unless hidden |
| Proton, hidraw path (opt-in `PROTON_ENABLE_HIDRAW=0x046D/0xC215` + uaccess rule) | **one device, Windows layout** (our descriptor); the SDL twin is auto-dropped | **broken**: the raw hidraw wins and the clone is dropped |
| Upstream Wine, evdev path | one device | duplicate unless hidden |
| X-Plane (evdev) | one device | duplicate unless hidden |
| FlightGear (joydev) | one device | grab silences raw js; the clone has its own js node |
| Any hidraw reader | the modified device | always the raw device |

---

## 5. Option D: anything else?

### 5.1 UHID daemon (`/dev/uhid`): the only credible alternative to A

- `UHID_CREATE2` takes `name[128], phys, uniq, rd_size, bus, vendor, product, version, country, rd_data[HID_MAX_DESCRIPTOR_SIZE]` (include/uapi/linux/uhid.h L45-56).
  - The daemon publishes **our exact Windows 14-byte descriptor** as a real HID device, with evdev, js *and* hidraw nodes.
  - `UHID_GET_REPORT` / `UHID_SET_REPORT` arrive in the daemon (Documentation/hid/uhid.rst L171-189), so the feature-report config channel works as on Windows.
  - No kernel-version constraint, and floats are fine.
- **Gotcha 1 (verified in source, not on hardware):** with `bus=BUS_USB, 046d:c215`, hid-lg matches by ID, but `lg_probe()` starts with `if (!hid_is_usb(hdev)) return -EINVAL;` (hid-lg.c L758-759).
  - hid-generic declines any device another driver's ID table matches (`hid_generic_match()`/`__check_hid_generic`, hid-generic.c L37-56).
  - Result: **an unbound device with no nodes**. Use `BUS_VIRTUAL`; Wine and SDL identity is by VID/PID, though SDL's GUID will differ from the real stick's.
- **Gotcha 2:** `/dev/uhid` is root-only. No systemd or steam-devices rule touches it (grep of 50-udev-default, 70-uaccess and 60-steam-input.rules).
- **Gotcha 3:** it still has B's whole hide-the-raw-device problem. That means a privileged system daemon (root or a dedicated user owning the raw nodes and `/dev/uhid`).
- **Use it** only as the fallback for kernels without `CONFIG_HID_BPF`, e.g. Debian 13 stable, if that reach ever matters. It beats uinput as a fallback because it keeps the hidraw/Proton path and the Windows descriptor.

### 5.2 Partial, no-daemon knobs (complements, not solutions)

- **hwdb `EVDEV_ABS_xx=min:max:res:fuzz:flat`** (60-evdev.hwdb) or `EVIOCSABS` (linuxconsoletools `evdev-joystick`).
  - These can change calibration range, fuzz and flat on the evdev node.
  - Wine evdev reads only min/max. SDL ignores flat unless `SDL_JOYSTICK_LINUX_DEADZONES=1`. Only joydev honours flat.
  - No curves, no remap, nothing for hidraw. Useful with A only to zero fuzz/flat.
- **joydev `JSIOCSCORR`** (jscal): broken-line deadzone and gain on `js*` only, so FlightGear and nothing else.
- **hid quirks** (`usbhid.quirks=`, `HID_QUIRK_*`): flags only (NOGET, IGNORE, …), no transforms. **hid-lg module params**: only `lg4ff_no_autoswitch` (hid-lg.c L941), nothing for c215.
- **libinput**: ignores joysticks (above).

---

## 6. Decision table

| Criterion | **A: HID-BPF (struct_ops)** | B: grab + uinput | D: UHID daemon |
|---|---|---|---|
| Kernel / distro reach | needs ≥ 6.11 and `HID_BPF=y`. Yes: Fedora, Arch, Debian sid/forky/trixie-bpo, SteamOS 3.7/3.8. **No: Debian 13 stock.** Ubuntu UNVERIFIED. | any kernel ≥ 4.5 | any kernel with uhid |
| Out-of-tree module / security knobs | none / none (fine under SB + lockdown integrity) | none / none | none / none |
| Root at runtime | **none**: udev runs the loader once at plug; the app is a normal user process | **permanent privileged daemon**, needed to hide raw nodes from same-user games and to open `/dev/uinput` | permanent privileged daemon |
| One-time root install | `.bpf.o` + udev rule (+ loader) + hidraw uaccess rule + hwdb fuzz override | udev hide rules + service + group/polkit | same as B |
| Identity | real device, 046d:c215, real bus, renamable | clone can copy bus/VID/PID/name exactly (same SDL GUID) | 046d:c215 but must use `BUS_VIRTUAL` (§5.1) |
| Duplicate devices | **none, by construction** | raw shows as a dead duplicate in SDL/Wine unless chmod-hidden; Proton's hidraw dedupe kills the clone | same as B |
| Proton Windows-identical layout (hidraw path) | **yes** (opt-in env/registry + our uaccess rule) | **impossible** (no hidraw) | yes |
| Config channel | hidraw feature report 3/4 intercepted in `hid_hw_request`: **same app code as Windows** | in-process / IPC to the daemon | UHID GET/SET_REPORT: same app code as Windows |
| Output layout vs Windows | same usages; split into 2×7-byte input reports (§2.3); Rz 12-bit; raw passthrough via feature 4 | full 16-bit, 32 buttons | identical to Windows |
| Pipeline port | **integer port required** (Q16, ±1 LSB, testable natively against vectors.json) | none (reuse `x3d_core`) | none |
| Dev effort (estimate) | BPF C ~300–400 lines + loader/rules + Linux app backend; verifier learning curve | Rust daemon ~400 lines + hide rules + service + IPC + replug | as B, plus descriptor/report handling |
| Unplug/replug | reattached by udev at every plug; config re-pushed by the app | virtual device survives replug (plus) | can survive replug |
| Latency | ~0 (inline, IRQ context) | +tens of µs | +tens of µs |
| Debuggability | verifier messages, `bpf_printk` (root), native-C unit tests; kernel-side bugs are harder | ordinary userspace (easy) | ordinary userspace |
| Steam Deck fit | kernels have `HID_BPF=y` (verified); no daemon in Game Mode; root install of rules in desktop mode; read-only `/usr` → loader lives under `/etc` or `/var` (persistence across updates UNVERIFIED) | daemon must run in Game Mode; uinput uaccess depends on steam-devices (UNVERIFIED on SteamOS) | as B |
| Failure mode | if the loader fails, the stock stick still works (no BPF = no change) | if the daemon dies, the grab is released, but hidden raw nodes stay hidden → **no stick at all** until rules are reverted | same as B |

---

## 7. Recommendation

1. **Build A**: one `openx3d.bpf.c` (license `"Dual MIT/GPL"`), struct_ops, kernel ≥ 6.11. It contains:
   - `hid_rdesc_fixup` (sleepable is fine): the Linux descriptor (§2.3 table), plus `hid_set_name("Logitech Extreme 3D Pro (Open X3D)")`.
   - `hid_device_event`:
     - if `source != 0`, pass through (our own injected report);
     - otherwise decode the 7 raw bytes, run the integer pipeline from the map blob, `hid_bpf_try_input_report(ID 1)`, rewrite the buffer as ID 5, and `return 7`;
     - stash the raw bytes and seq for feature 4.
   - `hid_hw_request`:
     - SET_FEATURE 3 → check magic/version/size (and CRC, or trust the app's CRC) → double-buffered map write → return the length;
     - GET_FEATURE 3/4 → answer from the map;
     - anything else aimed at the device → `-EIO` (never let GET_REPORT reach the stalling device).
2. **Install, once, as root** (distro package or install script):
   - the `.bpf.o`;
   - a udev rule to load it (distro `udev-hid-bpf` where packaged, our own loader elsewhere);
   - `SUBSYSTEM=="hidraw", ATTRS{idVendor}=="046d", ATTRS{idProduct}=="c215", TAG+="uaccess"`;
   - a hwdb file zeroing fuzz/flat on `ABS_X/Y/RZ/THROTTLE`.

   After that nothing runs as root.
3. **App (Linux backend).** Use `hidapi` on hidraw for feature reports 3 and 4, with the same code as Windows. Watch udev for the hidraw node and push the active blob when it appears. Document `PROTON_ENABLE_HIDRAW=0x046D/0xC215` (Steam launch option) for the Windows-identical DirectInput layout. Without it, Proton presents X, Y, Z, Rx, as it does for the stock stick today.
4. **Port the pipeline to integer C** shared by the BPF program and a host test against `vectors.json` (±1 LSB). Optionally adopt it in the Windows driver too, giving one implementation.
5. **Don't build B.** If reach to non-HID-BPF kernels (Debian 13 stock) is ever required, the fallback is the UHID system daemon (§5.1), not uinput, and it inherits B's privileged-daemon cost. Debian 13 users can instead use the trixie-backports kernel (7.2.6, `HID_BPF=y`).

---

## 8. Must be verified on a Linux machine with the stick

1. **URB framing (blocks the descriptor design).**
   - Attach a test rdesc fixup that declares a single 14-byte input report. Check with `evtest` / `hid-recorder` timestamps whether events still arrive at 100 Hz, or at 50 Hz with two raw reports per URB.
   - Then confirm the 2×7-byte split (`hid_bpf_try_input_report` + in-place) delivers both reports every 10 ms.
2. **`hid_hw_request` intercept.** `HIDIOCSFEATURE`/`HIDIOCGFEATURE` on report 3/4 through hidraw reach BPF, return our data, and the device never sees a GET_REPORT (`usbmon` shows no control IN to the device).
3. **Reprobe at attach.** Nodes are recreated cleanly. `udevadm info` on the new event node shows `ID_INPUT_JOYSTICK=1`, uaccess, our name, and hwdb fuzz/flat = 0 (`evtest` header).
4. **Button and axis order.** `evtest` shows 0x120–0x12f + 0x2c0–0x2cf. SDL (`testcontroller`) and joydev (`jstest`) show 32 buttons in order, with the hat on ABS_HAT0X/Y and Slider on ABS_THROTTLE.
5. **Proton, default SDL path.** In a DirectInput test app (e.g. DIView or DCS controls), exactly one device appears. Check its axis assignment (expected X, Y, Z, Rx) and that it is not mis-detected as a gamepad.
6. **Proton hidraw path.** With `PROTON_ENABLE_HIDRAW=0x046D/0xC215` (as a launch option, and via `environment.d` if relevant), Wine's hidclass splits our two collections, DirectInput shows X, Y, Rz, Slider, hat and 32 buttons, and the SDL twin is dropped.
7. **Ubuntu.** `grep HID_BPF /boot/config-$(uname -r)` on 24.04 HWE, 26.04 and 26.10.
8. **SteamOS 3.7/3.8.**
   - Our udev rule, hwdb and loader under `/etc` or `/var` survive an OS update.
   - Whether `60-steam-input.rules` (uinput uaccess) is present; only relevant to fallbacks.
   - Default hidraw permissions.
9. **Hotplug and suspend.** Unplug/replug and suspend/resume reattach the program, and the app re-pushes the blob. No stale pins remain under `/sys/fs/bpf/hid/`.
10. **Lockdown.** On Fedora with Secure Boot (lockdown=integrity), the loader works unchanged.
11. **Full-speed revision (bcdDevice 57.11).** wMaxPacketSize and report layout match. If the packet size is larger, the 14-byte single report may be fine on that revision.
12. **Steam "Generic gamepad support" on.** Does Steam hide or wrap 046d:c215 (`SDL_GAMECONTROLLER_IGNORE_DEVICES` in the game's environment)?
13. **X-Plane 12.** It still reads evdev and sees exactly one device.

---

## 9. Primary sources (retrieved 2026-10-04)

- Kernel (git.kernel.org torvalds/linux, master = 7.3-rc6, plus tags v6.2/v6.3/v6.10/v6.11/v6.12):
  - `Documentation/hid/hid-bpf.rst`, `Documentation/hid/uhid.rst`
  - `drivers/hid/bpf/{hid_bpf_dispatch.c, hid_bpf_struct_ops.c, Kconfig, progs/}`, `include/linux/hid_bpf.h`
  - `drivers/hid/{hid-core.c, hid-input.c, hid-lg.c, hid-generic.c, hidraw.c, usbhid/hid-core.c}`
  - `drivers/input/{input.c, evdev.c, joydev.c}`, `include/uapi/linux/{uhid.h, uinput.h}`
  - `kernel/bpf/{syscall.c, inode.c, verifier.c}`, `include/linux/{bpf.h, security.h, license.h}`
  - `Documentation/bpf/standardization/instruction-set.rst`, `Documentation/driver-api/usb/URB.rst`
  - `tools/testing/selftests/hid/progs/hid.c`
  - commit bf0964dc (2005)
- udev-hid-bpf: https://gitlab.freedesktop.org/libevdev/udev-hid-bpf (README, `81-hid-bpf.rules.in`, `udev-hid-bpf.man`, `src/bpf.rs`, `src/bpf/hid_bpf.h`, `src/bpf/stable/meson.build`, releases API); docs https://libevdev.pages.freedesktop.org/udev-hid-bpf/
- systemd: https://github.com/systemd/systemd (`rules.d/50-udev-default.rules.in`, `rules.d/70-uaccess.rules.in`, `hwdb.d/60-evdev.hwdb`, `src/shared/mount-setup.c`)
- Distro configs:
  - Fedora: https://gitlab.com/cki-project/kernel-ark/-/raw/os-build/redhat/configs/common/generic/CONFIG_HID_BPF
  - Arch: https://gitlab.archlinux.org/archlinux/packaging/packages/linux/-/raw/main/config.x86_64
  - Debian: https://salsa.debian.org/kernel-team/linux (`debian/latest` `debian/config/config` and changelog; `debian/6.12/trixie`); local `/boot/config-6.12.107+deb13-amd64`
  - SteamOS: https://steamdeck-packages.steamos.cloud/archlinux-mirror/sources/ (`jupiter-3.7/linux-neptune-611-6.11.11.valve29-1`, `jupiter-3.8/linux-neptune-618-6.18.50.valve2-1`)
- Packages: mdapi.fedoraproject.org, archlinux.org/packages, sources.debian.org/api, api.launchpad.net (udev-hid-bpf, linux)
- Ubuntu CVE-2025-38016 status: https://ubuntu.com/security/CVE-2025-38016
- Wine: https://gitlab.winehq.org/wine/wine/-/tree/master/dlls/winebus.sys (`main.c`, `bus_udev.c`, `bus_sdl.c`); `dlls/dinput/joystick_hid.c`
- Proton: https://github.com/ValveSoftware/wine/tree/proton_10.0/dlls/winebus.sys; https://github.com/ValveSoftware/Proton (README, `proton`; issues #3188, #4579, #7121, #9572)
- SDL: https://github.com/libsdl-org/SDL (`src/joystick/linux/SDL_sysjoystick.c`, `SDL_joystick.c`, `SDL_gamepad.c`, `hidapi/SDL_hidapijoystick.c`, `include/SDL3/SDL_hints.h`), SDL2 branch
- Valve steam-devices: https://github.com/ValveSoftware/steam-devices/blob/master/60-steam-input.rules
- Steamworks: https://partner.steamgames.com/doc/features/steam_controller/steam_input_gamepad_emulation_bestpractices
- pressure-vessel: https://gitlab.steamos.cloud/steamrt/steam-runtime-tools (`pressure-vessel/bwrap.c`); Flatpak: https://docs.flatpak.org/en/latest/sandbox-permissions.html
- Precedents:
  - https://github.com/ShadowBlip/InputPlumber (`src/udev/mod.rs`)
  - https://github.com/KarsMulder/evsieve
  - https://github.com/sezanzeb/input-remapper
  - linuxconsoletools `utils/evdev-joystick.c`
- Rust crates: https://docs.rs/evdev/0.13.2, https://crates.io/crates/evdev-rs
- libinput: https://gitlab.freedesktop.org/libinput/libinput/-/blob/main/src/evdev.c
- FlightGear: https://gitlab.com/flightgear/flightgear/-/raw/next/3rdparty/joystick/jsLinux.cxx
- SteamOS kernel versions (secondary): https://tbreak.com/steamos-3-8-steam-machine-handheld-support/, https://borncity.com/news/steamos-3-8-28-valve-erweitert-handheld-support-und-aktualisiert-firmware/
