import { useState } from 'react';
import { cx, Radio, Select, Slider, Toggle } from '../components/controls';
import { CurveEditor } from '../components/CurveEditor';
import { api } from '../lib/ipc';
import { axisInput, clamp, curveAt, toRaw16 } from '../lib/pipeline';
import { guard, useStore } from '../store';
import styles from '../styles/ui.module.css';
import type { AxisConfig, AxisId, Calibration, Curve } from '../types';
import { AXES, AXIS_LABELS } from '../types';

const pct = (v: number) => `${Math.round(v * 100)} %`;
// Two deadzones at 0.5 would leave no live range, which the backend rejects.
const DZ_MAX = 0.49;

const TARGETS: { value: AxisId | null; label: string }[] = [
  ...AXES.map((id) => ({ value: id, label: AXIS_LABELS[id] })),
  { value: null, label: 'Disabled' },
];

function switchCurve(curve: Curve, type: Curve['type']): Curve {
  if (type === 'linear') return { type };
  if (type === 'exponent') return { type, exponent: 2 };
  // Start the point editor from the current shape so switching does not jump.
  return { type, points: [0, 0.25, 0.5, 0.75, 1].map((x): [number, number] => [x, curveAt(curve, x)]) };
}

/** Commits on blur or Enter, so typing a number never passes through invalid in-between values. */
function CalInput({ value, disabled, onCommit }: { value: number; disabled: boolean; onCommit: (v: number) => void }) {
  const [text, setText] = useState<string | null>(null);
  const commit = () => {
    if (text !== null && text.trim() !== '' && Number.isFinite(Number(text))) onCommit(Number(text));
    setText(null);
  };
  return (
    <input
      type="number"
      min={0}
      max={65535}
      value={text ?? value}
      disabled={disabled}
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
  const symmetric = cfg.curveMode === 'symmetric';
  const raw = frame?.raw[axis];
  const input = raw === undefined ? null : axisInput(cfg, toRaw16(axis, raw));
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
      if (symmetric && !(cal.min < cal.center && cal.center < cal.max)) {
        throw new Error('the axis was not centred when Finish was clicked; calibrate again and let it return to centre');
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
  const dzLabel = (key: 'center' | 'min' | 'max') =>
    key === 'center' ? 'Centre deadzone' : `Deadzone at ${symmetric ? key : key === 'min' ? 'low' : 'high'} end`;
  const setCal = (key: keyof Calibration, value: number) => {
    edit((a) => {
      a.calibration[key] = Math.round(clamp(value, 0, 65535));
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
                    value={cfg.calibration[key]}
                    disabled={key === 'center' && !symmetric}
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
                  Move the axis to both extremes, then return it to centre and click Finish. Live raw:{' '}
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
            <Radio
              label="Curve mode"
              value={cfg.curveMode}
              options={[
                { value: 'symmetric', label: 'Symmetric (about centre)' },
                { value: 'full', label: 'Full range' },
              ]}
              onChange={(curveMode) => {
                edit((a) => {
                  a.curveMode = curveMode;
                  const { min, center, max } = a.calibration;
                  // Symmetric needs min < center < max (slider default has center = min).
                  if (curveMode === 'symmetric' && !(min < center && center < max)) {
                    // A range too narrow for a centre falls back to the identity calibration.
                    a.calibration =
                      max - min < 2
                        ? { min: 0, center: 32768, max: 65535 }
                        : { min, center: Math.round((min + max) / 2), max };
                  }
                });
              }}
            />
            {(['center', 'min', 'max'] as const).map(
              (key) =>
                (symmetric || key !== 'center') && (
                  <Slider
                    key={key}
                    label={dzLabel(key)}
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
                ),
            )}
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
          <h2>Curve</h2>
          <Radio
            label="Type"
            value={cfg.curve.type}
            options={[
              { value: 'linear', label: 'Linear' },
              { value: 'exponent', label: 'Exponent' },
              { value: 'points', label: 'Points' },
            ]}
            onChange={(type) => {
              edit((a) => {
                a.curve = switchCurve(a.curve, type);
              });
            }}
          />
          {cfg.curve.type === 'exponent' && (
            <Slider
              label="Exponent"
              value={cfg.curve.exponent}
              min={0.2}
              max={5}
              log
              onChange={(exponent) => {
                edit((a) => {
                  a.curve = { type: 'exponent', exponent };
                });
              }}
            />
          )}
          <CurveEditor
            curve={cfg.curve}
            sensitivity={cfg.sensitivity}
            input={input === null ? null : Math.abs(input)}
            onChange={(curve) => {
              edit((a) => {
                a.curve = curve;
              });
            }}
          />
          <p className={styles.muted}>
            {symmetric ? 'x = deflection from centre, either direction' : 'x = position along the full range'}
            {cfg.curve.type === 'points' && ' · click to add, drag to move, double-click to delete'}
          </p>
          <p className={styles.mono}>
            raw {raw ?? '—'} → in {input?.toFixed(3) ?? '—'} → out {output ?? '—'}
          </p>
          <button
            onClick={() => {
              edit((a) => {
                a.curve = { type: 'linear' };
                a.sensitivity = 1;
              });
            }}
          >
            Reset to linear
          </button>
        </section>
      </div>
    </div>
  );
}
