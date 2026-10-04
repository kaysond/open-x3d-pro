//! `%APPDATA%\OpenX3DPro\settings.json`.

use crate::profiles::write_atomic;
use std::io;
use std::path::Path;
use x3d_core::config::Settings;

/// Returns the settings and whether this is the first run (no file yet).
pub fn load(path: &Path) -> (Settings, bool) {
    match std::fs::read_to_string(path) {
        Ok(s) => {
            let settings = serde_json::from_str(&s).unwrap_or_else(|e| {
                log::warn!("ignoring invalid {}: {e}", path.display());
                Settings::default()
            });
            (settings, false)
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => (Settings::default(), true),
        Err(e) => {
            log::warn!("cannot read {}: {e}", path.display());
            (Settings::default(), false)
        }
    }
}

pub fn save(path: &Path, settings: &Settings) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    write_atomic(path, &serde_json::to_vec_pretty(settings)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiles::tests::temp_dir;
    use x3d_core::config::MatchMode;

    #[test]
    fn round_trip_and_first_run() {
        let dir = temp_dir("settings");
        let path = dir.join("sub").join("settings.json");
        assert_eq!(load(&path), (Settings::default(), true));
        let s = Settings {
            autostart: false,
            switching_paused: true,
            match_mode: MatchMode::Running,
        };
        save(&path, &s).unwrap();
        assert_eq!(load(&path), (s, false));
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(load(&path), (Settings::default(), false));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
