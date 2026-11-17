# Open X3D Pro: codebase walkthrough

A tour of every tracked file, in the order that makes the architecture easiest to absorb.
Written against `main` @ e190f23, plus the release workflow change that followed.

## The 60-second picture

Open X3D Pro replaces Logitech Gaming Software for the Extreme 3D Pro joystick. Three parts:

```
 joystick ──USB──▶ driver (C, UMDF 2)  ──HID joystick (report 1)──▶ games
                     │  ▲
        raw reports  │  │ 240-byte config blob (feature report 3)
        (report 2)   ▼  │
                   tray app backend (Rust, Tauri) ◀──IPC──▶ UI (React)
```

- **Driver** (`driver/`): takes over the stick, runs each 7-byte report through a pipeline
  (calibration, deadzones, curve, remap) and publishes a virtual 32-button joystick plus a
  vendor channel for the app.
- **Core crate** (`core/`): Rust definitions shared by everything: config types, the
  *reference* pipeline, the blob compiler, HID constants, and the test vectors that keep the
  C driver honest.
- **App** (`app/`): Tauri tray agent (`app/src-tauri/`, Rust) that stores profiles, picks
  the active one by foreground/running game, and pushes its compiled blob to the driver;
  plus the React UI (`app/src/`) for editing.

The same pipeline exists three times on purpose: Rust float reference (`core`), C in the
driver, and a TypeScript port of the axis math for the UI preview.
`core/testdata/vectors.txt` ties Rust and C together within ±1 LSB.

Code comments cite "CONTRACT §n". That spec lived in `docs/CONTRACT.md`, which was removed
from the repository in c710869; you may still have a local copy.

Suggested reading order: core → driver → backend → frontend → installer → CI.

---

## 1. Workspace root

- [Cargo.toml](Cargo.toml): Cargo workspace with two members, `core` and `app/src-tauri`.
  Holds the shared version (`[workspace.package]`) and the release profile (LTO, strip).
- [Cargo.lock](Cargo.lock): pinned Rust dependencies. Generated.
- [rust-toolchain.toml](rust-toolchain.toml): pins Rust 1.98.1 with rustfmt and clippy; CI
  runs `rustup toolchain install` to honor it.
- [rustfmt.toml](rustfmt.toml): formatter settings for `cargo fmt`.
- [README.md](README.md): user-facing overview, installation, and the long "Driver signing"
  and "Verifying a download" sections that the installer and release notes link to. The
  release certificate fingerprint listed here is checked by CI.
- [CONTRIBUTING.md](CONTRIBUTING.md): architecture, repository layout and build steps.
- [release-signing-cert.pem](release-signing-cert.pem): public half of the self-signed
  certificate that signs release installers, so users can verify a download.
- [LICENSE](LICENSE): MIT.
- [.gitignore](.gitignore): build outputs (Rust, Node, MSBuild), signing artefacts
  (`*.cat`, `*.cer`, `*.pfx`), and staged driver binaries under `app/src-tauri/resources/driver/`.
- [.gitattributes](.gitattributes): LF everywhere except `.ps1`/`.inf`/`.nsh`, which must be
  CRLF for Windows tooling.
- [.editorconfig](.editorconfig): 4 spaces for Rust/C/PowerShell, 2 for TS/JSON/YAML/Markdown.

## 2. Core crate: `core/` (start here)

Pure Rust, no platform code, runs on any OS. Everything else depends on its definitions.

- [core/Cargo.toml](core/Cargo.toml): crate `x3d-core`; only `serde` (`serde_json` for tests).
- [core/src/lib.rs](core/src/lib.rs): declares the five modules below.
- [core/src/config.rs](core/src/config.rs): the human-level data model, serialized as
  camelCase JSON to profile files and to the UI.
  - `AxisId` (x, y, rz, slider), `Calibration`, `Deadzone`, `Curve` (linear, or points on
    the 1/16 grid, mirrored for negative deflection). Every axis, throttle included, is
    symmetric about its calibrated center. `DeviceConfig::upgrade` converts v0.1.0 profiles
    (full-range throttle, exponent curves, off-grid points).
  - `AxisConfig` with `identity()` and `validate()` (ranges, strictly increasing curve
    points, deadzones that leave a live range).
  - `DeviceConfig`: four axes, `HatMode`, 12 `ButtonBinding`s, optional shift button and a
    shifted button layer, and `KeyBinding`s (button → keyboard chord).
    `DeviceConfig::validate()` is the gate every saved or previewed config passes.
  - `Profile` (id, name, exe paths, default flag, config) and `Settings` (autostart,
    switching paused, `MatchMode`).
