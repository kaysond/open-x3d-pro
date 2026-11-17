import { describe, expect, it } from 'vitest';
import type { AxisConfig } from '../types';
import { defaultDeviceConfig } from '../types';
import { buildLut, curveAt, deflection, processAxis, response } from './pipeline';

const axis = (patch: Partial<AxisConfig> = {}): AxisConfig => ({ ...defaultDeviceConfig().axes.x, ...patch });

describe('pipeline', () => {
  it('identity LUT is round(i * 32767 / 16)', () => {
    expect(buildLut({ type: 'linear' }, 1)).toEqual(Array.from({ length: 17 }, (_, i) => Math.round((i * 32767) / 16)));
  });

  it('identity: center is 0, ends are ±32767', () => {
    expect(processAxis(axis(), 508 << 6)).toBe(-256);
    expect(processAxis(axis(), 32768)).toBe(0);
    expect(processAxis(axis(), 1023 << 6)).toBe(32767);
    expect(processAxis(axis(), 0)).toBe(-32767);
  });

  it('inverts', () => {
    expect(processAxis(axis({ invert: true }), 508 << 6)).toBe(256);
  });

  it('rescales after deadzones, per side', () => {
    const dz = axis({ deadzone: { center: 0.1, min: 0, max: 0.2 } });
    const lut = buildLut(dz.curve, 1);
    expect(response(dz, lut, 0.05)).toBe(0);
    expect(response(dz, lut, 0.5) / 32767).toBeCloseTo((0.5 - 0.1) / (1 - 0.1 - 0.2), 3);
    expect(response(dz, lut, 0.95)).toBe(32767);
    // Negative side uses the min-end deadzone (0 here).
    expect(response(dz, lut, -0.95) / 32767).toBeCloseTo(-(0.95 - 0.1) / 0.9, 3);
    expect(deflection(dz, 32768 + (511 << 5))).toBeCloseTo(0.5, 3);
  });

  it('interpolates points and mirrors them', () => {
    const curve = { type: 'points' as const, points: [[0, 0], [0.5, 0.25], [1, 1]] as [number, number][] };
    expect(curveAt(curve, 0.25)).toBeCloseTo(0.125);
    expect(curveAt(curve, 0.75)).toBeCloseTo(0.625);
    const a = axis({ curve });
    expect(processAxis(a, 32768 + (511 << 5))).toBe(8192);
    expect(processAxis(a, 32768 - 16384)).toBe(-8192);
  });

  it('clamps sensitivity gain at full scale', () => {
    const lut = buildLut({ type: 'linear' }, 2);
    expect(lut[4]).toBe(16384);
    expect(lut.slice(8).every((v) => v === 32767)).toBe(true);
  });
});
