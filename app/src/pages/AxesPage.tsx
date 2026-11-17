import { useState } from 'react';
import { cx, Select, Slider, Toggle } from '../components/controls';
import { CurveEditor } from '../components/CurveEditor';
import { api } from '../lib/ipc';
import { clamp, deflection, OUT_MAX, rawShift, toRaw16 } from '../lib/pipeline';
import { guard, useStore } from '../store';
import styles from '../styles/ui.module.css';
import type { AxisConfig, AxisId, Calibration } from '../types';
import { AXES, AXIS_LABELS } from '../types';

const pct = (v: number) => `${Math.round(v * 100)} %`;
// Two deadzones at 0.5 would leave no live range, which the backend rejects.
const DZ_MAX = 0.49;
const DZ_LABELS = { center: 'Center Deadzone', min: 'Low Deadzone', max: 'High Deadzone' } as const;

const TARGETS: { value: AxisId | null; label: string }[] = [
  ...AXES.map((id) => ({ value: id, label: AXIS_LABELS[id] })),
  { value: null, label: 'Disabled' },
];

/** Commits on blur or Enter, so typing a number never passes through invalid in-between values. */
function CalInput({ value, max, onCommit }: { value: number; max: number; onCommit: (v: number) => void }) {
  const [text, setText] = useState<string | null>(null);
  const commit = () => {
    if (text !== null && text.trim() !== '' && Number.isFinite(Number(text))) onCommit(Number(text));
    setText(null);
  };
  return (
    <input
      type="number"
      min={0}
      max={max}
      value={text ?? value}
      onChange={(e) => {
        setText(e.target.value);
      }}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === 'Enter') commit();
        if (e.key === 'Escape') setText(null);
      }}
    />
  );
}

export function AxesPage() {
  const [axis, setAxis] = useState<AxisId>('x');
  const [calibrating, setCalibrating] = useState<AxisId | null>(null);
  const cfg = useStore((s) => s.draft?.config.axes[axis]);
  const frame = useStore((s) => s.frame);
  const editConfig = useStore((s) => s.editConfig);
  if (!cfg) return null;

  const edit = (mutate: (a: AxisConfig) => void) => {
    editConfig((c) => {
      mutate(c.axes[axis]);
    });
  };
  const raw = frame?.raw[axis];
  const input = raw === undefined ? null : deflection(cfg, toRaw16(axis, raw));
  const custom = cfg.curve.type === 'points';
  const output = frame && cfg.target !== null ? frame.processed[cfg.target] : null;

  const startCalibration = () =>
    guard(async () => {
      await api.startCalibration(axis);
      setCalibrating(axis);
    });
  // finish_calibration ends tracking on the backend even when it errors, so leave the wizard first.
  const finishCalibration = () =>
    guard(async () => {
      setCalibrating(null);
      const cal = await api.finishCalibration(axis);
      if (!(cal.min < cal.center && cal.center < cal.max)) {
        throw new Error('the axis was not centered when Finish was clicked; calibrate again and let it return to center');
      }
      edit((a) => {
        a.calibration = cal;
      });
    });
  const cancelCalibration = () => {
    setCalibrating(null);
    // There is no cancel command; finishing and dropping the result (or error) is the cancel.
    api.finishCalibration(axis).catch(() => undefined);
  };
  // Fields are in device units; the config stores 16-bit units.
  const shift = rawShift(axis);
  const deviceMax = 0xffff >> shift;
  const setCal = (key: keyof Calibration, value: number) => {
    edit((a) => {
      a.calibration[key] = Math.round(clamp(value, 0, deviceMax)) << shift;
    });
  };

  return (
    <div className={styles.stack}>
      <nav className={styles.subTabs}>
        {AXES.map((id) => (
          <button
            key={id}
            className={cx(id === axis && styles.active)}
            disabled={calibrating !== null && calibrating !== id}
            onClick={() => {
              setAxis(id);
            }}
          >
            {AXIS_LABELS[id]}
          </button>
        ))}
      </nav>

      <div className={styles.row}>
        <div className={styles.stack}>
          <section className={styles.card}>
            <h2>Calibration</h2>
            <div className={styles.row}>
              {(['min', 'center', 'max'] as const).map((key) => (
                <label key={key} className={styles.field}>
                  <span className={styles.fieldLabel}>{key}</span>
                  <CalInput
                    value={cfg.calibration[key] >> shift}
                    max={deviceMax}
                    onCommit={(v) => {
                      setCal(key, v);
                    }}
                  />
                </label>
              ))}
            </div>
            {calibrating === axis ? (
              <>
                <p>
                  {axis === 'slider'
                    ? 'Move the throttle to both extremes, then click Finish.'
                    : 'Move the axis to both extremes, then return it to center and click Finish.'}{' '}
                  Live raw:{' '}
                  {raw ?? '—'}
                </p>
                <div className={styles.row}>
                  <button onClick={() => void finishCalibration()}>Finish</button>
                  <button onClick={cancelCalibration}>Cancel</button>
                </div>
              </>
            ) : (
              <button onClick={() => void startCalibration()}>Calibrate…</button>
            )}
          </section>

          <section className={styles.card}>
            <h2>Response</h2>
            <Select
              label="Output axis"
              value={cfg.target}
              options={TARGETS}
              onChange={(target) => {
                edit((a) => {
                  a.target = target;
                });
              }}
            />
            <Toggle
              label="Invert"
              checked={cfg.invert}
              onChange={(invert) => {
                edit((a) => {
                  a.invert = invert;
                });
              }}
            />
            {(['center', 'min', 'max'] as const).map((key) => (
              <Slider
                key={key}
                label={DZ_LABELS[key]}
                value={cfg.deadzone[key]}
                min={0}
                max={DZ_MAX}
                format={pct}
                onChange={(v) => {
                  edit((a) => {
                    a.deadzone[key] = v;
                  });
                }}
              />
            ))}
            <Slider
              label="Sensitivity"
              value={cfg.sensitivity}
              min={0.1}
              max={4}
              log
              format={(v) => `×${v.toFixed(2)}`}
              onChange={(v) => {
                edit((a) => {
                  a.sensitivity = v;
                });
              }}
            />
          </section>
        </div>

        <section className={styles.card}>
          <div className={styles.cardHeader}>
            <h2>Custom Curve</h2>
            <input
              type="checkbox"
              role="switch"
              className={styles.switch}
              aria-label="Custom curve"
              checked={custom}
              onChange={(e) => {
                const on = e.target.checked;
                edit((a) => {
                  // Start from the linear shape so turning it on does not change the output.
                  a.curve = on
                    ? // Mirrored, this is 5 points: both ends, the center, and halfway on each side.
                      { type: 'points', points: [[0, 0], [0.5, 0.5], [1, 1]] }
                    : { type: 'linear' };
                });
              }}
            />
          </div>
          <fieldset className={styles.cardBody} disabled={!custom}>
            <CurveEditor
              axis={cfg}
              input={input}
              disabled={!custom}
              onChange={(curve) => {
                edit((a) => {
                  a.curve = curve;
                });
              }}
            />
            {custom && (
              <p className={styles.muted}>
                Double-click to add or remove a point; drag to move one. The other side mirrors.
              </p>
            )}
            <p className={styles.mono}>
              raw {raw ?? '—'} → in {input?.toFixed(3) ?? '—'} → out{' '}
              {output === null ? '—' : (output / OUT_MAX).toFixed(3)}
            </p>
          </fieldset>
        </section>
      </div>
    </div>
  );
}
