# Open X3D Pro: Tauri backend

Rust side of the tray agent (`open-x3d-pro.exe`). It manages profiles, follows the foreground or running app, pushes config blobs to the driver, and streams live input to the UI. Interfaces are defined in [`docs/CONTRACT.md`](../../docs/CONTRACT.md) §5 and §6.

## Running

On Windows, from `app/`:

```sh
npm ci
cargo tauri dev          # starts `npm run dev` (Vite on :5173) and the backend
```

Pass `--minimized` to start in the tray without opening the window; autostart uses this flag.

On Linux and macOS only the platform-neutral modules (profiles, settings, switcher, keys, calibration, input coalescing) build and are tested with `cargo test --workspace`. Tauri, hidapi and the Win32 hooks are Windows-only dependencies, so no webkit2gtk or GTK packages are needed. For UI work, run `npm run dev` in `app/`, which uses the frontend's mock transport.

## Logging

Set `RUST_LOG` (default `info`), for example `RUST_LOG=open_x3d_pro_lib=debug`. Release builds have no console, so logs only show when the app is started from a terminal in a debug build.

## Data

- Profiles: `%APPDATA%\OpenX3DPro\profiles\<id>.json`, one file per profile. The file name is the id, and exactly one profile is the default. A `Default` identity profile is created on first run. Unreadable or invalid files are skipped with a warning and left on disk.
- Settings: `%APPDATA%\OpenX3DPro\settings.json`. On first run in a release build, autostart is registered under `HKCU\...\Run`.

## Without the driver (fallback mode)

The backend looks for the driver's vendor collection (`HID\VID_046D&PID_C215&Col02`, usage page `0xFF00`).

- If the collection is missing, the backend opens the stock joystick collection instead and reads its 7-byte reports.
- It computes the `processed` values itself with `x3d_core::pipeline`, using the active or previewed config.
- `get_state` reports `driverInstalled: false`.
- `preview_config` and profile switches then change only what the app shows. Games keep seeing the raw stock device.
- Key bindings and calibration work the same in both modes.
- The device is re-detected every second, so installing the driver or replugging the stick needs no restart.
