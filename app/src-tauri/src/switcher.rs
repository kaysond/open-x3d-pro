//! Chooses the active profile (manual > foreground/running match > default) and owns the
//! config the device should carry.

use serde::Serialize;
use x3d_core::blob::{blob_crc, CompiledConfig, BLOB_LEN};
use x3d_core::config::{DeviceConfig, KeyBinding, MatchMode, Profile, Settings};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Reason {
    Foreground,
    Running,
    Manual,
    Default,
}

/// Payload of the `profile_switched` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSwitched {
    pub profile_id: String,
    pub exe_path: Option<String>,
    pub reason: Reason,
}

pub struct Switcher {
    manual: Option<String>,
    preview: Option<DeviceConfig>,
    foreground: Option<String>,
    running: Vec<String>,
    active: Option<ProfileSwitched>,
    config: DeviceConfig,
    compiled: CompiledConfig,
    blob: [u8; BLOB_LEN],
    pushed_crc: Option<u32>,
}

impl Default for Switcher {
    fn default() -> Self {
        let config = DeviceConfig::default();
        let compiled = CompiledConfig::from_config(&config);
        Switcher {
            manual: None,
            preview: None,
            foreground: None,
            running: Vec::new(),
            active: None,
            blob: compiled.to_bytes(),
            config,
            compiled,
            pushed_crc: None,
        }
    }
}

impl Switcher {
    pub fn active(&self) -> Option<&ProfileSwitched> {
        self.active.as_ref()
    }

    /// The effective config (preview or active profile) for in-app processing.
    pub fn compiled(&self) -> &CompiledConfig {
        &self.compiled
    }

    pub fn key_bindings(&self) -> &[KeyBinding] {
        &self.config.key_bindings
    }

    /// Manual override (`None` = automatic); ends any preview.
    pub fn set_manual(&mut self, id: Option<String>) {
        self.manual = id;
        self.preview = None;
    }

    /// Live-edit config; holds the active profile until cleared.
    pub fn set_preview(&mut self, config: DeviceConfig) {
        self.preview = Some(config);
    }

    pub fn clear_preview(&mut self) {
        self.preview = None;
    }

    pub fn set_foreground(&mut self, exe: Option<String>) {
        self.foreground = exe;
    }

    pub fn set_running(&mut self, exes: Vec<String>) {
        self.running = exes;
    }

    /// Recomputes the active profile and effective config; `Some` when the active profile changed.
    pub fn evaluate(
        &mut self,
        profiles: &[Profile],
        settings: &Settings,
    ) -> Option<ProfileSwitched> {
        let find = |id: &str| profiles.iter().find(|p| p.id == id);
        let hold = settings.switching_paused || self.preview.is_some();
        let kept = self
            .active
            .as_ref()
            .filter(|_| hold)
            .and_then(|a| Some((find(&a.profile_id)?, a.clone())));
        let (profile, next) = if let Some(p) = self.manual.as_deref().and_then(find) {
            (p, switched(p, None, Reason::Manual))
        } else if let Some(k) = kept {
            k
        } else if let Some((p, exe, reason)) = self.auto_match(profiles, settings.match_mode, hold)
        {
            (p, switched(p, Some(exe), reason))
        } else {
            let p = profiles.iter().find(|p| p.is_default)?;
            (p, switched(p, None, Reason::Default))
        };

        let config = self.preview.as_ref().unwrap_or(&profile.config);
        if *config != self.config {
            self.config = config.clone();
            self.compiled = CompiledConfig::from_config(&self.config);
            self.blob = self.compiled.to_bytes();
        }
        let changed = self.active.as_ref() != Some(&next);
        self.active = Some(next);
        changed.then(|| self.active.clone()).flatten()
    }