- [core/src/pipeline.rs](core/src/pipeline.rs): the **reference pipeline** in floating
  point.
  - `RawReport::parse` / `to_bytes`: bit layout of the stick's 7-byte report (10-bit X/Y,
    8-bit twist and throttle, 4-bit hat, 12 buttons). `raw16()` scales axes to 16 bits.
  - `build_lut`: turns a `Curve` + sensitivity into a 17-point lookup table (0..32767).
  - `process_axis`: calibrate → invert → deadzones → LUT, output -32767..32767.
  - `process`: whole report. Routes each physical axis to its target, applies the shift
    layer and button map, converts the hat to buttons 13..16 when asked. Returns `JoyReport`.
- [core/src/blob.rs](core/src/blob.rs): the 240-byte `X3D_CONFIG` wire format the driver
  consumes.
  - `CompiledConfig::from_config` flattens a `DeviceConfig` into numbers (permille
    deadzones, precomputed LUTs); `to_bytes` writes header (magic "X3DP", version, size,
    CRC-32) + four 50-byte axes + hat/shift/button maps.
  - `compile` is the one-call shortcut; `blob_crc` reads the header CRC (used to decide
    whether the device needs a push); `crc32` is a bitwise IEEE CRC. There is no Rust
    parser: validating blobs is the driver's job (`pipeline_validate`).
- [core/src/hid.rs](core/src/hid.rs): the HID interface the driver publishes, from the
  app's side. VID/PID, usage pages, report IDs and lengths, and codecs:
  `VendorInput::parse` (report 2: raw bytes + sequence), `DriverInfo::parse` (feature
  report 4: version, active config CRC), `parse_joystick_report` (report 1),
  `config_report` (wraps a blob as feature report 3). Its tests parse the driver's
  `descriptor.h` and check these lengths against it.
- [core/src/vectors.rs](core/src/vectors.rs): generates the cross-language test vectors.
  `scenarios()` lists named configs (inversion, deadzones, curves, swapped targets, shift
  layer…), `raw_cases()` lists input frames, `generate()` runs every pair through the
  reference pipeline and emits a line format: `V <name> <blob hex>` per scenario, then
  `C <raw hex> <x> <y> <rz> <slider> <hat> <buttons>` per frame. A test fails if the
  committed file is stale.
- [core/src/bin/gen-vectors.rs](core/src/bin/gen-vectors.rs): `cargo run -p x3d-core --bin
  gen-vectors` rewrites the committed vectors. Run it whenever pipeline or blob changes.
- [core/testdata/vectors.txt](core/testdata/vectors.txt): generated output: per scenario
  a hex blob and expected outputs per raw frame. Consumed by the C driver test.

## 3. Driver: `driver/`

A UMDF 2 (user-mode) HID minidriver in C. No kernel code, so Secure Boot and Memory
Integrity stay on. Device stack: `hidclass + mshidumdf → WUDFRd → openx3d.dll → USB`.

- [driver/README.md](driver/README.md): how the driver binds, build/package/install steps,
  the go/no-go test on a real machine, and known limits. Read before touching the INF.
- [driver/.gitignore](driver/.gitignore): ignores the host test binary.

### 3.1 Portable pipeline: `driver/pipeline/`

Plain C with no Windows headers, so it compiles on Linux for testing.

