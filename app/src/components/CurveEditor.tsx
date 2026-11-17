import { useState } from 'react';
import type { MouseEvent, PointerEvent, ReactNode } from 'react';
import { buildLut, clamp, liveRange, OUT_MAX, response, STEPS } from '../lib/pipeline';
import styles from '../styles/ui.module.css';
import type { AxisConfig, Curve } from '../types';

const SIZE = 340;
const PAD = 12;
const PLOT = SIZE - 2 * PAD;
const GRID = [-0.5, 0, 0.5];
const HIT_RADIUS = 8;
const LINE_HIT = 5;

type Pt = [number, number];
type Hover = { kind: 'point'; index: number; negative: boolean } | { kind: 'line'; slope: number; at: Pt };

// Plot space is deflection (x) and output (y), both -1..1.
const px = (x: number) => PAD + ((x + 1) / 2) * PLOT;
const py = (y: number) => PAD + ((1 - y) / 2) * PLOT;
const svgPoints = (pts: Pt[]) => pts.map(([x, y]) => `${px(x)},${py(y)}`).join(' ');
// Point x stays on the LUT grid so the driver's table, and so the drawn line, passes through it.
const snap = (t: number) => Math.round(t * STEPS) / STEPS;
const round3 = (v: number) => Math.round(v * 1000) / 1000;
// Keeps the curve non-decreasing by pushing the neighbors of point i out of its way.
const monotone = (points: Pt[], i: number): Pt[] => {
  const y = points[i]?.[1] ?? 0;
  return points.map(([x, v], j): Pt => [x, j < i ? Math.min(v, y) : j > i ? Math.max(v, y) : v]);
};

function segmentDistance([x, y]: Pt, [ax, ay]: Pt, [bx, by]: Pt) {
  const [dx, dy] = [bx - ax, by - ay];
  const len2 = dx * dx + dy * dy;
  const t = len2 === 0 ? 0 : clamp(((x - ax) * dx + (y - ay) * dy) / len2, 0, 1);
  return Math.hypot(x - (ax + t * dx), y - (ay + t * dy));
}

interface CurveEditorProps {
  axis: AxisConfig;
  /** Live deflection (-1..1) for the marker, null to hide it. */
  input: number | null;
  disabled: boolean;
  onChange: (curve: Curve) => void;
}

