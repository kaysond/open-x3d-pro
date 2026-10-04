//! Shared backend state and the IPC payload types of CONTRACT §5.

use crate::calibration::Calibrator;
use crate::profiles::ProfileStore;
use crate::switcher::{ProfileSwitched, Switcher};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use x3d_core::config::Settings;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppState {
    pub device_connected: bool,
    pub driver_installed: bool,
    pub driver_version: Option<String>,
    pub active_profile_id: Option<String>,
    pub matched_exe: Option<String>,
    pub settings: Settings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowInfo {
    pub pid: u32,
    pub exe_path: String,
    pub title: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeviceStatus {
    pub connected: bool,
    pub driver_installed: bool,
    pub driver_version: Option<String>,
}

pub struct Inner {
    pub store: ProfileStore,
    pub settings: Settings,
    pub settings_path: PathBuf,
    pub switcher: Switcher,
    pub device: DeviceStatus,
    pub calibration: Calibrator,
}

impl Inner {
    pub fn new(store: ProfileStore, settings: Settings, settings_path: PathBuf) -> Self {
        Inner {
            store,
            settings,
            settings_path,
            switcher: Switcher::default(),
            device: DeviceStatus::default(),
            calibration: Calibrator::default(),
        }
    }

    /// Re-runs profile selection; `Some` when the active profile changed.
    pub fn reevaluate(&mut self) -> Option<ProfileSwitched> {
        self.switcher.evaluate(self.store.list(), &self.settings)
    }

    pub fn app_state(&self) -> AppState {
        let active = self.switcher.active();
        AppState {
            device_connected: self.device.connected,
            driver_installed: self.device.driver_installed,
            driver_version: self.device.driver_version.clone(),
            active_profile_id: active.map(|a| a.profile_id.clone()),
            matched_exe: active.and_then(|a| a.exe_path.clone()),
            settings: self.settings.clone(),
        }
    }

    pub fn active_profile_name(&self) -> String {
        self.switcher
            .active()
            .and_then(|a| self.store.get(&a.profile_id))
            .map_or_else(|| "none".to_string(), |p| p.name.clone())
    }
}

/// The one lock all threads share; held only for short, non-blocking sections.
#[derive(Clone)]
pub struct Shared(Arc<Mutex<Inner>>);

impl Shared {
    pub fn new(inner: Inner) -> Self {
        Shared(Arc::new(Mutex::new(inner)))
    }

    pub fn lock(&self) -> MutexGuard<'_, Inner> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