- [driver/pipeline/pipeline.h](driver/pipeline/pipeline.h): packed structs `X3D_AXIS_CONFIG`
  (50 bytes), `X3D_CONFIG` (240 bytes, mirrors `core/src/blob.rs`) and `X3D_JOY_REPORT`
  (14 bytes), with static size asserts, plus constants for report IDs, modes and hat values.
- [driver/pipeline/pipeline.c](driver/pipeline/pipeline.c): the C twin of
  `core/src/pipeline.rs`.
  - `pipeline_validate`: header, CRC and every field range. The trust boundary for blobs
    arriving from user space or the registry.
  - `pipeline_identity`: the built-in pass-through config.
  - `pipeline_process`: one raw report in, one joystick report out.
  - `pipeline_crc32`: same CRC as the Rust side.

### 3.2 The driver proper: `driver/openx3d/`

- [driver/openx3d/driver.h](driver/openx3d/driver.h): shared includes, the `X3D_LOG` debug
  macro, and prototypes grouped by source file. Good map of the driver.
- [driver/openx3d/device.h](driver/openx3d/device.h): `DEVICE_CONTEXT` (per-device state:
  USB handles, queues, locks, active config, last raw/joy reports, sequence counter, flags)
  and report size constants.
- [driver/openx3d/driver.c](driver/openx3d/driver.c): `DriverEntry` and `EvtDeviceAdd`.
  Marks itself a filter, creates the device, its spin lock and wait lock, the default
  parallel IOCTL queue and a manual queue for pending reads, then loads the stored config
  or falls back to identity.
- [driver/openx3d/usb.c](driver/openx3d/usb.c): the USB side. `EvtDevicePrepareHardware`
  selects the configuration, finds the interrupt-IN pipe and sets up a continuous reader;
  `EvtDeviceD0Entry`/`Exit` start and stop it; `EvtUsbReadComplete` hands each 7-byte
  report to `DeviceOnUsbReport`.
- [driver/openx3d/device.c](driver/openx3d/device.c): state transitions under the spin
  lock. `DeviceApplyConfig` swaps the active config; `DeviceOnUsbReport` runs the pipeline
  and bumps the sequence; `DeviceBuildInputReport` formats report 1 or 2;
  `DeviceCompletePendingReads` implements the policy for feeding both HID collections from
  one stream of pending reads (read the comment block above it).
- [driver/openx3d/hid.c](driver/openx3d/hid.c): `EvtIoDeviceControl`, the IOCTL switch
  hidclass talks to. Serves the HID/report descriptors, attributes and strings, parks
  `READ_REPORT` requests, answers `GET_INPUT_REPORT`, and implements the vendor feature
  reports: `GetFeature` (config blob, driver info) and `SetFeature` (validate → persist →
  apply). The `GetXferPacketToRead/Write` helpers follow Microsoft's vhidmini2 sample.
- [driver/openx3d/descriptor.h](driver/openx3d/descriptor.h): the HID report descriptor
  (joystick collection + vendor collection) and HID descriptor. The only copy: the tests
  in `core/src/hid.rs` parse this file, so keep one item per line with `0x` bytes.
- [driver/openx3d/registry.c](driver/openx3d/registry.c): `RegistryLoadConfig` /
  `RegistrySaveConfig` persist the blob as `REG_BINARY "Config"` under the device's
  hardware key, so the last config survives reboots without the app running.
- [driver/openx3d/openx3d.inf](driver/openx3d/openx3d.inf): install file. Matches
  `USB\VID_046D&PID_C215`, with separate Windows 10 and Windows 11 sections (explicit
  services vs. including the inbox INFs). The hardware-ID ranking comments explain how it
  outranks the inbox driver and LGS.
- [driver/openx3d/openx3d.rc](driver/openx3d/openx3d.rc): version resource for the DLL.
  Its version is checked against the tag at release time.
- [driver/openx3d/openx3d.vcxproj](driver/openx3d/openx3d.vcxproj): MSBuild project (x64,
  UMDF 2.15, warnings as errors, compiled as C). Signing is off here on purpose.

### 3.3 Test and packaging

