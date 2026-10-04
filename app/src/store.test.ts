import { afterAll, expect, it, vi } from 'vitest';
import { api } from './lib/ipc';
import { useStore } from './store';

const off = await useStore.getState().init();
afterAll(off);

const savedInvert = async (id: string) =>
  (await api.listProfiles()).find((p) => p.id === id)?.config.axes.x.invert;

it('loads the default profile from the mock transport', () => {
  const { draft, profiles, error } = useStore.getState();
  expect(error).toBeNull();
  expect(profiles.map((p) => p.name)).toEqual(['Default', 'DCS World']);
  expect(draft?.isDefault).toBe(true);
});

it('tracks dirty edits and saves them', async () => {
  const id = useStore.getState().draft?.id ?? '';
  useStore.getState().editConfig((c) => {
    c.axes.x.invert = true;
  });
  expect(useStore.getState().dirty).toBe(true);
  expect(await savedInvert(id)).toBe(false);

  await useStore.getState().save();
  expect(useStore.getState().dirty).toBe(false);
  expect(await savedInvert(id)).toBe(true);
});

it('discard restores the saved profile', () => {
  useStore.getState().editProfile({ name: 'Renamed' });
  expect(useStore.getState().dirty).toBe(true);
  useStore.getState().discard();
  expect(useStore.getState().draft?.name).toBe('Default');
  expect(useStore.getState().dirty).toBe(false);
});

it('debounces previews to the latest config', () => {
  vi.useFakeTimers();
  const preview = vi.spyOn(api, 'previewConfig');
  for (const v of [0.1, 0.2, 0.3]) {
    useStore.getState().editConfig((c) => {
      c.axes.y.deadzone.center = v;
    });
  }
  expect(preview).not.toHaveBeenCalled();
  vi.advanceTimersByTime(50);
  expect(preview).toHaveBeenCalledTimes(1);
  expect(preview.mock.calls[0]?.[0].axes.y.deadzone.center).toBe(0.3);
  vi.useRealTimers();
});
