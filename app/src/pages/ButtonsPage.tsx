import { useEffect, useState } from 'react';
import { cx, Radio, Select } from '../components/controls';
import { useStore } from '../store';
import styles from '../styles/ui.module.css';
import { OUTPUT_BUTTONS, PHYSICAL_BUTTONS } from '../types';

const range = (n: number) => Array.from({ length: n }, (_, i) => i + 1);
const OUTPUTS = [{ value: null, label: 'None' }, ...range(OUTPUT_BUTTONS).map((n) => ({ value: n, label: `${n}` }))];
const PHYSICAL = range(PHYSICAL_BUTTONS).map((n) => ({ value: n, label: `${n}` }));
const SHIFT = [{ value: null, label: 'None' }, ...PHYSICAL];

export function ButtonsPage() {
  const config = useStore((s) => s.draft?.config);
  const pressed = useStore((s) => s.frame?.raw.buttons ?? 0);
  const editConfig = useStore((s) => s.editConfig);
  /** Index of the key binding whose chord is being captured; keyBindings.length = a new one. */
  const [recording, setRecording] = useState<number | null>(null);
  const [live, setLive] = useState<string[]>([]);
  const [newButton, setNewButton] = useState(1);

  // Captures one chord as KeyboardEvent.code names; finishes when every key is released.
  useEffect(() => {
    if (recording === null) return;
    const chord: string[] = [];
    const held = new Set<string>();
    const down = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      held.add(e.code);
      if (!chord.includes(e.code)) {
        chord.push(e.code);
        setLive([...chord]);
      }
    };
    const up = (e: KeyboardEvent) => {
      e.preventDefault();
      held.delete(e.code);
      if (held.size > 0 || chord.length === 0) return;
      setRecording(null);
      editConfig((c) => {
        const kb = c.keyBindings[recording];
        // New bindings enter the config only with keys: the backend rejects empty chords.
        if (kb) kb.keys = chord;
        else c.keyBindings.push({ button: newButton, keys: chord });
      });
    };
    window.addEventListener('keydown', down, true);
    window.addEventListener('keyup', up, true);
    return () => {
      window.removeEventListener('keydown', down, true);
      window.removeEventListener('keyup', up, true);
    };
  }, [recording, editConfig, newButton]);

  const record = (i: number | null) => {
    setLive([]);
    setRecording(i);
  };

  if (!config) return null;
  const shiftSet = config.shiftButton !== null;

  return (
    <div className={styles.row}>
      <section className={styles.card}>
        <h2>Button map</h2>
        <Select
          label="Shift button"
          value={config.shiftButton}
          options={SHIFT}
          onChange={(shiftButton) => {
            editConfig((c) => {
              c.shiftButton = shiftButton;
            });
          }}
        />
        <table className={styles.table}>
          <thead>
            <tr>
              <th>Physical</th>
              <th>Output</th>
              <th>Output while shifted</th>
            </tr>
          </thead>
          <tbody>
            {PHYSICAL.map(({ value: n }) => (
              <tr key={n}>
                <td>
                  <span className={cx(styles.cell, ((pressed >>> (n - 1)) & 1) === 1 && styles.on)}>{n}</span>
                  {config.shiftButton === n && <span className={styles.badge}>shift</span>}
                </td>
                <td>
                  <Select
                    value={config.buttons[n - 1]?.output ?? null}
                    options={OUTPUTS}
                    onChange={(output) => {
                      editConfig((c) => {
                        c.buttons[n - 1] = { output };
                      });
                    }}
                  />
                </td>
                <td>
                  <Select
                    value={config.buttonsShifted[n - 1]?.output ?? null}
                    options={OUTPUTS}
                    disabled={!shiftSet}
                    onChange={(output) => {
                      editConfig((c) => {
                        c.buttonsShifted[n - 1] = { output };
                      });
                    }}
                  />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>

      <div className={styles.stack}>
        <section className={styles.card}>
          <h2>Hat</h2>
          <Radio
            label="Hat mode"
            value={config.hatMode}
            options={[
              { value: 'hat', label: 'Hat switch' },
              { value: 'buttons', label: 'Buttons 13–16' },
              { value: 'both', label: 'Both' },
            ]}
            onChange={(hatMode) => {
              editConfig((c) => {
                c.hatMode = hatMode;
              });
            }}
          />
        </section>

        <section className={styles.card}>
          <h2>Key bindings</h2>
          <p className={styles.muted}>Sent by the app as keystrokes while the physical button is held.</p>
          {config.keyBindings.map((kb, i) => (
            <div key={i} className={styles.row}>
              <Select
                value={kb.button}
                options={PHYSICAL}
                onChange={(button) => {
                  editConfig((c) => {
                    const b = c.keyBindings[i];
                    if (b) b.button = button;
                  });
                }}
              />
              <span className={cx(styles.mono, styles.grow)}>
                {recording === i ? (live.length ? live.join(' + ') : 'Press keys…') : kb.keys.join(' + ') || '—'}
              </span>
              <button
                onClick={() => {
                  record(recording === i ? null : i);
                }}
              >
                {recording === i ? 'Cancel' : 'Record keys'}
              </button>
              <button
                onClick={() => {
                  record(null);
                  editConfig((c) => {
                    c.keyBindings.splice(i, 1);
                  });
                }}
              >
                Remove
              </button>
            </div>
          ))}
          {recording === config.keyBindings.length ? (
            <div className={styles.row}>
              <Select value={newButton} options={PHYSICAL} onChange={setNewButton} />
              <span className={cx(styles.mono, styles.grow)}>{live.length ? live.join(' + ') : 'Press keys…'}</span>
              <button
                onClick={() => {
                  record(null);
                }}
              >
                Cancel
              </button>
            </div>
          ) : (
            <button
              onClick={() => {
                record(config.keyBindings.length);
              }}
            >
              Add binding
            </button>
          )}
        </section>
      </div>
    </div>
  );
}
