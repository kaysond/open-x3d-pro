//! Open X3D Pro tray agent. Platform-neutral logic lives in the public modules and is tested
//! everywhere; the Tauri shell, HID access and Win32 hooks are Windows-only.

pub mod calibration;
pub mod device;
pub mod keys;
pub mod profiles;
pub mod settings;
pub mod state;
pub mod switcher;

#[cfg(windows)]
mod commands;
#[cfg(windows)]
mod tray;
#[cfg(windows)]
mod watcher;

/// Passed by autostart; the app then starts in the tray without opening the window.
pub const MINIMIZED_ARG: &str = "--minimized";

#[cfg(windows)]
pub use app::run;

#[cfg(windows)]
mod app {
    use crate::profiles::ProfileStore;
    use crate::state::{Inner, Shared};
    use crate::switcher::ProfileSwitched;
    use crate::{commands, device, settings, tray, watcher, MINIMIZED_ARG};
    use tauri::{AppHandle, Emitter, Manager, WindowEvent};

    pub fn run() {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
        let result = tauri::Builder::default()
            // Must be first so a second launch exits before touching anything.
            .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
                show_main(app)
            }))
            .plugin(
                tauri_plugin_autostart::Builder::new()
                    .arg(MINIMIZED_ARG)
                    .build(),
            )
            .invoke_handler(tauri::generate_handler![
                commands::get_state,
                commands::list_profiles,
                commands::save_profile,
                commands::delete_profile,
                commands::set_default_profile,
                commands::activate_profile,
                commands::preview_config,
                commands::list_windows,
                commands::get_settings,
                commands::save_settings,
                commands::start_calibration,
                commands::finish_calibration,
                commands::export_profile,
                commands::import_profile,
            ])
            .on_window_event(|window, event| {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            })
            .setup(|app| setup(app.handle()))
            .run(tauri::generate_context!());
        if let Err(e) = result {
            log::error!("tauri: {e}");
        }
    }

    fn setup(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
        let dir = app.path().data_dir()?.join("OpenX3DPro");
        let settings_path = dir.join("settings.json");
        let (settings, first_run) = settings::load(&settings_path);
        let store = ProfileStore::load(dir.join("profiles"))?;
        if first_run {
            if let Err(e) = settings::save(&settings_path, &settings) {
                log::warn!("cannot write {}: {e}", settings_path.display());
            }
            // Dev builds would register target/debug in HKCU\...\Run.
            if !cfg!(debug_assertions) {
                if let Err(e) = apply_autostart(app, settings.autostart) {
                    log::warn!("{e}");
                }
            }
        }
        app.manage(Shared::new(Inner::new(store, settings, settings_path)));
        tray::create(app)?;
        let switched = app.state::<Shared>().lock().reevaluate();
        notify(app, switched);
        device::spawn(app);
        watcher::start(app);
        if !std::env::args().any(|a| a == MINIMIZED_ARG) {
            show_main(app);
        }
        Ok(())
    }

    /// Emits `profile_switched` (if any) and `state`, and refreshes the tray.
    pub(crate) fn notify(app: &AppHandle, switched: Option<ProfileSwitched>) {
        let (state, name) = {
            let shared = app.state::<Shared>();
            let s = shared.lock();
            (s.app_state(), s.active_profile_name())
        };
        if let Some(ev) = switched {
            log::info!("profile {name:?} ({:?})", ev.reason);
            if let Err(e) = app.emit("profile_switched", ev) {
                log::debug!("emit failed: {e}");
            }
        }
        tray::update(app, &name, state.settings.switching_paused);
        if let Err(e) = app.emit("state", state) {
            log::debug!("emit failed: {e}");
        }
    }

    pub(crate) fn show_main(app: &AppHandle) {
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.unminimize();
            let _ = w.show();
            let _ = w.set_focus();
        }
    }

    pub(crate) fn toggle_pause(app: &AppHandle) {
        let switched = {
            let shared = app.state::<Shared>();
            let mut s = shared.lock();
            s.settings.switching_paused = !s.settings.switching_paused;
            if let Err(e) = settings::save(&s.settings_path, &s.settings) {
                log::warn!("cannot save settings: {e}");
            }
            s.reevaluate()
        };
        notify(app, switched);
    }

    pub(crate) fn apply_autostart(app: &AppHandle, enabled: bool) -> Result<(), String> {
        use tauri_plugin_autostart::ManagerExt;
        let launcher = app.autolaunch();
        if launcher.is_enabled().ok() == Some(enabled) {
            return Ok(());
        }
        let result = if enabled {
            launcher.enable()
        } else {
            launcher.disable()
        };
        result.map_err(|e| format!("cannot change autostart: {e}"))
    }
}

#[cfg(windows)]
pub(crate) use app::{apply_autostart, notify, show_main, toggle_pause};