- [driver/test/test_pipeline.c](driver/test/test_pipeline.c): host test. `run_file` reads
  `vectors.txt` line by line with `sscanf`, validates each blob, runs every case through
  `pipeline_process` and compares (axes ±1, hat/buttons exact). `self_checks()` covers the
  CRC check value and blobs that must be rejected.
- [driver/test/Makefile](driver/test/Makefile): `make -C driver/test` builds and runs the
  test.
- [driver/scripts/package.ps1](driver/scripts/package.ps1): stages `openx3d.dll` + INF and
  runs Inf2Cat to produce the **unsigned** catalog.
- [driver/scripts/sign-catalog.ps1](driver/scripts/sign-catalog.ps1): runs on the *user's*
  PC from the installer. Creates a self-signed code-signing certificate, signs the
  catalog, deletes the private key, then trusts the certificate. `-Remove` undoes it.
  This is the project's answer to driver signing without a paid certificate.

## 4. App backend: `app/src-tauri/`

The tray agent, crate `open-x3d-pro`. Platform-neutral logic compiles and is tested on
Linux; Tauri, HID and Win32 code is behind `cfg(windows)`.

- [app/src-tauri/README.md](app/src-tauri/README.md): running, logging, where data lives,
  and fallback mode without the driver.
- [app/src-tauri/Cargo.toml](app/src-tauri/Cargo.toml): dependencies. Note the
  `cfg(windows)` block: Tauri, hidapi and windows-sys are Windows-only.
- [app/src-tauri/build.rs](app/src-tauri/build.rs): runs `tauri_build` on Windows only.
- [app/src-tauri/tauri.conf.json](app/src-tauri/tauri.conf.json): window (starts hidden),
  CSP, NSIS bundle settings (per-machine install, `windows/hooks.nsh` as installer hooks).
  It has no `version`: Tauri takes it from `Cargo.toml`.
- [app/src-tauri/capabilities/default.json](app/src-tauri/capabilities/default.json): what
  the webview may call.
- [app/src-tauri/src/main.rs](app/src-tauri/src/main.rs): entry point; calls `run()` on
  Windows, prints a hint elsewhere.
- [app/src-tauri/src/lib.rs](app/src-tauri/src/lib.rs): module list and the `app` module:
  `run()` builds the Tauri app (single-instance and autostart plugins, command handlers,
  hide-on-close), `setup()` loads settings and profiles and starts the device and watcher
  threads, `notify()` is the single place that emits `state` / `profile_switched` events
  and refreshes the tray.
- [app/src-tauri/src/state.rs](app/src-tauri/src/state.rs): `Inner` (store, settings,
  switcher, device status, calibrator, chords) behind `Shared`, the one mutex all threads
  use. Also the IPC payload types `AppState` and `WindowInfo`. `Inner::reevaluate()` reruns
  profile selection.
- [app/src-tauri/src/commands.rs](app/src-tauri/src/commands.rs): every `#[tauri::command]`
  the UI can call. All thin: the `mutate()` helper locks, applies a closure, re-evaluates
  the active profile and notifies. `save_settings` is the one with real logic (autostart
  change with rollback).
- [app/src-tauri/src/profiles.rs](app/src-tauri/src/profiles.rs): `ProfileStore`, one JSON
  file per profile, exactly one default. `load` skips bad files and repairs the default
  flag; `save`, `delete`, `set_default`, `import`. `write_atomic` (write, fsync,
  rename) is reused by settings. `validate_config` adds key-name checks on top of core's
  validation. `StoreError` is the error type.
- [app/src-tauri/src/settings.rs](app/src-tauri/src/settings.rs): `load` (also reports
  first run) and `save` for `settings.json`.
- [app/src-tauri/src/switcher.rs](app/src-tauri/src/switcher.rs): `Switcher` decides the
  active profile: manual override > held (paused or previewing) > foreground/running match
  > default. `evaluate()` returns a `ProfileSwitched` when it changes and recompiles the
  blob; `take_push()` hands the device thread a blob only when its CRC differs from what
  the driver has. `match_profile` does full-path-then-basename matching.
