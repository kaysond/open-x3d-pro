import { create } from 'zustand';
import { api, errorMessage } from './lib/ipc';
import type { AppState, DeviceConfig, InputFrame, Profile, ProfileSwitched, Settings } from './types';
import { defaultDeviceConfig, uuid } from './types';

interface Store {
  appState: AppState | null;
  profiles: Profile[];
  /** Working copy of the selected profile; Axes/Buttons/Profiles pages edit this. */
  draft: Profile | null;
  dirty: boolean;
  frame: InputFrame | null;
  hz: number;
  lastSwitch: ProfileSwitched | null;
  error: string | null;

  init: () => Promise<() => void>;
  select: (id: string) => void;
  editConfig: (mutate: (config: DeviceConfig) => void) => void;
  editProfile: (patch: Partial<Pick<Profile, 'name' | 'exePaths'>>) => void;
  save: () => Promise<void>;
  discard: () => void;
  createProfile: (from?: Profile) => Promise<void>;
  deleteProfile: (id: string) => Promise<void>;
  setDefault: (id: string) => Promise<void>;
  activate: (id: string | null) => Promise<void>;
  importProfile: (json: string) => Promise<void>;
  saveSettings: (patch: Partial<Settings>) => Promise<void>;
}

/** Runs an IPC action, surfacing failures in the error banner instead of throwing. */
export async function guard(fn: () => Promise<unknown>): Promise<void> {
  try {
    await fn();
  } catch (e) {
    useStore.setState({ error: errorMessage(e) });
  }
}

let previewTimer: ReturnType<typeof setTimeout> | undefined;
let previewing = false;

function schedulePreview(config: DeviceConfig) {
  clearTimeout(previewTimer);
  previewTimer = setTimeout(() => {
    previewing = true;
    void guard(() => api.previewConfig(config));
  }, 50);
}

/** Replaces the draft; ends a live preview so the driver goes back to the real active profile. */
function load(profile: Profile | undefined) {
  clearTimeout(previewTimer);
  if (previewing) {
    previewing = false;
    const ls = useStore.getState().lastSwitch;
    void guard(() => api.activateProfile(ls?.reason === 'manual' ? ls.profileId : null));
  }
  useStore.setState({ draft: profile ? structuredClone(profile) : null, dirty: false });
}

let rate = { seq: 0, t: 0 };
function onFrame(frame: InputFrame) {
  const t = performance.now();
  const dt = t - rate.t;
  if (dt < 500) {
    useStore.setState({ frame });
    return;
  }
  // seq counts device reports (u16, wraps), so this is the device rate, not the ≤60 Hz event rate.
  const dseq = (((frame.seq - rate.seq) % 65536) + 65536) % 65536;
  const hz = rate.t === 0 ? 0 : Math.round((dseq * 1000) / dt);
  rate = { seq: frame.seq, t };
  useStore.setState({ frame, hz });
}

export const useStore = create<Store>()((set, get) => ({
  appState: null,
  profiles: [],
  draft: null,
  dirty: false,
  frame: null,
  hz: 0,
  lastSwitch: null,
  error: null,

  init: async () => {
    const offs: (() => void)[] = [];
    await guard(async () => {
      offs.push(await api.onInput(onFrame));
      offs.push(
        await api.onState((appState) => {
          set(appState.deviceConnected ? { appState } : { appState, hz: 0 });
        }),
      );
      offs.push(
        await api.onProfileSwitched((lastSwitch) => {
          set((s) => ({
            lastSwitch,
            appState: s.appState && { ...s.appState, activeProfileId: lastSwitch.profileId },
          }));
        }),
      );
      const [appState, profiles] = await Promise.all([api.getState(), api.listProfiles()]);
      set({ appState, profiles });
      load(profiles.find((p) => p.id === appState.activeProfileId) ?? profiles.find((p) => p.isDefault) ?? profiles[0]);
    });
    return () => {
      for (const off of offs) off();
    };
  },

  select: (id) => {
    if (get().dirty && !window.confirm('Discard unsaved changes?')) return;
    load(get().profiles.find((p) => p.id === id));
  },

  editConfig: (mutate) => {
    const { draft } = get();
    if (!draft) return;
    const config = structuredClone(draft.config);
    mutate(config);
    set({ draft: { ...draft, config }, dirty: true });
    schedulePreview(config);
  },

  editProfile: (patch) => {
    const { draft } = get();
    if (draft) set({ draft: { ...draft, ...patch }, dirty: true });
  },

  save: async () => {
    const { draft } = get();
    if (!draft) return;
    clearTimeout(previewTimer);
    await guard(async () => {
      const stored = get().profiles.find((p) => p.id === draft.id);
      const saved = await api.saveProfile({ ...draft, isDefault: stored?.isDefault ?? false });
      // Saving ends the preview on the backend side.
      previewing = false;
      set((s) => ({ profiles: s.profiles.map((p) => (p.id === saved.id ? saved : p)) }));
      load(saved);
    });
  },

  discard: () => {
    const { draft, profiles } = get();
    load(profiles.find((p) => p.id === draft?.id));
  },

  createProfile: async (from) => {
    await guard(async () => {
      const profile: Profile = from
        ? // Exe paths are not copied: two profiles matching one exe would be ambiguous.
          { ...structuredClone(from), id: uuid(), name: `${from.name} (copy)`, exePaths: [], isDefault: false }
        : { id: uuid(), name: 'New profile', exePaths: [], isDefault: false, config: defaultDeviceConfig() };
      const saved = await api.saveProfile(profile);
      set((s) => ({ profiles: [...s.profiles, saved] }));
      get().select(saved.id);
    });
  },

  deleteProfile: async (id) => {
    await guard(async () => {
      await api.deleteProfile(id);
      const profiles = get().profiles.filter((p) => p.id !== id);
      set({ profiles });
      if (get().draft?.id === id) load(profiles.find((p) => p.isDefault) ?? profiles[0]);
    });
  },

  setDefault: async (id) => {
    await guard(async () => {
      await api.setDefaultProfile(id);
      set((s) => ({
        profiles: s.profiles.map((p) => ({ ...p, isDefault: p.id === id })),
        draft: s.draft && { ...s.draft, isDefault: s.draft.id === id },
      }));
    });
  },

  activate: async (id) => {
    await guard(async () => {
      await api.activateProfile(id);
      previewing = false;
    });
  },

  importProfile: async (json) => {
    await guard(async () => {
      const profile = await api.importProfile(json);
      // Reload rather than append: the backend may have replaced a profile with the same id.
      set({ profiles: await api.listProfiles() });
      get().select(profile.id);
    });
  },

  saveSettings: async (patch) => {
    const { appState } = get();
    if (!appState) return;
    await guard(async () => {
      const settings = await api.saveSettings({ ...appState.settings, ...patch });
      set((s) => ({ appState: s.appState && { ...s.appState, settings } }));
    });
  },
}));