    fn auto_match<'a>(
        &self,
        profiles: &'a [Profile],
        mode: MatchMode,
        hold: bool,
    ) -> Option<(&'a Profile, String, Reason)> {
        if hold {
            return None;
        }
        match mode {
            MatchMode::Foreground => {
                let exe = self.foreground.as_ref()?;
                match_profile(profiles, exe).map(|p| (p, exe.clone(), Reason::Foreground))
            }
            MatchMode::Running => {
                // Keep the current match while its process lives.
                let current = self
                    .active
                    .as_ref()
                    .filter(|a| a.reason == Reason::Running)
                    .and_then(|a| {
                        let exe = a.exe_path.as_ref().filter(|e| self.running.contains(e))?;
                        let p = match_profile(profiles, exe).filter(|p| p.id == a.profile_id)?;
                        Some((p, exe.clone(), Reason::Running))
                    });
                current.or_else(|| {
                    self.running.iter().find_map(|exe| {
                        match_profile(profiles, exe).map(|p| (p, exe.clone(), Reason::Running))
                    })
                })
            }
        }
    }

    /// The blob to send if it differs from what the device was last given.
    pub fn take_push(&mut self) -> Option<[u8; BLOB_LEN]> {
        let crc = blob_crc(&self.blob);
        if self.pushed_crc == Some(crc) {
            return None;
        }
        self.pushed_crc = Some(crc);
        Some(self.blob)
    }

    /// Records the config CRC the driver reports after (re)connecting; `None` forces a push.
    pub fn set_device_crc(&mut self, crc: Option<u32>) {
        self.pushed_crc = crc;
    }
}

fn switched(p: &Profile, exe_path: Option<String>, reason: Reason) -> ProfileSwitched {
    ProfileSwitched {
        profile_id: p.id.clone(),
        exe_path,
        reason,
    }
}

/// Lowercase, backslash-separated path for case-insensitive comparison.
pub fn normalize(path: &str) -> String {
    path.trim().replace('/', "\\").to_lowercase()
}

/// Last component of a [`normalize`]d path.
pub fn basename(path: &str) -> &str {
    path.rsplit('\\').next().unwrap_or(path)
}

/// Full-path match across all profiles first, then basename match.
pub fn match_profile<'a>(profiles: &'a [Profile], exe: &str) -> Option<&'a Profile> {
    let exe = normalize(exe);
    let base = basename(&exe);
    let any = |f: &dyn Fn(&str) -> bool| {
        profiles
            .iter()
            .find(|p| p.exe_paths.iter().any(|e| f(&normalize(e))))
    };
    any(&|e| e == exe).or_else(|| any(&|e| basename(e) == base))
}

#[cfg(test)]
mod tests {
    use super::*;
    use x3d_core::config::Curve;

    fn profile(id: &str, exes: &[&str], is_default: bool) -> Profile {
        let mut config = DeviceConfig::default();
        config.axes.x.sensitivity = 1.0 + id.len() as f64 / 10.0;
        Profile {
            id: id.into(),
            name: id.into(),
            exe_paths: exes.iter().map(|s| s.to_string()).collect(),
            is_default,
            config,
        }
    }

    fn profiles() -> Vec<Profile> {
        vec![
            profile("def", &[], true),
            profile("bare", &["game.exe"], false),
            profile("full", &["D:/Games/Other/GAME.exe"], false),
            profile("sim", &["C:\\Sims\\dcs.exe"], false),
        ]
    }

    #[test]
    fn matcher_rules() {
        let ps = profiles();
        let id = |exe: &str| match_profile(&ps, exe).map(|p| p.id.as_str());
        // Full path beats an earlier basename-only match, case- and slash-insensitively.
        assert_eq!(id("d:\\games\\other\\game.EXE"), Some("full"));
        assert_eq!(id("C:\\Elsewhere\\Game.exe"), Some("bare"));
        // Basename fallback also applies to full-path entries.
        assert_eq!(id("E:\\moved\\DCS.exe"), Some("sim"));
        assert_eq!(id("C:\\Windows\\explorer.exe"), None);
    }

