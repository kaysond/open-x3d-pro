// Typed wrappers for CONTRACT §5. The transport is picked once at startup: real Tauri IPC
// inside the app, an in-memory mock in a plain browser (npm run dev) and in tests.
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type {
  AppState,
  AxisId,
  Calibration,
  DeviceConfig,
  InputFrame,
  Profile,
  ProfileSwitched,
  Settings,
  WindowInfo,
} from '../types';
import { createMockTransport } from './mock';

type NoArgs = Record<string, never>;

/** command name -> [args, resolved value]; Rust `()` arrives as null. */
export interface Commands {
  get_state: [NoArgs, AppState];
  list_profiles: [NoArgs, Profile[]];
  save_profile: [{ profile: Profile }, Profile];
  delete_profile: [{ id: string }, null];
  set_default_profile: [{ id: string }, null];
  activate_profile: [{ id: string | null }, null];
  preview_config: [{ config: DeviceConfig }, null];
  list_windows: [NoArgs, WindowInfo[]];
  get_settings: [NoArgs, Settings];
  save_settings: [{ settings: Settings }, Settings];
  start_calibration: [{ axis: AxisId }, null];
  finish_calibration: [{ axis: AxisId }, Calibration];
  export_profile: [{ id: string }, string];
  import_profile: [{ json: string }, Profile];
}

export interface Events {
  input: InputFrame;
  state: AppState;
  profile_switched: ProfileSwitched;
}

export interface Transport {
  invoke<K extends keyof Commands>(cmd: K, args: Commands[K][0]): Promise<Commands[K][1]>;
  listen<K extends keyof Events>(event: K, cb: (payload: Events[K]) => void): Promise<() => void>;
}

const tauriTransport: Transport = {
  invoke: <K extends keyof Commands>(cmd: K, args: Commands[K][0]) => invoke<Commands[K][1]>(cmd, args),
  listen: <K extends keyof Events>(event: K, cb: (payload: Events[K]) => void) =>
    listen<Events[K]>(event, (e) => {
      cb(e.payload);
    }),
};

const transport: Transport =
  typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window ? tauriTransport : createMockTransport();

export const api = {
  getState: () => transport.invoke('get_state', {}),
  listProfiles: () => transport.invoke('list_profiles', {}),
  saveProfile: (profile: Profile) => transport.invoke('save_profile', { profile }),
  deleteProfile: (id: string) => transport.invoke('delete_profile', { id }),
  setDefaultProfile: (id: string) => transport.invoke('set_default_profile', { id }),
  activateProfile: (id: string | null) => transport.invoke('activate_profile', { id }),
  previewConfig: (config: DeviceConfig) => transport.invoke('preview_config', { config }),
  listWindows: () => transport.invoke('list_windows', {}),
  getSettings: () => transport.invoke('get_settings', {}),
  saveSettings: (settings: Settings) => transport.invoke('save_settings', { settings }),
  startCalibration: (axis: AxisId) => transport.invoke('start_calibration', { axis }),
  finishCalibration: (axis: AxisId) => transport.invoke('finish_calibration', { axis }),
  exportProfile: (id: string) => transport.invoke('export_profile', { id }),
  importProfile: (json: string) => transport.invoke('import_profile', { json }),
  onInput: (cb: (frame: InputFrame) => void) => transport.listen('input', cb),
  onState: (cb: (state: AppState) => void) => transport.listen('state', cb),
  onProfileSwitched: (cb: (e: ProfileSwitched) => void) => transport.listen('profile_switched', cb),
};

/** Tauri rejects with the command's error string; the mock rejects with an Error. */
export const errorMessage = (e: unknown): string => (e instanceof Error ? e.message : String(e));