- [app/src-tauri/src/device.rs](app/src-tauri/src/device.rs): the device thread. `open`
  finds the driver's vendor collection or falls back to the stock joystick; `session` loops:
  push config if due, read a report, update calibration and key chords, compute or read the
  processed values, and emit `input` events capped at ~60 Hz by `Coalescer`.
- [app/src-tauri/src/keys.rs](app/src-tauri/src/keys.rs): key bindings. `KEYS` maps W3C
  `KeyboardEvent.code` names to scan codes; `Chords::update` turns button edges into key
  down/up events (shared keys stay down until the last chord releases); `send` injects them
  with `SendInput`.
- [app/src-tauri/src/calibration.rs](app/src-tauri/src/calibration.rs): `Calibrator`
  tracks per-axis min/max between `start` and `finish`; center is the value at finish.
- [app/src-tauri/src/watcher.rs](app/src-tauri/src/watcher.rs): Win32 glue feeding the
  switcher. A `SetWinEventHook` foreground hook (debounced 250 ms) and, in "running" mode, a
  2-second process scan. `list_windows` backs the "Add running app" picker.
- [app/src-tauri/src/tray.rs](app/src-tauri/src/tray.rs): tray icon and menu (Open, active
  profile, Pause switching, Quit); `update` rebuilds only when name or paused state changes.
- Icons referenced by `tauri.conf.json`: [32x32.png](app/src-tauri/icons/32x32.png),
  [128x128.png](app/src-tauri/icons/128x128.png),
  [128x128@2x.png](app/src-tauri/icons/128x128@2x.png),
  [icon.ico](app/src-tauri/icons/icon.ico).
- [app/src-tauri/resources/driver/.gitkeep](app/src-tauri/resources/driver/.gitkeep):
  placeholder. CI stages `openx3d.inf/.dll/.cat` here before building the installer.

## 5. App frontend: `app/` and `app/src/`

React 19 + TypeScript + Vite + zustand. Talks to the backend only through Tauri commands
and events; in a plain browser it runs against an in-memory mock.

### 5.1 Tooling

- [app/package.json](app/package.json): scripts (`dev`, `build`, `lint`, `typecheck`,
  `test`, `tauri`) and dependencies.
- [app/package-lock.json](app/package-lock.json): pinned npm dependencies. Generated.
- [app/index.html](app/index.html): Vite entry page; mounts `src/main.tsx` into `#root`.
- [app/vite.config.ts](app/vite.config.ts): React plugin, fixed port 5173 (matches
  `devUrl` in `tauri.conf.json`), ignores `src-tauri` in the watcher.
- [app/tsconfig.json](app/tsconfig.json): strict TypeScript, `noUncheckedIndexedAccess`,
  no emit (Vite bundles).
- [app/eslint.config.js](app/eslint.config.js): typescript-eslint strict type-checked plus
  react-hooks rules.
- [app/src/README.md](app/src/README.md): frontend quick start and layout summary.

### 5.2 Data and plumbing

- [app/src/types.ts](app/src/types.ts): TypeScript mirror of the Rust config and IPC
  types, plus `defaultDeviceConfig()` (must match `DeviceConfig::default()` in core).
- [app/src/lib/ipc.ts](app/src/lib/ipc.ts): the only door to the backend. `Commands` and
  `Events` type every command and event; `Transport` abstracts real Tauri IPC vs. the mock
  (chosen once at startup); `api` is the typed wrapper the rest of the UI calls.
- [app/src/lib/mock.ts](app/src/lib/mock.ts): `createMockTransport()`, a fake backend with
  two profiles, synthetic 60 Hz input and calibration tracking. It applies per-axis
  curves only; axis targets, button maps and hat modes are not mocked. Used by
  `npm run dev` in a browser and by tests; tree-shaken out of production builds.
- [app/src/lib/pipeline.ts](app/src/lib/pipeline.ts): TypeScript port of the per-axis
  math. `curveAt`, `buildLut`, `lutAt` draw the curve; `axisInput` gives the live marker
  position; `processAxis` feeds the mock; `clamp` is shared by the components. The
  backend/driver stay authoritative.