export function CurveEditor({ axis, input, disabled, onChange }: CurveEditorProps) {
  const [drag, setDrag] = useState<number | null>(null);
  const [hover, setHover] = useState<Hover | null>(null);
  const { deadzone: dz, curve, sensitivity } = axis;
  const points = curve.type === 'points' ? curve.points : null;
  const lut = buildLut(curve, sensitivity);

  // Curve input t (0..1) on one side -> deflection.
  const toX = (t: number, negative: boolean) => (negative ? -1 : 1) * (dz.center + t * liveRange(axis, negative));
  // Exact vertices of what the driver does: flat center deadzone, LUT nodes, flat end deadzone, mirrored.
  const outline = (l: number[]): Pt[] => {
    const half = (negative: boolean): Pt[] => {
      const s = negative ? -1 : 1;
      return [
        [s * dz.center, 0],
        ...l.map((v, i): Pt => [toX(i / STEPS, negative), (s * v) / OUT_MAX]),
        [s, (s * (l[STEPS] ?? 0)) / OUT_MAX],
      ];
    };
    return [...half(true).reverse(), ...half(false)];
  };
  const line = outline(lut);
  const handles = (points ?? []).flatMap(([x, y], index) => {
    const out = Math.min(y * sensitivity, 1);
    return [false, true].map((negative) => ({
      index,
      negative,
      at: [toX(x, negative), negative ? -out : out] as Pt,
    }));
  });

  const toSvg = (e: MouseEvent<SVGSVGElement>): Pt => {
    const r = e.currentTarget.getBoundingClientRect();
    return [((e.clientX - r.left) * SIZE) / r.width, ((e.clientY - r.top) * SIZE) / r.height];
  };
  // A plot position as a point on the stored (positive-side) curve; the negative side mirrors.
  const toCurve = ([sx, sy]: Pt) => {
    const x = clamp(((sx - PAD) / PLOT) * 2 - 1, -1, 1);
    const y = clamp(1 - ((sy - PAD) / PLOT) * 2, -1, 1);
    const negative = x < 0;
    const t = (Math.abs(x) - dz.center) / liveRange(axis, negative);
    return { negative, t, y: clamp((negative ? -y : y) / sensitivity, 0, 1) };
  };
  // Hit-test by distance rather than event target: pointer capture retargets click/dblclick.
  const hitHandle = ([sx, sy]: Pt) => {
    let best: (typeof handles)[number] | undefined;
    let bestDist = HIT_RADIUS;
    for (const h of handles) {
      const d = Math.hypot(px(h.at[0]) - sx, py(h.at[1]) - sy);
      if (d <= bestDist) [best, bestDist] = [h, d];
    }
    return best;
  };
  const hitLine = (s: Pt): Hover | null => {
    let best: Hover | null = null;
    let bestDist = LINE_HIT;
    line.forEach((a, i) => {
      const b = line[i + 1];
      // Vertical steps (an offset at the deadzone edge) have no finite slope.
      if (!b || Math.abs(b[0] - a[0]) < 1e-9) return;
      const d = segmentDistance(s, [px(a[0]), py(a[1])], [px(b[0]), py(b[1])]);
      if (d <= bestDist) [best, bestDist] = [{ kind: 'line', slope: (b[1] - a[1]) / (b[0] - a[0]), at: s }, d];
    });
    return best;
  };

  const onPointerDown = (e: PointerEvent<SVGSVGElement>) => {
    if (disabled || !points || e.button !== 0) return;
    const index = hitHandle(toSvg(e))?.index;
    if (index === undefined) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    setDrag(index);
  };

  const onPointerMove = (e: PointerEvent<SVGSVGElement>) => {
    const s = toSvg(e);
    if (drag === null || !points) {
      const h = hitHandle(s);
      setHover(h ? { kind: 'point', index: h.index, negative: h.negative } : hitLine(s));
      return;
    }
    const c = toCurve(s);
    const last = points.length - 1;
    const own = points[drag]?.[0] ?? 0;
    const lo = (points[drag - 1]?.[0] ?? 0) + 1 / STEPS;
    const hi = (points[drag + 1]?.[0] ?? 1) - 1 / STEPS;
    // Endpoints keep their x; a point squeezed between neighbors (imported profiles) keeps its x too.
    const x = drag === 0 || drag === last || lo > hi ? own : clamp(snap(c.t), lo, hi);
    const moved = points.map((p, i): Pt => (i === drag ? [x, round3(c.y)] : p));
    onChange({ type: 'points', points: monotone(moved, drag) });
    setHover({ kind: 'point', index: drag, negative: c.negative });
  };

  // On a point: remove it. Elsewhere: add one at the nearest free grid slot.
  const onDoubleClick = (e: MouseEvent<SVGSVGElement>) => {
    if (disabled || !points) return;
    const s = toSvg(e);
    const hit = hitHandle(s)?.index;
    if (hit !== undefined) {
      // Endpoints cannot be deleted, which also keeps the 2-point minimum.
      if (hit === 0 || hit === points.length - 1) return;
      onChange({ type: 'points', points: points.filter((_, i) => i !== hit) });
      setHover(null);
      return;
    }
    const c = toCurve(s);
    const t = snap(c.t);
    const index = points.findIndex((p) => p[0] > t);
    // Past the ends, in a deadzone, or on an occupied grid slot: nothing to add.
    const prev = points[index - 1];
    if (!prev || prev[0] === t) return;
    const added: Pt[] = [...points.slice(0, index), [t, round3(c.y)], ...points.slice(index)];
    onChange({ type: 'points', points: monotone(added, index) });
  };

  const endDrag = () => {
    setDrag(null);
  };

  let tip: { at: Pt; text: string } | null = null;
  if (hover?.kind === 'point') {
    const h = handles.find((c) => c.index === hover.index && c.negative === hover.negative);
    if (h) tip = { at: [px(h.at[0]), py(h.at[1])], text: `in ${h.at[0].toFixed(3)}, out ${h.at[1].toFixed(3)}` };
  } else if (hover) {
    tip = { at: hover.at, text: `Sensitivity ${hover.slope.toFixed(2)}` };
  }
  const bands: Pt[] = [
    [-dz.center, dz.center],
    [-1, -1 + dz.min],
    [1 - dz.max, 1],
  ];

  return (
    <div className={styles.curveBox}>
      <svg
        className={styles.curve}
        viewBox={`0 0 ${SIZE} ${SIZE}`}
        width={SIZE}
        height={SIZE}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
        onPointerLeave={() => {
          if (drag === null) setHover(null);
        }}
        onDoubleClick={onDoubleClick}
      >
        <rect className={styles.curveBg} x={PAD} y={PAD} width={PLOT} height={PLOT} />
        {bands.map(
          ([a, b], i) =>
            b > a && (
              <rect key={i} className={styles.curveDeadzone} x={px(a)} y={PAD} width={px(b) - px(a)} height={PLOT} />
            ),
        )}
        {GRID.map((g) => (
          <g key={g} className={styles.curveGrid}>
            <line x1={px(g)} y1={py(-1)} x2={px(g)} y2={py(1)} />
            <line x1={px(-1)} y1={py(g)} x2={px(1)} y2={py(g)} />
          </g>
        ))}
        <polyline className={styles.curveLine} points={svgPoints(line)} />
        {/* Over the curve, so it stays visible where the two coincide. */}
        <polyline className={styles.curveIdentity} points={svgPoints(outline(buildLut({ type: 'linear' }, sensitivity)))} />
        {handles.map((h) => (
          <circle
            key={`${h.index}${h.negative ? '-' : '+'}`}
            className={styles.handle}
            cx={px(h.at[0])}
            cy={py(h.at[1])}
            r={5}
          />
        ))}
        {input !== null && (
          <circle className={styles.marker} cx={px(input)} cy={py(response(axis, lut, input) / OUT_MAX)} r={5} />
        )}
        {tip && (
          <text
            className={styles.curveTip}
            x={tip.at[0] + (tip.at[0] > SIZE / 2 ? -10 : 10)}
            y={tip.at[1] < 30 ? tip.at[1] + 22 : tip.at[1] - 10}
            textAnchor={tip.at[0] > SIZE / 2 ? 'end' : 'start'}
          >
            {tip.text}
          </text>
        )}
      </svg>
      <ul className={styles.legend}>
        <li>
          <Swatch>
            <line className={styles.curveLine} x1={1} y1={6} x2={19} y2={6} />
          </Swatch>
          Response
        </li>
        <li>
          <Swatch>
            <line className={styles.curveIdentity} x1={1} y1={6} x2={19} y2={6} />
          </Swatch>
          Linear at this sensitivity
        </li>
        <li>
          <Swatch>
            <rect className={styles.curveDeadzone} x={1} y={0} width={18} height={12} />
          </Swatch>
          Deadzone
        </li>
        {points && (
          <li>
            <Swatch>
              <circle className={styles.handle} cx={10} cy={6} r={4} />
            </Swatch>
            Point
          </li>
        )}
        <li>
          <Swatch>
            <circle className={styles.marker} cx={10} cy={6} r={4} />
          </Swatch>
          Live input
        </li>
      </ul>
    </div>
  );
}

function Swatch({ children }: { children: ReactNode }) {
  return (
    <svg width={20} height={12} aria-hidden>
      {children}
    </svg>
  );
}
