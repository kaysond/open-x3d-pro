// TS port of CONTRACT §4.1 for the curve preview and the mock transport.
// The backend/driver output is authoritative; this only has to agree within ±1 LSB.
import type { AxisConfig, AxisId, Curve } from '../types';

export const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
// Degenerate calibration (b <= 0) saturates instead of producing NaN.
const div = (a: number, b: number) => (b > 0 ? a / b : a > 0 ? Infinity : a < 0 ? -Infinity : 0);

export const toRaw16 = (axis: AxisId, raw: number) => (axis === 'x' || axis === 'y' ? raw << 6 : raw << 8);

export function curveAt(curve: Curve, x: number): number {
  switch (curve.type) {
    case 'linear':
      return x;
    case 'exponent':
      return Math.pow(x, curve.exponent);
    case 'points': {
      const pts = curve.points;
      let prev = pts[0];
      if (!prev) return x;
      for (const p of pts) {
        if (x <= p[0]) {
          return p[0] === prev[0] ? p[1] : prev[1] + ((x - prev[0]) / (p[0] - prev[0])) * (p[1] - prev[1]);
        }
        prev = p;
      }
      return prev[1];
    }
  }
}

/** 17 u16 entries, lut[i] = output for input i/16, sensitivity multiplied in. */
export function buildLut(curve: Curve, sensitivity: number): number[] {
  return Array.from({ length: 17 }, (_, i) =>
    clamp(Math.round(curveAt(curve, i / 16) * sensitivity * 65535), 0, 65535),
  );
}

export function lutAt(lut: readonly number[], m: number): number {
  const pos = clamp(m, 0, 1) * 16;
  const i = Math.min(Math.floor(pos), 15);
  const a = lut[i] ?? 0;
  const b = lut[i + 1] ?? a;
  return a + (pos - i) * (b - a);
}

/** Steps 2–4: signed magnitude in [-1,1] (symmetric) or [0,1] (full), i.e. the curve's input. */
export function axisInput(cfg: AxisConfig, raw16: number): number {
  const { min, center, max } = cfg.calibration;
  const dz = cfg.deadzone;
  if (cfg.curveMode === 'full') {
    let v = clamp(div(raw16 - min, max - min), 0, 1);
    if (cfg.invert) v = 1 - v;
    return clamp(div(v - dz.min, 1 - dz.min - dz.max), 0, 1);
  }
  let v = clamp(raw16 <= center ? -div(center - raw16, center - min) : div(raw16 - center, max - center), -1, 1);
  if (cfg.invert) v = -v;
  const m = Math.abs(v);
  const end = v < 0 ? dz.min : dz.max;
  const out = m <= dz.center ? 0 : clamp(div(m - dz.center, 1 - dz.center - end), 0, 1);
  return v < 0 ? -out : out;
}

export function processAxis(cfg: AxisConfig, raw16: number): number {
  const v = axisInput(cfg, raw16);
  const lutval = lutAt(buildLut(cfg.curve, cfg.sensitivity), Math.abs(v));
  if (cfg.curveMode === 'full') return Math.round(lutval);
  return clamp(Math.round(32768 + (Math.sign(v) * lutval) / 2), 0, 65535);
}