- [app/src/store.ts](app/src/store.ts): the zustand store and all UI logic. State: backend
  `appState`, `profiles`, the `draft` being edited, `dirty`, live `frame` and report rate.
  Key actions: `init` (subscribe + initial load), `editConfig` (mutate draft, debounced
  `preview_config` after 50 ms), `save`, `discard`, `select`, profile CRUD, and
  `saveSettings` (serialized through a promise queue). `guard()` routes errors to the banner.
- [app/src/main.tsx](app/src/main.tsx): starts `init()` once outside React, then renders
  `<App />`.
- [app/src/App.tsx](app/src/App.tsx): shell: header with `StatusBar` and the
  profile-being-edited selector, tab navigation over the five pages, error banner, and the
  Save/Discard bar when the draft is dirty.

### 5.3 Components

- [app/src/components/controls.tsx](app/src/components/controls.tsx): form primitives:
  `Slider` (optional log scale), `Toggle`, generic `Select` (values may be null/number),
  `Radio`, and the `cx` class-name helper.
- [app/src/components/live.tsx](app/src/components/live.tsx): read-only live widgets:
  `AxisBar`, `ButtonGrid`, `HatRose`.
- [app/src/components/CurveEditor.tsx](app/src/components/CurveEditor.tsx): SVG response
  curve over -1..1 deflection and output. Draws the driver's exact response (deadzone bands,
  17-point LUT, mirrored), a dashed linear reference at the current sensitivity, a live
  marker, and hover readouts (point position, segment slope). Point curves add
  click-to-add, drag, and double-click-to-delete, with x snapped to the 1/16 LUT grid.

### 5.4 Pages

- [app/src/pages/TestPage.tsx](app/src/pages/TestPage.tsx): raw vs. processed axes, hat,
  buttons, and report rate. The first place to look when checking hardware.
- [app/src/pages/AxesPage.tsx](app/src/pages/AxesPage.tsx): per-axis editor: calibration
  fields and wizard (`start_calibration` / `finish_calibration`), output axis, invert,
  deadzones, sensitivity, and the custom-curve switch with the `CurveEditor`.
- [app/src/pages/ButtonsPage.tsx](app/src/pages/ButtonsPage.tsx): button map with shift
  layer, hat mode, and key bindings with live chord capture (Esc cancels).
- [app/src/pages/ProfilesPage.tsx](app/src/pages/ProfilesPage.tsx): profile list,
  new/duplicate/import/export/delete, default and manual activation, and executable paths
  including the "Add running app" picker (`RunningAppsModal`, a native `<dialog>`). Export
  serializes the saved profile in the browser; import goes through the backend.
- [app/src/pages/SettingsPage.tsx](app/src/pages/SettingsPage.tsx): autostart, pause
  switching, match mode, plus driver/joystick status and the code-signing note.

### 5.5 Styles

- [app/src/styles/global.css](app/src/styles/global.css): dark theme variables and base
  element styles.
- [app/src/styles/ui.module.css](app/src/styles/ui.module.css): CSS module with every
  component class (`card`, `row`, `badge`, `curve*`, `modal`…), imported as `styles`.

### 5.6 Tests

- [app/src/types.test.ts](app/src/types.test.ts): `defaultDeviceConfig()` equals the
  identity config.
- [app/src/lib/pipeline.test.ts](app/src/lib/pipeline.test.ts): axis math and LUTs in
  the TS port.
- [app/src/store.test.ts](app/src/store.test.ts): store behavior against the mock: dirty
  tracking, save during edits, discard, preview debounce, settings queue, rejected previews.

## 6. Installer

- [app/src-tauri/windows/hooks.nsh](app/src-tauri/windows/hooks.nsh): NSIS hooks compiled
  into Tauri's installer. Adds the "Driver certificate" consent page; on install runs
  `sign-catalog.ps1` then `pnputil /add-driver`; on uninstall finds the driver packages
  with `Get-WindowsDriver`, deletes them and removes the certificate. Driver files are
  extracted to `$PLUGINSDIR`, never the install folder, because they run elevated.