    #[test]
    fn foreground_switching_and_default_fallback() {
        let ps = profiles();
        let settings = Settings::default();
        let mut s = Switcher::default();
        let ev = s.evaluate(&ps, &settings).unwrap();
        assert_eq!(
            (ev.profile_id.as_str(), ev.reason),
            ("def", Reason::Default)
        );
        assert_eq!(s.evaluate(&ps, &settings), None);

        s.set_foreground(Some("C:\\Sims\\DCS.exe".into()));
        let ev = s.evaluate(&ps, &settings).unwrap();
        assert_eq!(ev.profile_id, "sim");
        assert_eq!(ev.reason, Reason::Foreground);
        assert_eq!(ev.exe_path.as_deref(), Some("C:\\Sims\\DCS.exe"));

        s.set_foreground(Some("C:\\Windows\\explorer.exe".into()));
        assert_eq!(s.evaluate(&ps, &settings).unwrap().reason, Reason::Default);
    }

    #[test]
    fn manual_pause_and_preview_hold() {
        let ps = profiles();
        let mut settings = Settings::default();
        let mut s = Switcher::default();
        s.set_manual(Some("bare".into()));
        s.set_foreground(Some("C:\\Sims\\dcs.exe".into()));
        assert_eq!(s.evaluate(&ps, &settings).unwrap().reason, Reason::Manual);
        s.set_manual(None);
        assert_eq!(s.evaluate(&ps, &settings).unwrap().profile_id, "sim");

        settings.switching_paused = true;
        s.set_foreground(None);
        assert_eq!(s.evaluate(&ps, &settings), None);
        assert_eq!(s.active().unwrap().profile_id, "sim");
        settings.switching_paused = false;

        let mut preview = DeviceConfig::default();
        preview.axes.y.curve = Curve::Exponent { exponent: 3.0 };
        s.set_preview(preview.clone());
        assert_eq!(s.evaluate(&ps, &settings), None);
        assert_eq!(s.compiled(), &CompiledConfig::from_config(&preview));
        s.clear_preview();
        assert_eq!(s.evaluate(&ps, &settings).unwrap().reason, Reason::Default);
    }

    #[test]
    fn running_mode_keeps_match_while_alive() {
        let ps = profiles();
        let settings = Settings {
            match_mode: MatchMode::Running,
            ..Settings::default()
        };
        let mut s = Switcher::default();
        s.set_running(vec!["c:\\x\\other.exe".into(), "C:\\Sims\\dcs.exe".into()]);
        assert_eq!(s.evaluate(&ps, &settings).unwrap().profile_id, "sim");
        s.set_running(vec!["C:\\a\\game.exe".into(), "C:\\Sims\\dcs.exe".into()]);
        assert_eq!(s.evaluate(&ps, &settings), None);
        s.set_running(vec!["C:\\a\\game.exe".into()]);
        assert_eq!(s.evaluate(&ps, &settings).unwrap().profile_id, "bare");
        s.set_running(vec![]);
        assert_eq!(s.evaluate(&ps, &settings).unwrap().reason, Reason::Default);
    }

    #[test]
    fn push_only_when_crc_changes() {
        let mut ps = profiles();
        ps[2].config = ps[1].config.clone();
        let settings = Settings::default();
        let mut s = Switcher::default();
        s.evaluate(&ps, &settings);
        assert!(s.take_push().is_some());
        assert!(s.take_push().is_none());
        // Different profile, identical config: switch event but no push.
        s.set_foreground(Some("x:\\game.exe".into()));
        assert_eq!(s.evaluate(&ps, &settings).unwrap().profile_id, "bare");
        assert!(s.take_push().is_some());
        s.set_foreground(Some("D:\\Games\\Other\\game.exe".into()));
        assert_eq!(s.evaluate(&ps, &settings).unwrap().profile_id, "full");
        assert!(s.take_push().is_none());
        // Driver reconnects with another config loaded.
        s.set_device_crc(Some(0));
        assert!(s.take_push().is_some());
    }
}
