import { useState } from 'react';
import { cx, Select } from './components/controls';
import { AxesPage } from './pages/AxesPage';
import { ButtonsPage } from './pages/ButtonsPage';
import { ProfilesPage } from './pages/ProfilesPage';
import { SettingsPage } from './pages/SettingsPage';
import { TestPage } from './pages/TestPage';
import { useStore } from './store';
import styles from './styles/ui.module.css';

const PAGES = {
  test: ['Test', TestPage],
  axes: ['Axes', AxesPage],
  buttons: ['Buttons', ButtonsPage],
  profiles: ['Profiles', ProfilesPage],
  settings: ['Settings', SettingsPage],
} as const;
type PageId = keyof typeof PAGES;

function StatusBar() {
  const appState = useStore((s) => s.appState);
  const profiles = useStore((s) => s.profiles);
  const reason = useStore((s) => s.lastSwitch?.reason);
  if (!appState) return null;
  const active = profiles.find((p) => p.id === appState.activeProfileId);
  return (
    <div className={styles.wrap}>
      <span className={cx(styles.badge, appState.deviceConnected ? styles.ok : styles.bad)}>
        {appState.deviceConnected ? 'joystick connected' : 'joystick not connected'}
      </span>
      <span className={cx(styles.badge, appState.driverInstalled ? styles.ok : styles.warn)}>
        {appState.driverInstalled ? `driver ${appState.driverVersion ?? ''}` : 'no driver (preview only)'}
      </span>
      <span className={styles.badge}>
        active: {active?.name ?? '—'}
        {reason && ` (${reason})`}
        {appState.settings.switchingPaused && ' · switching paused'}
      </span>
    </div>
  );
}

export function App() {
  const [page, setPage] = useState<PageId>('test');
  const profiles = useStore((s) => s.profiles);
  const draft = useStore((s) => s.draft);
  const dirty = useStore((s) => s.dirty);
  const error = useStore((s) => s.error);
  const { select, save, discard } = useStore.getState();
  const Page = PAGES[page][1];

  return (
    <div className={styles.app}>
      <header className={styles.header}>
        <h1>Open X3D Pro</h1>
        <StatusBar />
        <span className={styles.grow} />
        {draft && (
          <Select
            label="Editing"
            value={draft.id}
            options={profiles.map((p) => ({ value: p.id, label: p.name }))}
            onChange={select}
          />
        )}
      </header>

      <nav className={styles.tabs}>
        {(Object.keys(PAGES) as PageId[]).map((id) => (
          <button
            key={id}
            className={cx(id === page && styles.active)}
            onClick={() => {
              setPage(id);
            }}
          >
            {PAGES[id][0]}
          </button>
        ))}
      </nav>

      {error && (
        <div className={styles.error}>
          <span className={styles.grow}>{error}</span>
          <button
            onClick={() => {
              useStore.setState({ error: null });
            }}
          >
            Dismiss
          </button>
        </div>
      )}

      <main className={styles.main}>
        <Page />
      </main>

      {dirty && draft && (
        <footer className={styles.saveBar}>
          <span className={styles.grow}>Unsaved changes to “{draft.name}” — the joystick is previewing them live.</span>
          <button onClick={discard}>Discard</button>
          <button className={styles.primary} onClick={() => void save()}>
            Save
          </button>
        </footer>
      )}
    </div>
  );
}
