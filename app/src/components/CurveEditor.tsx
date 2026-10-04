import { useState } from 'react';
import type { MouseEvent, PointerEvent } from 'react';
import { buildLut, lutAt } from '../lib/pipeline';
import styles from '../styles/ui.module.css';
import type { Curve } from '../types';

const SIZE = 300;
const PAD = 12;
const PLOT = SIZE - 2 * PAD;
const MAX_POINTS = 17;
// Keeps point x strictly increasing, as the contract requires.
const MIN_GAP = 0.01;
const GRID = [0.25, 0.5, 0.75];
const HIT_RADIUS = 9;

const px = (x: number) => PAD + x * PLOT;
const py = (y: number) => PAD + (1 - y) * PLOT;
const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
const round3 = (v: number) => Math.round(v * 1000) / 1000;

interface CurveEditorProps {
  curve: Curve;
  sensitivity: number;
  /** Current curve input (0..1) for the live marker, null to hide it. */
  input: number | null;
  onChange: (curve: Curve) => void;
}

export function CurveEditor({ curve, sensitivity, input, onChange }: CurveEditorProps) {
  const [drag, setDrag] = useState<number | null>(null);
  // Draw the 17-point LUT itself: that is what the driver interpolates.
  const lut = buildLut(curve, sensitivity);
  const points = curve.type === 'points' ? curve.points : null;

  const toSvg = (e: MouseEvent<SVGSVGElement>): [number, number] => {
    const r = e.currentTarget.getBoundingClientRect();
    return [((e.clientX - r.left) * SIZE) / r.width, ((e.clientY - r.top) * SIZE) / r.height];
  };
  const toUnit = (e: MouseEvent<SVGSVGElement>): [number, number] => {
    const [sx, sy] = toSvg(e);
    return [clamp((sx - PAD) / PLOT, 0, 1), clamp(1 - (sy - PAD) / PLOT, 0, 1)];
  };
  // Hit-test by distance rather than event target: pointer capture retargets click/dblclick.
  const hitIndex = (e: MouseEvent<SVGSVGElement>) => {
    const [sx, sy] = toSvg(e);
    let best = -1;
    let bestDist = HIT_RADIUS;
    points?.forEach(([x, y], i) => {
      const d = Math.hypot(px(x) - sx, py(y) - sy);
      if (d <= bestDist) [best, bestDist] = [i, d];
    });
    return best;
  };

  const onPointerDown = (e: PointerEvent<SVGSVGElement>) => {
    if (!points || e.button !== 0) return;
    let index = hitIndex(e);
    if (index < 0) {
      const [x, y] = toUnit(e);
      index = points.findIndex((p) => p[0] > x);
      const prev = points[index - 1];
      const next = points[index];
      if (points.length >= MAX_POINTS || !prev || !next) return;
      if (x - prev[0] < MIN_GAP || next[0] - x < MIN_GAP) return;
      onChange({ type: 'points', points: [...points.slice(0, index), [round3(x), round3(y)], ...points.slice(index)] });
    }
    e.currentTarget.setPointerCapture(e.pointerId);
    setDrag(index);
  };

  const onPointerMove = (e: PointerEvent<SVGSVGElement>) => {
    if (drag === null || !points) return;
    const [x, y] = toUnit(e);
    const last = points.length - 1;
    const lo = (points[drag - 1]?.[0] ?? 0) + MIN_GAP;
    const hi = (points[drag + 1]?.[0] ?? 1) - MIN_GAP;
    // Endpoints stay pinned at x = 0 and x = 1.
    const nx = drag === 0 ? 0 : drag === last ? 1 : round3(clamp(x, lo, hi));
    onChange({
      type: 'points',
      points: points.map((p, i): [number, number] => (i === drag ? [nx, round3(y)] : p)),
    });
  };

  const onDoubleClick = (e: MouseEvent<SVGSVGElement>) => {
    const index = hitIndex(e);
    // Endpoints are pinned, so they (and the 2-point minimum) cannot be deleted.
    if (!points || index <= 0 || index === points.length - 1) return;
    onChange({ type: 'points', points: points.filter((_, i) => i !== index) });
  };

  const endDrag = () => {
    setDrag(null);
  };

  return (
    <svg
      className={styles.curve}
      viewBox={`0 0 ${SIZE} ${SIZE}`}
      width={SIZE}
      height={SIZE}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={endDrag}
      onPointerCancel={endDrag}
      onDoubleClick={onDoubleClick}
    >
      <rect className={styles.curveBg} x={PAD} y={PAD} width={PLOT} height={PLOT} />
      {GRID.map((g) => (
        <g key={g} className={styles.curveGrid}>
          <line x1={px(g)} y1={py(0)} x2={px(g)} y2={py(1)} />
          <line x1={px(0)} y1={py(g)} x2={px(1)} y2={py(g)} />
        </g>
      ))}
      <line className={styles.curveIdentity} x1={px(0)} y1={py(0)} x2={px(1)} y2={py(1)} />
      <polyline
        className={styles.curveLine}
        points={lut.map((v, i) => `${px(i / 16)},${py(v / 65535)}`).join(' ')}
      />
      {points?.map(([x, y], i) => <circle key={i} className={styles.handle} cx={px(x)} cy={py(y)} r={6} />)}
      {input !== null && (
        <circle className={styles.marker} cx={px(input)} cy={py(lutAt(lut, input) / 65535)} r={5} />
      )}
    </svg>
  );
}
