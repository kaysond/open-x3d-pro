import { describe, expect, it } from 'vitest';
import type { AxisConfig } from '../types';
import { defaultDeviceConfig } from '../types';
import { axisInput, buildLut, curveAt, processAxis, processFrame } from './pipeline';

const axis = (patch: Partial<AxisConfig> = {}, id: 'x' | 'slider' = 'x'): AxisConfig => ({
  ...defaultDeviceConfig().axes[id],
  ...patch,
});

describe('pipeline', () => {
  it('identity LUT is round(i * 65535 / 16)', () => {
    expect(buildLut({ type: 'linear' }, 1)).toEqual(Array.from({ length: 17 }, (_, i) => Math.round((i * 65535) / 16)));
  });

  it('identity: X rest 508 -> 32512, centre and ends exact', () => {
    expect(processAxis(axis(), 508 << 6)).toBe(32512);
    expect(processAxis(axis(), 32768)).toBe(32768);
    expect(processAxis(axis(), 65535)).toBe(65535);
    expect(processAxis(axis(), 0)).toBe(1);
  });

  it('inverts symmetric and full axes', () => {
    expect(processAxis(axis({ invert: true }), 508 << 6)).toBe(33024);
    expect(processAxis(axis({ invert: true }, 'slider'), 0)).toBe(65535);
  });

  it('rescales after deadzones', () => {
    const dz = axis({ deadzone: { center: 0.1, min: 0, max: 0.2 } });
    expect(axisInput(dz, 32768 + 3000)).toBe(0);
    expect(axisInput(dz, 32768 + Math.round(0.5 * 32767))).toBeCloseTo((0.5 - 0.1) / (1 - 0.1 - 0.2), 3);
    expect(axisInput(dz, 32768 + Math.round(0.95 * 32767))).toBe(1);
    // Negative side uses the min-end deadzone (0 here).
    expect(axisInput(dz, 32768 - Math.round(0.95 * 32768))).toBeCloseTo(-(0.95 - 0.1) / 0.9, 3);
    const full = axis({ deadzone: { center: 0, min: 0.25, max: 0.25 } }, 'slider');
    expect(axisInput(full, 0x3000)).toBe(0);
    expect(axisInput(full, 32768)).toBeCloseTo(0.5, 3);
  });

  it('exponent 2 at 0.5 -> 0.25', () => {
    expect(curveAt({ type: 'exponent', exponent: 2 }, 0.5)).toBe(0.25);
    expect(buildLut({ type: 'exponent', exponent: 2 }, 1)[8]).toBe(16384);
  });

  it('interpolates points linearly', () => {
    const curve = { type: 'points' as const, points: [[0, 0], [0.5, 0.25], [1, 1]] as [number, number][] };
    expect(curveAt(curve, 0.25)).toBeCloseTo(0.125);
    expect(curveAt(curve, 0.75)).toBeCloseTo(0.625);
    expect(curveAt(curve, 1)).toBe(1);
  });

  it('clamps sensitivity gain at full scale', () => {
    const lut = buildLut({ type: 'linear' }, 2);
    expect(lut[4]).toBe(32768);
    expect(lut.slice(8).every((v) => v === 65535)).toBe(true);
  });

  it('maps hat to buttons and applies the shift layer', () => {
    const config = defaultDeviceConfig();
    config.hatMode = 'buttons';
    config.shiftButton = 12;
    config.buttonsShifted[0] = { output: 20 };
    const out = processFrame(config, { x: 512, y: 512, rz: 128, slider: 0, hat: 1, buttons: 0b1000_0000_0001 });
    expect(out.hat).toBe(8);
    expect(out.buttons).toBe((1 << 12) | (1 << 13) | (1 << 19));
  });
});
