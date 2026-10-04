import { useEffect, useState } from 'react';
import { cx } from '../components/controls';
import { api } from '../lib/ipc';
import { guard, useStore } from '../store';
import styles from '../styles/ui.module.css';
import type { WindowInfo } from '../types';

function download(name: string, text: string) {
  const url = URL.createObjectURL(new Blob([text], { type: 'application/json' }));
  const a = document.createElement('a');
  a.href = url;
  a.download = `${name.replace(/[\\/:*?"<>|]/g, '_')}.json`;
  a.click();
  setTimeout(() => {
    URL.revokeObjectURL(url);
  }, 1000);
}

function RunningAppsModal({ onPick, onClose }: { onPick: (exePath: string) => void; onClose: () => void }) {
  const [windows, setWindows] = useState<WindowInfo[] | null>(null);
  useEffect(() => {
    void guard(async () => {
      setWindows(await api.listWindows());
    });
  }, []);
  return (
    <div className={styles.overlay} onClick={onClose}>
      <div
        className={styles.modal}
        onClick={(e) => {
          e.stopPropagation();
        }}
      >
        <h2>Add running app</h2>
        {windows === null && <p className={styles.muted}>Loading…</p>}
        {windows?.length === 0 && <p className={styles.muted}>No windows found.</p>}
        <ul className={styles.list}>
          {windows?.map((w) => (
            <li key={`${w.pid}-${w.title}`}>
              <button
                className={styles.listItem}
                onClick={() => {
                  onPick(w.exePath);
                }}
              >
                <strong>{w.title || '(untitled)'}</strong>
                <span className={cx(styles.muted, styles.mono)}>{w.exePath}</span>
              </button>
            </li>
          ))}
        </ul>
        <button onClick={onClose}>Close</button>
      </div>
    </div>
  );
}

export function ProfilesPage() {
  const profiles = useStore((s) => s.profiles);
  const draft = useStore((s) => s.draft);
  const activeId = useStore((s) => s.appState?.activeProfileId ?? null);
  const { select, editProfile, createProfile, deleteProfile, setDefault, activate, importProfile } = useStore.getState();
  const [newPath, setNewPath] = useState('');
  const [picking, setPicking] = useState(false);

  const addPath = (path: string) => {
    const p = path.trim();
    if (!draft || !p) return;
    if (!draft.exePaths.some((x) => x.toLowerCase() === p.toLowerCase())) editProfile({ exePaths: [...draft.exePaths, p] });
  };
  const saved = profiles.find((p) => p.id === draft?.id);

  return (
    <div className={styles.row}>
      <section className={styles.card}>
        <h2>Profiles</h2>
        <ul className={styles.list}>
          {profiles.map((p) => (
            <li key={p.id}>
              <button
                className={cx(styles.listItem, p.id === draft?.id && styles.active)}
                onClick={() => {
                  select(p.id);
                }}
              >
                <strong>{p.name}</strong>
                <span>
                  {p.isDefault && <span className={styles.badge}>default</span>}
                  {p.id === activeId && <span className={cx(styles.badge, styles.ok)}>active</span>}
                  <span className={styles.muted}>
                    {p.exePaths.length} exe{p.exePaths.length === 1 ? '' : 's'}
                  </span>
                </span>
              </button>
            </li>
          ))}
        </ul>
        <div className={styles.wrap}>
          <button onClick={() => void createProfile()}>New</button>
          <button disabled={!saved} onClick={() => void (saved && createProfile(saved))}>
            Duplicate
          </button>
          <label className={styles.button}>
            Import…
            <input
              type="file"
              accept=".json,application/json"
              hidden
              onChange={(e) => {
                const input = e.currentTarget;
                const file = input.files?.[0];
                if (!file) return;
                void guard(async () => {
                  await importProfile(await file.text());
                }).finally(() => {
                  input.value = '';
                });
              }}
            />
          </label>
        </div>
        <div className={styles.wrap}>
          <button onClick={() => void activate(null)}>
            Back to automatic
          </button>
        </div>
      </section>

      {draft && (
        <section className={cx(styles.card, styles.grow)}>
          <h2>Edit profile</h2>
          <label className={styles.field}>
            <span className={styles.fieldLabel}>Name</span>
            <input
              value={draft.name}
              onChange={(e) => {
                editProfile({ name: e.target.value });
              }}
            />
          </label>
          <div className={styles.wrap}>
            <button disabled={draft.isDefault} onClick={() => void setDefault(draft.id)}>
              Set default
            </button>
            <button onClick={() => void activate(draft.id)}>Activate manually</button>
            <button
              onClick={() =>
                void guard(async () => {
                  download(draft.name, await api.exportProfile(draft.id));
                })
              }
            >
              Export
            </button>
            <button
              disabled={draft.isDefault}
              onClick={() => {
                if (window.confirm(`Delete profile "${draft.name}"?`)) void deleteProfile(draft.id);
              }}
            >
              Delete
            </button>
          </div>

          <h3>Executables</h3>
          {draft.isDefault && <p className={styles.muted}>The default profile applies when no executable matches.</p>}
          <ul className={styles.list}>
            {draft.exePaths.map((path) => (
              <li key={path} className={styles.row}>
                <span className={cx(styles.mono, styles.grow)}>{path}</span>
                <button
                  onClick={() => {
                    editProfile({ exePaths: draft.exePaths.filter((x) => x !== path) });
                  }}
                >
                  Remove
                </button>
              </li>
            ))}
          </ul>
          <form
            className={styles.row}
            onSubmit={(e) => {
              e.preventDefault();
              addPath(newPath);
              setNewPath('');
            }}
          >
            <input
              className={styles.grow}
              placeholder="C:\Games\game.exe or game.exe"
              value={newPath}
              onChange={(e) => {
                setNewPath(e.target.value);
              }}
            />
            <button type="submit">Add path</button>
            <button
              type="button"
              onClick={() => {
                setPicking(true);
              }}
            >
              Add running app…
            </button>
          </form>
        </section>
      )}

      {picking && (
        <RunningAppsModal
          onPick={(path) => {
            addPath(path);
            setPicking(false);
          }}
          onClose={() => {
            setPicking(false);
          }}
        />
      )}
    </div>
  );
}
