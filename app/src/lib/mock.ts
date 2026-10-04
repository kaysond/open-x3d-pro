// In-memory stand-in for the Tauri backend so the UI runs in a plain browser and in tests.
import type { AppState, AxisId, DeviceConfig, InputFrame, Profile, Settings } from '../types';
import { AXES, defaultDeviceConfig, uuid } from '../types';
import type { Commands, Events, Transport } from './ipc';
import { processFrame, toRaw16 } from './pipeline';

const DCS_EXE = 'C:\\Program Files\\Eagle Dynamics\\DCS World\\bin\\DCS.exe';

function dcsConfig(): DeviceConfig {
  const c = defaultDeviceConfig();
  for (const id of ['x', 'y'] as const) {
    c.axes[id].curve = { type: 'exponent', exponent: 1.8 };
    c.axes[id].deadzone.center = 0.04;
  }
  c.axes.rz.curve = { type: 'points', points: [[0, 0], [0.5, 0.25], [1, 1]] };
  c.hatMode = 'both';
  c.shiftButton = 12;
  c.buttonsShifted[0] = { output: 17 };
  c.keyBindings = [{ button: 7, keys: ['ControlLeft', 'KeyF'] }];
  return c;
}

export function createMockTransport(): Transport {
  let profiles: Profile[] = [
    { id: uuid(), name: 'Default', exePaths: [], isDefault: true, config: defaultDeviceConfig() },
    { id: uuid(), name: 'DCS World', exePaths: [DCS_EXE], isDefault: false, config: dcsConfig() },
  ];
  let settings: Settings = { autostart: true, switchingPaused: false, matchMode: 'foreground' };
  let manual: string | null = null;
  let preview: DeviceConfig | null = null;
  let raw: InputFrame['raw'] = { x: 508, y: 512, rz: 128, slider: 0, hat: 8, buttons: 0 };
  const calibrating: Partial<Record<AxisId, { min: number; max: number }>> = {};

  const find = (id: string) => {
    const p = profiles.find((x) => x.id === id);
    if (!p) throw new Error(`Unknown profile ${id}`);
    return p;
  };
  const active = () => profiles.find((p) => p.id === manual) ?? profiles.find((p) => p.isDefault);
  const state = (): AppState => ({
    deviceConnected: true,
    driverInstalled: false,
    driverVersion: null,
    activeProfileId: active()?.id ?? null,
    matchedExe: null,
    settings,
  });

  const listeners: { [K in keyof Events]: Set<(payload: Events[K]) => void> } = {
    input: new Set(),
    state: new Set(),
    profile_switched: new Set(),
  };
  const emit = <K extends keyof Events>(event: K, payload: Events[K]) => {
    for (const cb of listeners[event]) cb(structuredClone(payload));
  };

  const t0 = performance.now();
  let buttons = 0;
  let nextButtons = 0;
  const tick = () => {
    const t = (performance.now() - t0) / 1000;
    if (t >= nextButtons) {
      buttons = Math.random() < 0.4 ? 0 : 1 << Math.floor(Math.random() * 12);
      nextButtons = t + 0.3 + Math.random() * 0.7;
    }
    raw = {
      x: Math.round(508 + 500 * Math.sin(t * 0.7)),
      y: Math.round(508 + 480 * Math.sin(t * 0.45 + 1)),
      rz: Math.round(128 + 120 * Math.sin(t * 0.3)),
      slider: Math.round(255 * Math.abs(((t / 4) % 2) - 1)),
      hat: Math.floor(t * 2) % 9,
      buttons,
    };
    for (const id of AXES) {
      const c = calibrating[id];
      if (c) {
        const v = toRaw16(id, raw[id]);
        c.min = Math.min(c.min, v);
        c.max = Math.max(c.max, v);
      }
    }
    const config = preview ?? active()?.config ?? defaultDeviceConfig();
    // seq mimics the device's 100 Hz u16 report counter.
    emit('input', { raw, processed: processFrame(config, raw), seq: Math.floor(t * 100) & 0xffff });
  };
  let timer: ReturnType<typeof setInterval> | undefined;

  const handlers: { [K in keyof Commands]: (args: Commands[K][0]) => Commands[K][1] } = {
    get_state: state,
    list_profiles: () => profiles,
    save_profile: ({ profile }) => {
      const old = profiles.find((p) => p.id === profile.id);
      const saved = { ...profile, isDefault: old?.isDefault ?? false };
      profiles = old ? profiles.map((p) => (p === old ? saved : p)) : [...profiles, saved];
      preview = null;
      emit('state', state());
      return saved;
    },
    delete_profile: ({ id }) => {
      if (find(id).isDefault) throw new Error('Cannot delete the default profile');
      profiles = profiles.filter((p) => p.id !== id);
      if (manual === id) manual = null;
      emit('state', state());
      return null;
    },
    set_default_profile: ({ id }) => {
      find(id);
      profiles = profiles.map((p) => ({ ...p, isDefault: p.id === id }));
      emit('state', state());
      return null;
    },
    activate_profile: ({ id }) => {
      if (id !== null) find(id);
      manual = id;
      preview = null;
      const a = active();
      if (a) emit('profile_switched', { profileId: a.id, exePath: null, reason: id === null ? 'default' : 'manual' });
      emit('state', state());
      return null;
    },
    preview_config: ({ config }) => {
      preview = config;
      return null;
    },
    list_windows: () => [
      { pid: 4242, exePath: DCS_EXE, title: 'Digital Combat Simulator' },
      {
        pid: 5150,
        exePath: 'C:\\Program Files (x86)\\Steam\\steamapps\\common\\IL-2 Sturmovik Battle of Stalingrad\\bin\\game\\Il-2.exe',
        title: 'IL-2 Sturmovik',
      },
      { pid: 812, exePath: 'C:\\Windows\\explorer.exe', title: 'File Explorer' },
    ],
    get_settings: () => settings,
    save_settings: ({ settings: s }) => {
      settings = s;
      emit('state', state());
      return settings;
    },
    start_calibration: ({ axis }) => {
      const v = toRaw16(axis, raw[axis]);
      calibrating[axis] = { min: v, max: v };
      return null;
    },
    finish_calibration: ({ axis }) => {
      const c = calibrating[axis];
      if (!c) throw new Error(`Calibration of ${axis} was not started`);
      calibrating[axis] = undefined;
      if (c.min >= c.max) throw new Error('the axis did not move; sweep it through its full range');
      return { min: c.min, center: toRaw16(axis, raw[axis]), max: c.max };
    },
    export_profile: ({ id }) => JSON.stringify(find(id), null, 2),
    import_profile: ({ json }) => {
      const p = JSON.parse(json) as Partial<Profile>;
      if (typeof p.name !== 'string' || typeof p.config !== 'object') throw new Error('Not a profile JSON file');
      const created: Profile = { exePaths: [], ...p, name: p.name, config: p.config, id: uuid(), isDefault: false };
      profiles = [...profiles, created];
      return created;
    },
  };

  return {
    // Cloning both ways mimics the JSON boundary so UI and mock never share objects.
    invoke: <K extends keyof Commands>(cmd: K, args: Commands[K][0]) => {
      try {
        return Promise.resolve(structuredClone(handlers[cmd](structuredClone(args))));
      } catch (e) {
        return Promise.reject(e instanceof Error ? e : new Error(String(e)));
      }
    },
    listen: <K extends keyof Events>(event: K, cb: (payload: Events[K]) => void) => {
      listeners[event].add(cb);
      if (event === 'input' && !timer) timer = setInterval(tick, 1000 / 60);
      return Promise.resolve(() => {
        listeners[event].delete(cb);
        if (listeners.input.size === 0 && timer) {
          clearInterval(timer);
          timer = undefined;
        }
      });
    },
  };
}
