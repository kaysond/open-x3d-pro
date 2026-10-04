import { Radio, Toggle } from '../components/controls';
import { useStore } from '../store';
import styles from '../styles/ui.module.css';

export function SettingsPage() {
  const appState = useStore((s) => s.appState);
  const saveSettings = useStore((s) => s.saveSettings);
  if (!appState) return null;
  const { settings } = appState;

  return (
    <div className={styles.stack}>
      <section className={styles.card}>
        <h2>Behaviour</h2>
        <Toggle
          label="Start with Windows (in the tray)"
          checked={settings.autostart}
          onChange={(autostart) => void saveSettings({ autostart })}
        />
        <Toggle
          label="Pause automatic profile switching"
          checked={settings.switchingPaused}
          onChange={(switchingPaused) => void saveSettings({ switchingPaused })}
        />
        <Radio
          label="Switch profile when the game is…"
          value={settings.matchMode}
          options={[
            { value: 'foreground', label: 'In the foreground' },
            { value: 'running', label: 'Running (survives overlays and alt-tab)' },
          ]}
          onChange={(matchMode) => void saveSettings({ matchMode })}
        />
      </section>

      <section className={styles.card}>
        <h2>Status</h2>
        <table className={styles.table}>
          <tbody>
            <tr>
              <th>Driver</th>
              <td>
                {appState.driverInstalled
                  ? `installed, version ${appState.driverVersion ?? 'unknown'}`
                  : 'not installed — the app applies profiles to its own live view only'}
              </td>
            </tr>
            <tr>
              <th>Joystick</th>
              <td>{appState.deviceConnected ? 'connected' : 'not connected'}</td>
            </tr>
            <tr>
              <th>Matched executable</th>
              <td className={styles.mono}>{appState.matchedExe ?? '—'}</td>
            </tr>
          </tbody>
        </table>
      </section>

      <section className={styles.card}>
        <h2>About the beta certificate</h2>
        <p className={styles.muted}>
          Beta builds are signed with a self-signed certificate. The installer adds it to the machine&apos;s Root and
          TrustedPublisher stores so Windows accepts the driver, which means this PC trusts anything signed with it. The
          uninstaller removes it again.
        </p>
      </section>
    </div>
  );
}