## 7. GitHub automation: `.github/`

- [.github/workflows/README.md](.github/workflows/README.md): explains every workflow, the
  release signing certificate, how to cut a release and how to re-run one.
- [.github/workflows/ci.yml](.github/workflows/ci.yml): on push/PR. Four jobs: Rust on
  Linux (fmt, clippy, tests), frontend (lint, typecheck, tests, build), the driver host
  test (`make -C driver/test`), and Windows (driver build, InfVerif, Rust, app build).
- [.github/workflows/build.yml](.github/workflows/build.yml): reusable workflow that builds
  the driver, packages it, builds the NSIS installer, smoke-tests local catalog signing,
  optionally signs the installer, writes checksums, and attests provenance in a separate job.
- [.github/workflows/nightly.yml](.github/workflows/nightly.yml): calls `build.yml` on every
  push to `main`, unsigned.
- [.github/workflows/release.yml](.github/workflows/release.yml): the release button.
  Started with **Run workflow** on `main` and a version, its `bump` job writes every
  version file, commits, tags and restarts the workflow on the tag. On a `v*` tag (from
  `bump`, or pushed by hand) it checks the tag is on `main` and that all version numbers
  match it, calls `build.yml` with signing, then publishes a GitHub release behind the
  `release` environment.
- [.github/dependabot.yml](.github/dependabot.yml): one grouped weekly PR for cargo, npm and
  actions, with a 7-day cooldown and no major bumps.
- [.github/CODEOWNERS](.github/CODEOWNERS): single owner for everything.
- [.github/pull_request_template.md](.github/pull_request_template.md): PR checklist
  (tests, regenerated vectors, CI, real-hardware test for driver changes).

---

## Following one change end to end

"User drags a deadzone slider":

1. [AxesPage.tsx](app/src/pages/AxesPage.tsx) calls `editConfig` in
   [store.ts](app/src/store.ts), which updates the draft and schedules `preview_config`.
2. [ipc.ts](app/src/lib/ipc.ts) invokes the command;
   [commands.rs](app/src-tauri/src/commands.rs) validates and calls
   `Switcher::set_preview` in [switcher.rs](app/src-tauri/src/switcher.rs).
3. `evaluate()` recompiles via [blob.rs](core/src/blob.rs); the device thread in
   [device.rs](app/src-tauri/src/device.rs) sees `take_push()` return a new blob and sends
   feature report 3.
4. The driver's `SetFeature` in [hid.c](driver/openx3d/hid.c) validates with
   [pipeline.c](driver/pipeline/pipeline.c), persists via
   [registry.c](driver/openx3d/registry.c), and applies it in
   [device.c](driver/openx3d/device.c). The next USB report uses the new deadzone.

## Things that must change together

| If you change… | also update |
|---|---|
| Pipeline math | `core/src/pipeline.rs`, `driver/pipeline/pipeline.c`, `app/src/lib/pipeline.ts` (axis math only), then regenerate vectors |
| Blob layout | `core/src/blob.rs`, `driver/pipeline/pipeline.h` |
| HID descriptor / reports | `driver/openx3d/descriptor.h`, the lengths and codecs in `core/src/hid.rs` |
| Config or IPC types | `core/src/config.rs` (or `state.rs`), `app/src/types.ts`, `app/src/lib/mock.ts` |
| A Tauri command | `commands.rs`, handler list in `lib.rs`, `ipc.ts`, `mock.ts` |
| Version | nothing by hand: the `bump` job in `release.yml` writes `Cargo.toml`, `app/package.json`, `openx3d.inf`, `openx3d.rc` and `device.h` (`X3D_DRIVER_VERSION_BCD`) |

## Commands

```sh
cargo test --workspace                      # Rust logic (any OS)
cargo run -p x3d-core --bin gen-vectors     # regenerate test vectors
make -C driver/test                         # C pipeline vs. vectors
cd app && npm ci && npm run dev             # UI in a browser with the mock backend
cd app && npm run lint && npm test          # frontend checks
```
