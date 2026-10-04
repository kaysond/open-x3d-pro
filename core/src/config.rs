//! Profile and device configuration types (CONTRACT §3), shared with the frontend as JSON.

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AxisId {
    X,
    Y,
    Rz,
    Slider,
}

impl AxisId {
    /// Blob/pipeline order.
    pub const ALL: [AxisId; 4] = [AxisId::X, AxisId::Y, AxisId::Rz, AxisId::Slider];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn as_str(self) -> &'static str {
        match self {
            AxisId::X => "x",
            AxisId::Y => "y",
            AxisId::Rz => "rz",
            AxisId::Slider => "slider",
        }
    }
}

/// Raw units 0..65535 (10-bit axes `<< 6`, 8-bit axes `<< 8`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Calibration {
    pub min: u16,
    pub center: u16,
    pub max: u16,
}

/// Fractions 0..0.5: `center` of the half-range, `min`/`max` of the range ends.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Deadzone {
    pub center: f64,
    pub min: f64,
    pub max: f64,
}

impl Deadzone {
    /// `[center, min, max]` in permille, as stored in the blob.
    pub fn permille(&self) -> [u16; 3] {
        [self.center, self.min, self.max].map(|f| (f * 1000.0).round() as u16)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Curve {
    Linear,
    Exponent { exponent: f64 },
    Points { points: Vec<[f64; 2]> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CurveMode {
    Symmetric,
    Full,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AxisConfig {
    pub calibration: Calibration,
    pub invert: bool,
    pub deadzone: Deadzone,
    pub curve: Curve,
    pub sensitivity: f64,
    pub curve_mode: CurveMode,
    pub target: Option<AxisId>,
}

impl AxisConfig {
    pub fn identity(axis: AxisId) -> Self {
        let full = axis == AxisId::Slider;
        AxisConfig {
            calibration: Calibration {
                min: 0,
                center: if full { 0 } else { 32768 },
                max: 65535,
            },
            invert: false,
            deadzone: Deadzone::default(),
            curve: Curve::Linear,
            sensitivity: 1.0,
            curve_mode: if full {
                CurveMode::Full
            } else {
                CurveMode::Symmetric
            },
            target: Some(axis),
        }
    }

    fn validate(&self) -> Result<(), String> {
        let Calibration { min, center, max } = self.calibration;
        match self.curve_mode {
            CurveMode::Symmetric if !(min < center && center < max) => {
                return Err("calibration needs min < center < max".into())
            }
            CurveMode::Full if min >= max => return Err("calibration needs min < max".into()),
            _ => {}
        }
        let d = self.deadzone;
        if ![d.center, d.min, d.max]
            .iter()
            .all(|v| (0.0..=0.5).contains(v))
        {
            return Err("deadzones must be within 0..0.5".into());
        }
        // Pairs that share a rescale denominator must leave a live range (no division by zero).
        let [c, lo, hi] = d.permille();
        if c + lo >= 1000 || c + hi >= 1000 || lo + hi >= 1000 {
            return Err("deadzones leave no live range".into());
        }
        if !(0.1..=4.0).contains(&self.sensitivity) {
            return Err("sensitivity must be within 0.1..4.0".into());
        }
        match &self.curve {
            Curve::Linear => {}
            Curve::Exponent { exponent } => {
                if !(0.2..=5.0).contains(exponent) {
                    return Err("curve exponent must be within 0.2..5".into());
                }
            }
            Curve::Points { points } => {
                if !(2..=17).contains(&points.len()) {
                    return Err("curve needs 2..17 points".into());
                }
                if !points.iter().flatten().all(|v| (0.0..=1.0).contains(v)) {
                    return Err("curve points must be within 0..1".into());
                }
                if !points.windows(2).all(|w| w[0][0] < w[1][0]) {
                    return Err("curve point x values must be strictly increasing".into());
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Axes {
    pub x: AxisConfig,
    pub y: AxisConfig,
    pub rz: AxisConfig,
    pub slider: AxisConfig,
}

impl Axes {
    /// In [`AxisId::ALL`] order.
    pub fn all(&self) -> [&AxisConfig; 4] {
        [&self.x, &self.y, &self.rz, &self.slider]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HatMode {
    Hat,
    Buttons,
    Both,
}

/// Output button 1..32, `None` = dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ButtonBinding {
    pub output: Option<u8>,
}

/// Physical button 1..12 -> keyboard chord of W3C `KeyboardEvent.code` names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyBinding {
    pub button: u8,
    pub keys: Vec<String>,
}

pub const PHYSICAL_BUTTONS: usize = 12;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceConfig {
    pub axes: Axes,
    pub hat_mode: HatMode,
    /// Index = physical button - 1.
    pub buttons: [ButtonBinding; PHYSICAL_BUTTONS],
    pub shift_button: Option<u8>,
    pub buttons_shifted: [ButtonBinding; PHYSICAL_BUTTONS],
    pub key_bindings: Vec<KeyBinding>,
}

impl Default for DeviceConfig {
    fn default() -> Self {
        DeviceConfig {
            axes: Axes {
                x: AxisConfig::identity(AxisId::X),
                y: AxisConfig::identity(AxisId::Y),
                rz: AxisConfig::identity(AxisId::Rz),
                slider: AxisConfig::identity(AxisId::Slider),
            },
            hat_mode: HatMode::Hat,
            buttons: std::array::from_fn(|i| ButtonBinding {
                output: Some(i as u8 + 1),
            }),
            shift_button: None,
            buttons_shifted: [ButtonBinding::default(); PHYSICAL_BUTTONS],
            key_bindings: Vec::new(),
        }
    }
}

impl DeviceConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        for (id, axis) in AxisId::ALL.iter().zip(self.axes.all()) {
            axis.validate()
                .map_err(|e| ConfigError(format!("axis {}: {e}", id.as_str())))?;
        }
        let physical = 1..=PHYSICAL_BUTTONS as u8;
        for (i, b) in self.buttons.iter().chain(&self.buttons_shifted).enumerate() {
            if b.output.is_some_and(|o| !(1..=32).contains(&o)) {
                let layer = if i < PHYSICAL_BUTTONS { "" } else { "shifted " };
                return Err(ConfigError(format!(
                    "{layer}button {}: output must be 1..32",
                    i % PHYSICAL_BUTTONS + 1
                )));
            }
        }
        if self.shift_button.is_some_and(|b| !physical.contains(&b)) {
            return Err(ConfigError("shift button must be 1..12".into()));
        }
        for k in &self.key_bindings {
            if !physical.contains(&k.button) {
                return Err(ConfigError("key binding button must be 1..12".into()));
            }
            if k.keys.is_empty() {
                return Err(ConfigError(format!(
                    "key binding for button {} has no keys",
                    k.button
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub exe_paths: Vec<String>,
    pub is_default: bool,
    pub config: DeviceConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MatchMode {
    #[default]
    Foreground,
    Running,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub autostart: bool,
    pub switching_paused: bool,
    pub match_mode: MatchMode,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            autostart: true,
            switching_paused: false,
            match_mode: MatchMode::Foreground,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError(pub String);

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ConfigError {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn default_is_valid_identity() {
        let c = DeviceConfig::default();
        c.validate().unwrap();
        assert_eq!(c.axes.slider.calibration.center, 0);
        assert_eq!(c.axes.slider.curve_mode, CurveMode::Full);
        assert_eq!(c.axes.rz.target, Some(AxisId::Rz));
        assert_eq!(c.buttons[11].output, Some(12));
        assert!(c.buttons_shifted.iter().all(|b| b.output.is_none()));
    }

    #[test]
    fn json_shape_matches_contract() {
        let v = serde_json::to_value(DeviceConfig::default()).unwrap();
        assert_eq!(v["hatMode"], "hat");
        assert_eq!(v["axes"]["x"]["curveMode"], "symmetric");
        assert_eq!(v["axes"]["x"]["curve"], json!({ "type": "linear" }));
        assert_eq!(v["axes"]["slider"]["target"], "slider");
        assert_eq!(v["buttons"][0], json!({ "output": 1 }));
        assert_eq!(v["shiftButton"], json!(null));
        assert_eq!(v["buttonsShifted"].as_array().unwrap().len(), 12);
        let curve: Curve =
            serde_json::from_value(json!({ "type": "points", "points": [[0, 0], [1, 1]] }))
                .unwrap();
        assert_eq!(
            curve,
            Curve::Points {
                points: vec![[0.0, 0.0], [1.0, 1.0]]
            }
        );
        let s = serde_json::to_value(Settings::default()).unwrap();
        assert_eq!(
            s,
            json!({ "autostart": true, "switchingPaused": false, "matchMode": "foreground" })
        );
    }

    #[test]
    fn wrong_button_array_length_is_rejected() {
        let mut v = serde_json::to_value(DeviceConfig::default()).unwrap();
        v["buttons"].as_array_mut().unwrap().pop();
        assert!(serde_json::from_value::<DeviceConfig>(v).is_err());
    }

    fn invalid(f: impl FnOnce(&mut DeviceConfig)) -> String {
        let mut c = DeviceConfig::default();
        f(&mut c);
        c.validate().unwrap_err().0
    }

    #[test]
    fn validation_rejects_out_of_range() {
        invalid(|c| c.axes.x.calibration.center = 0);
        invalid(|c| c.axes.slider.calibration.max = 0);
        invalid(|c| c.axes.y.deadzone.center = 0.6);
        invalid(|c| {
            c.axes.y.deadzone.center = 0.5;
            c.axes.y.deadzone.max = 0.4996;
        });
        invalid(|c| c.axes.rz.sensitivity = 4.5);
        invalid(|c| c.axes.rz.sensitivity = f64::NAN);
        invalid(|c| c.axes.x.curve = Curve::Exponent { exponent: 0.1 });
        invalid(|c| {
            c.axes.x.curve = Curve::Points {
                points: vec![[0.0, 0.0]],
            }
        });
        invalid(|c| {
            c.axes.x.curve = Curve::Points {
                points: vec![[0.0, 0.0], [0.5, 0.2], [0.5, 1.0]],
            }
        });
        invalid(|c| {
            c.axes.x.curve = Curve::Points {
                points: vec![[0.0, 0.0], [1.0, 1.2]],
            }
        });
        invalid(|c| c.buttons[3].output = Some(33));
        assert!(invalid(|c| c.buttons_shifted[0].output = Some(0)).starts_with("shifted button 1"));
        invalid(|c| c.shift_button = Some(13));
        invalid(|c| {
            c.key_bindings.push(KeyBinding {
                button: 1,
                keys: vec![],
            })
        });
    }

    #[test]
    fn settings_missing_fields_default() {
        let s: Settings = serde_json::from_str(r#"{"matchMode":"running"}"#).unwrap();
        assert_eq!(s.match_mode, MatchMode::Running);
        assert!(s.autostart);
    }
}
