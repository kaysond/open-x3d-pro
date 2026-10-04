# Open X3D Pro — frontend

React 18 + TypeScript (strict) + Vite 5 + zustand, plain CSS modules. Talks to the Rust backend only through the commands and events in [`docs/CONTRACT.md`](../../docs/CONTRACT.md) §5.

## Run

All commands run from `app/`.

```sh
npm ci
npm run dev          # http://localhost:5173 in any browser, mock backend
npm run tauri dev    # real app: Tauri runs `npm run dev` and opens the window
```

Without `window.__TAURI_INTERNALS__` (a plain browser, or vitest), `src/lib/ipc.ts` uses an in-memory mock instead of Tauri IPC. The mock has a Default and a "DCS World" profile, synthetic 60 Hz input (sine on X/Y, slider sweep, rotating hat, random buttons), calibration tracking and a fake window list. It reports `driverInstalled: false`, like the backend's no-driver fallback. The transport is picked once at startup; nothing else knows which one is active.

## Scripts

| Script | Does |
|---|---|
| `dev` | Vite dev server on port 5173 (strict) |
| `build` | `tsc --noEmit` then `vite build` to `dist/` (Tauri's `frontendDist`) |
| `preview` | serve `dist/` |
| `lint` | ESLint (typescript-eslint strict type-checked + react-hooks) |
| `typecheck` | `tsc --noEmit` |
| `test` | vitest: pipeline math, default config, store dirty/save/preview |
| `tauri` | Tauri CLI, e.g. `npm run tauri build` |

## Layout

```
src/
├── main.tsx, App.tsx      entry, header/status, tabs, save bar
├── types.ts               CONTRACT §3/§5 types, defaultDeviceConfig(), uuid()
├── store.ts               zustand: app state, profiles, draft + dirty, live frame, debounced preview_config
├── lib/ipc.ts             typed command/event wrappers, transport selection
├── lib/mock.ts            mock transport
├── lib/pipeline.ts        CONTRACT §4.1 axis math (curve preview, live marker, mock output)
├── components/            controls (Slider, Toggle, Select, Radio), live widgets (AxisBar, ButtonGrid, HatRose), CurveEditor
├── pages/                 Test, Axes, Buttons, Profiles, Settings
└── styles/                global.css (theme), ui.module.css
```

Edits on the Axes/Buttons/Profiles pages change a working copy of the selected profile. Config edits are pushed with `preview_config` after 50 ms, Save persists with `save_profile`, and Discard ends the preview with `activate_profile`.
