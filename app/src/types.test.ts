import { expect, it } from 'vitest';
import { defaultDeviceConfig, uuid } from './types';

it('default config is the identity from CONTRACT §3', () => {
  const c = defaultDeviceConfig();
  for (const id of ['x', 'y', 'rz'] as const) {
    expect(c.axes[id]).toEqual({
      calibration: { min: 0, center: 32768, max: 65535 },
      invert: false,
      deadzone: { center: 0, min: 0, max: 0 },
      curve: { type: 'linear' },
      sensitivity: 1,
      curveMode: 'symmetric',
      target: id,
    });
  }
  expect(c.axes.slider.calibration).toEqual({ min: 0, center: 0, max: 65535 });
  expect(c.axes.slider.curveMode).toBe('full');
  expect(c.axes.slider.target).toBe('slider');
  expect(c.hatMode).toBe('hat');
  expect(c.buttons.map((b) => b.output)).toEqual([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
  expect(c.shiftButton).toBeNull();
  expect(c.buttonsShifted).toEqual(Array.from({ length: 12 }, () => ({ output: null })));
  expect(c.keyBindings).toEqual([]);
});

it('uuid is v4', () => {
  expect(uuid()).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
});
