//! Tray icon: Open, active profile (disabled), Pause switching, Quit.

use std::sync::Mutex;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Wry};

const TRAY_ID: &str = "main";

/// Last (profile name, paused) shown, to avoid rebuilding the menu on every state event.
static SHOWN: Mutex<Option<(String, bool)>> = Mutex::new(None);

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu(app, "none", false)?)
        .tooltip("Open X3D Pro")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => crate::show_main(app),
            "pause" => crate::toggle_pause(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                crate::show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

pub fn update(app: &AppHandle, profile_name: &str, paused: bool) {
    let next = (profile_name.to_string(), paused);
    {
        let mut shown = SHOWN.lock().unwrap_or_else(|e| e.into_inner());
        if shown.as_ref() == Some(&next) {
            return;
        }
        *shown = Some(next);
    }
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let result = tray
        .set_tooltip(Some(format!("Open X3D Pro: {profile_name}")))
        .and_then(|()| tray.set_menu(Some(menu(app, profile_name, paused)?)));
    if let Err(e) = result {
        log::warn!("tray update failed: {e}");
    }
}

fn menu(app: &AppHandle, profile_name: &str, paused: bool) -> tauri::Result<Menu<Wry>> {
    // A single '&' would be taken as a mnemonic marker.
    let label = format!("Profile: {}", profile_name.replace('&', "&&"));
    Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "open", "Open", true, None::<&str>)?,
            &MenuItem::with_id(app, "active", label, false, None::<&str>)?,
            &CheckMenuItem::with_id(app, "pause", "Pause switching", true, paused, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?,
        ],
    )
}
