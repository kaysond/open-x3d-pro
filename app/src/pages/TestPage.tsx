import { AxisBar, ButtonGrid, HatRose } from '../components/live';
import { OUT_MAX } from '../lib/pipeline';
import { useStore } from '../store';
import styles from '../styles/ui.module.css';
import { AXES, AXIS_LABELS, OUTPUT_BUTTONS, PHYSICAL_BUTTONS } from '../types';

export function TestPage() {
  const frame = useStore((s) => s.frame);
  const hz = useStore((s) => s.hz);
  if (!frame) return <p className={styles.muted}>Waiting for input…</p>;
  const { raw, processed } = frame;

  return (
    <div className={styles.stack}>
      <section className={styles.card}>
        <h2>
          Axes <span className={styles.muted}>— Raw (device units) / Processed (−1..1)</span>
        </h2>
        {AXES.map((id) => (
          <div key={id} className={styles.axisPair}>
            <AxisBar
              label={`${AXIS_LABELS[id]} Raw`}
              value={raw[id]}
              max={id === 'x' || id === 'y' ? 1023 : 255}
              centered={id !== 'slider'}
            />
            <AxisBar
              label={`${AXIS_LABELS[id]} Processed`}
              value={processed[id]}
              min={-OUT_MAX}
              max={OUT_MAX}
              centered
              text={(processed[id] / OUT_MAX).toFixed(3)}
            />
          </div>
        ))}
      </section>

      <div className={styles.row}>
        <section className={styles.card}>
          <h2>Hat</h2>
          <div className={styles.row}>
            <HatRose label="Raw" value={raw.hat} />
            <HatRose label="Processed" value={processed.hat} />
          </div>
        </section>
        <section className={styles.card}>
          <h2>Buttons</h2>
          <span className={styles.fieldLabel}>Physical 1–{PHYSICAL_BUTTONS}</span>
          <ButtonGrid count={PHYSICAL_BUTTONS} mask={raw.buttons} />
          <span className={styles.fieldLabel}>Output 1–{OUTPUT_BUTTONS}</span>
          <ButtonGrid count={OUTPUT_BUTTONS} mask={processed.buttons} />
        </section>
      </div>

      <p className={styles.muted}>Report rate {hz} Hz</p>
    </div>
  );
}
