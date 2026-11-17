// TS port of the reference pipeline (core/src/pipeline.rs) for the curve preview and the mock transport.
// The backend/driver output is authoritative; this only has to agree within ±1 LSB.
import type { AxisConfig, AxisId, Curve } from '../types';

/** Axis output range is ±OUT_MAX, center 0. */
export const OUT_MAX = 32767;
/** The LUT has STEPS + 1 entries, for inputs i / STEPS. */
export const STEPS = 16;

export const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
// Degenerate calibration (b <= 0) saturates instead of producing NaN.
const div = (a: number, b: number) => (b > 0 ? a / b : a > 0 ? Infinity : a < 0 ? -Infinity : 0);

/** Left shift from device units (10-bit X/Y, 8-bit Rz/slider) to the 16-bit calibration units. */
export const rawShift = (axis: AxisId) => (axis === 'x' || axis === 'y' ? 6 : 8);
export const toRaw16 = (axis: AxisId, raw: number) => raw << rawShift(axis);

export function curveAt(curve: Curve, x: number): number {
  if (curve.type === 'linear') return x;
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

/** STEPS + 1 entries in output units 0..OUT_MAX, lut[i] = output for input i / STEPS, sensitivity multiplied in. */
export function buildLut(curve: Curve, sensitivity: number): number[] {
  return Array.from({ length: STEPS + 1 }, (_, i) =>
    clamp(Math.round(curveAt(curve, i / STEPS) * sensitivity * OUT_MAX), 0, OUT_MAX),
  );
}

export function lutAt(lut: readonly number[], m: number): number {
  const pos = clamp(m, 0, 1) * STEPS;
  const i = Math.min(Math.floor(pos), STEPS - 1);
  const a = lut[i] ?? 0;
  const b = lut[i + 1] ?? a;
  return a + (pos - i) * (b - a);
}

/** Calibrated, inverted deflection in [-1, 1], before deadzones. */
export function deflection(cfg: AxisConfig, raw16: number): number {
  const { min, center, max } = cfg.calibration;
  const v = clamp(raw16 <= center ? -div(center - raw16, center - min) : div(raw16 - center, max - center), -1, 1);
  return cfg.invert ? -v : v;
}

/** Live range of the curve on one side: deflection between the center and end deadzones. */
export const liveRange = (cfg: AxisConfig, negative: boolean) =>
  1 - cfg.deadzone.center - (negative ? cfg.deadzone.min : cfg.deadzone.max);

/** Output in LUT units (±OUT_MAX) for deflection v: deadzones, then the curve, mirrored for v < 0. */
export function response(cfg: AxisConfig, lut: readonly number[], v: number): number {
  const m = Math.abs(v);
  if (m <= cfg.deadzone.center) return 0;
  return Math.sign(v) * lutAt(lut, div(m - cfg.deadzone.center, liveRange(cfg, v < 0)));
}

export function processAxis(cfg: AxisConfig, raw16: number): number {
  return Math.round(response(cfg, buildLut(cfg.curve, cfg.sensitivity), deflection(cfg, raw16)));
}
