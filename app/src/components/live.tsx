import styles from '../styles/ui.module.css';
import { cx } from './controls';

interface AxisBarProps {
  label: string;
  value: number;
  max: number;
  /** Fill grows from the middle; for axes that rest at centre. */
  centered?: boolean;
}

export function AxisBar({ label, value, max, centered = false }: AxisBarProps) {
  const pct = Math.min(Math.max(value / max, 0), 1) * 100;
  const fill = centered ? { left: `${Math.min(pct, 50)}%`, width: `${Math.abs(pct - 50)}%` } : { width: `${pct}%` };
  return (
    <div className={styles.axisBar}>
      <span className={styles.axisLabel}>{label}</span>
      <div className={styles.track}>
        <div className={styles.fill} style={fill} />
        {centered && <div className={styles.centerLine} />}
      </div>
      <span className={styles.axisValue}>
        {value} / {max}
      </span>
    </div>
  );
}

interface ButtonGridProps {
  count: number;
  mask: number;
}

export function ButtonGrid({ count, mask }: ButtonGridProps) {
  return (
    <div className={styles.buttonGrid}>
      {Array.from({ length: count }, (_, i) => (
        <span key={i} className={cx(styles.cell, ((mask >>> i) & 1) === 1 && styles.on)}>
          {i + 1}
        </span>
      ))}
    </div>
  );
}

const ARROWS = ['↑', '↗', '→', '↘', '↓', '↙', '←', '↖'];
// 3×3 layout of hat values (0 = N, clockwise); 8 = centred.
const ROSE = [7, 0, 1, 6, 8, 2, 5, 4, 3];

export function HatRose({ label, value }: { label: string; value: number }) {
  return (
    <div className={styles.hat}>
      <span className={styles.fieldLabel}>{label}</span>
      <div className={styles.hatGrid}>
        {ROSE.map((h) => (
          <span key={h} className={cx(styles.cell, value === h && styles.on)}>
            {ARROWS[h] ?? '•'}
          </span>
        ))}
      </div>
    </div>
  );
}
