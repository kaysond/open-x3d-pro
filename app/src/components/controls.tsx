import styles from '../styles/ui.module.css';

export const cx = (...classes: (string | false | null | undefined)[]) => classes.filter(Boolean).join(' ');

interface SliderProps {
  label: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  /** Equal travel = equal ratio; for gains and exponents. */
  log?: boolean;
  format?: (v: number) => string;
  onChange: (v: number) => void;
}

const LOG_STEPS = 1000;

export function Slider({ label, value, min, max, step = 0.01, log = false, format, onChange }: SliderProps) {
  const toPos = (v: number) => (log ? (Math.log(v / min) / Math.log(max / min)) * LOG_STEPS : v);
  const fromPos = (p: number) => (log ? Math.round(min * Math.pow(max / min, p / LOG_STEPS) * 100) / 100 : p);
  return (
    <label className={styles.field}>
      <span className={styles.fieldLabel}>{label}</span>
      <input
        type="range"
        min={log ? 0 : min}
        max={log ? LOG_STEPS : max}
        step={log ? 1 : step}
        value={toPos(value)}
        onChange={(e) => {
          onChange(fromPos(Number(e.target.value)));
        }}
      />
      <output className={styles.fieldValue}>{format ? format(value) : value.toFixed(2)}</output>
    </label>
  );
}

interface ToggleProps {
  label: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (checked: boolean) => void;
}

export function Toggle({ label, checked, disabled, onChange }: ToggleProps) {
  return (
    <label className={styles.toggle}>
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={(e) => {
          onChange(e.target.checked);
        }}
      />
      {label}
    </label>
  );
}

interface Option<T> {
  value: T;
  label: string;
}

interface SelectProps<T> {
  label?: string;
  value: T;
  options: Option<T>[];
  disabled?: boolean;
  onChange: (value: T) => void;
}

/** Option values may be null/number, so the DOM value is the option index. */
export function Select<T>({ label, value, options, disabled, onChange }: SelectProps<T>) {
  const select = (
    <select
      value={options.findIndex((o) => o.value === value)}
      disabled={disabled}
      onChange={(e) => {
        const o = options[Number(e.target.value)];
        if (o) onChange(o.value);
      }}
    >
      {options.map((o, i) => (
        <option key={i} value={i}>
          {o.label}
        </option>
      ))}
    </select>
  );
  if (label === undefined) return select;
  return (
    <label className={styles.field}>
      <span className={styles.fieldLabel}>{label}</span>
      {select}
    </label>
  );
}

interface RadioProps<T extends string> {
  label: string;
  value: T;
  options: Option<T>[];
  onChange: (value: T) => void;
}

export function Radio<T extends string>({ label, value, options, onChange }: RadioProps<T>) {
  return (
    <fieldset className={styles.radio}>
      <legend>{label}</legend>
      {options.map((o) => (
        <label key={o.value}>
          <input
            type="radio"
            checked={o.value === value}
            onChange={() => {
              onChange(o.value);
            }}
          />
          {o.label}
        </label>
      ))}
    </fieldset>
  );
}
