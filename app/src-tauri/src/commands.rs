//! Tauri commands (CONTRACT §5). Thin: validate, mutate shared state, notify.

use crate::state::{AppState, Shared, WindowInfo};
use tauri::{AppHandle, State};
use x3d_core::config::{AxisId, Calibration, DeviceConfig, Profile, Settings};

/// Runs `f` under the lock, re-evaluates the active profile, then emits events.
fn mutate<T>(
    app: &AppHandle,
    shared: &Shared,
    f: impl FnOnce(&mut crate::state::Inner) -> Result<T, String>,
) -> Result<T, String> {
    let (value, switched) = {
        let mut s = shared.lock();
        let value = f(&mut s)?;
        (value, s.reevaluate())
    };
    crate::notify(app, switched);
    Ok(value)
}

#[tauri::command]
pub fn get_state(shared: State<'_, Shared>) -> Result<AppState, String> {
    Ok(shared.lock().app_state())
}

#[tauri::command]
pub fn list_profiles(shared: State<'_, Shared>) -> Result<Vec<Profile>, String> {
    Ok(shared.lock().store.list().to_vec())
}

#[tauri::command]
pub fn save_profile(
    app: AppHandle,
    shared: State<'_, Shared>,
    profile: Profile,
) -> Result<Profile, String> {
    mutate(&app, &shared, |s| {
        let saved = s.store.save(profile).map_err(|e| e.to_string())?;
        s.switcher.clear_preview();
        Ok(saved)
    })
}

#[tauri::command]
pub fn delete_profile(app: AppHandle, shared: State<'_, Shared>, id: String) -> Result<(), String> {
    mutate(&app, &shared, |s| {
        s.store.delete(&id).map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub fn set_default_profile(
    app: AppHandle,
    shared: State<'_, Shared>,
    id: String,
) -> Result<(), String> {
    mutate(&app, &shared, |s| {
        s.store.set_default(&id).map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub fn activate_profile(
    app: AppHandle,
    shared: State<'_, Shared>,
    id: Option<String>,
) -> Result<(), String> {
    mutate(&app, &shared, |s| {
        if let Some(id) = &id {
            if s.store.get(id).is_none() {
                return Err(format!("no profile with id {id}"));
            }
        }
        s.switcher.set_manual(id);
        Ok(())
    })
}

#[tauri::command]
pub fn preview_config(
    app: AppHandle,
    shared: State<'_, Shared>,
    config: DeviceConfig,
) -> Result<(), String> {
    config.validate().map_err(|e| e.to_string())?;
    mutate(&app, &shared, |s| {
        s.switcher.set_preview(config);
        Ok(())
    })
}

#[tauri::command]
pub fn list_windows() -> Result<Vec<WindowInfo>, String> {
    Ok(crate::watcher::list_windows())
}

#[tauri::command]
pub fn get_settings(shared: State<'_, Shared>) -> Result<Settings, String> {
    Ok(shared.lock().settings.clone())
}

#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    shared: State<'_, Shared>,
    settings: Settings,
) -> Result<Settings, String> {
    let autostart_changed = shared.lock().settings.autostart != settings.autostart;
    if autostart_changed {
        crate::apply_autostart(&app, settings.autostart)?;
    }
    mutate(&app, &shared, |s| {
        crate::settings::save(&s.settings_path, &settings)
            .map_err(|e| format!("cannot save settings: {e}"))?;
        s.settings = settings.clone();
        Ok(settings)
    })
}

#[tauri::command]
pub fn start_calibration(shared: State<'_, Shared>, axis: AxisId) -> Result<(), String> {
    shared.lock().calibration.start(axis);
    Ok(())
}

#[tauri::command]
pub fn finish_calibration(shared: State<'_, Shared>, axis: AxisId) -> Result<Calibration, String> {
    shared.lock().calibration.finish(axis).map_err(String::from)
}

#[tauri::command]
pub fn export_profile(shared: State<'_, Shared>, id: String) -> Result<String, String> {
    shared.lock().store.export(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn import_profile(
    app: AppHandle,
    shared: State<'_, Shared>,
    json: String,
) -> Result<Profile, String> {
    mutate(&app, &shared, |s| {
        s.store.import(&json).map_err(|e| e.to_string())
    })
}
