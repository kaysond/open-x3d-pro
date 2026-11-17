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
        <h2>Behavior</h2>
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
                  ? `Installed, version ${appState.driverVersion ?? 'unknown'}`
                  : 'Not installed — the app applies profiles to its own live view only'}
              </td>
            </tr>
            <tr>
              <th>Joystick</th>
              <td>{appState.deviceConnected ? 'Connected' : 'Not connected'}</td>
            </tr>
            <tr>
              <th>Matched executable</th>
              <td className={styles.mono}>{appState.matchedExe ?? '—'}</td>
            </tr>
          </tbody>
        </table>
      </section>

      <section className={styles.card}>
        <h2>Code signing</h2>
        <p className={styles.muted}>
          {appState.driverInstalled
            ? 'The driver was signed on this PC by a certificate the installer created here. Its private key was deleted before the certificate was trusted, so nothing else can ever be signed with it. Uninstalling removes the certificate.'
            : 'The driver is not installed, so no certificate was created. Run the installer again and tick its certificate checkbox to install the driver.'}
        </p>
      </section>
    </div>
  );
}
