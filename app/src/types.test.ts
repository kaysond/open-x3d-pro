import { expect, it } from 'vitest';
import { defaultDeviceConfig } from './types';

it('default config is the identity from CONTRACT §3', () => {
  const c = defaultDeviceConfig();
  for (const id of ['x', 'y', 'rz', 'slider'] as const) {
    expect(c.axes[id]).toEqual({
      calibration: { min: 0, center: 32768, max: id === 'x' || id === 'y' ? 1023 << 6 : 255 << 8 },
      invert: false,
      deadzone: { center: 0, min: 0, max: 0 },
      curve: { type: 'linear' },
      sensitivity: 1,
      target: id,
    });
  }
  expect(c.hatMode).toBe('hat');
  expect(c.buttons.map((b) => b.output)).toEqual([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
  expect(c.shiftButton).toBeNull();
  expect(c.buttonsShifted).toEqual(Array.from({ length: 12 }, () => ({ output: null })));
  expect(c.keyBindings).toEqual([]);
});
