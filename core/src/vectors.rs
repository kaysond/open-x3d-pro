//! Test vectors shared with the driver's host test (CONTRACT §4.3).

use crate::blob::{compile, CompiledConfig};
use crate::config::{AxisId, Calibration, Curve, DeviceConfig, HatMode};
use crate::pipeline::{process, RawReport};
use serde::Serialize;
use std::error::Error;

/// Path of the committed file, relative to the crate root.
pub const PATH: &str = "testdata/vectors.json";

#[derive(Serialize)]
struct Vector {
    name: &'static str,
    blob: String,
    cases: Vec<Case>,
}

#[derive(Serialize)]
struct Case {
    raw: String,
    x: u16,
    y: u16,
    rz: u16,
    slider: u16,
    hat: u8,
    buttons: u32,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn frame(x: u16, y: u16, hat: u8, rz: u8, slider: u8, buttons: u16) -> [u8; 7] {
    RawReport {
        x,
        y,
        rz,
        slider,
        hat,
        buttons,
    }
    .to_bytes()
}

fn raw_cases() -> Vec<[u8; 7]> {
    let mut v = vec![
        [0xfc, 0xf1, 0x87, 0x80, 0x00, 0x99, 0x00], // resting stick as captured
        frame(512, 512, 8, 128, 0, 0),              // exact centre
        frame(0, 0, 0, 0, 0, 0x001),                // all minimums
        frame(1023, 1023, 4, 255, 255, 0xFFF),      // all maximums, every button
        frame(256, 768, 2, 64, 192, 0x0A5),
        frame(530, 495, 6, 131, 10, 0x800), // near centre, shift-only
        frame(560, 470, 8, 140, 20, 0x801), // just past small deadzones, shift + 1
        frame(900, 120, 1, 220, 64, 0x5A0),
        frame(700, 300, 7, 30, 128, 0x003),
        frame(100, 950, 8, 250, 240, 0x803),
        frame(1000, 20, 8, 5, 250, 0x012),
    ];
    v.extend((0..=8).map(|hat| frame(512, 512, hat, 128, 0, 0)));
    v
}

fn scenarios() -> Vec<(&'static str, DeviceConfig)> {
    let base = DeviceConfig::default;
    let with = |f: &dyn Fn(&mut DeviceConfig)| {
        let mut c = base();
        f(&mut c);
        c
    };
    let all = |c: &mut DeviceConfig, f: &dyn Fn(&mut crate::config::AxisConfig)| {
        for a in [
            &mut c.axes.x,
            &mut c.axes.y,
            &mut c.axes.rz,
            &mut c.axes.slider,
        ] {
            f(a);
        }
    };
    let five = Curve::Points {
        points: vec![[0.0, 0.0], [0.2, 0.05], [0.5, 0.3], [0.8, 0.7], [1.0, 1.0]],
    };
    vec![
        ("identity", base()),
        (
            "calibration",
            with(&|c| {
                c.axes.x.calibration = Calibration {
                    min: 100 << 6,
                    center: 500 << 6,
                    max: 900 << 6,
                };
                c.axes.rz.calibration = Calibration {
                    min: 20 << 8,
                    center: 120 << 8,
                    max: 235 << 8,
                };
                c.axes.slider.calibration = Calibration {
                    min: 16 << 8,
                    center: 16 << 8,
                    max: 240 << 8,
                };
            }),
        ),
        ("inversion", with(&|c| all(c, &|a| a.invert = true))),
        (
            "deadzone_center",
            with(&|c| {
                c.axes.x.deadzone.center = 0.1;
                c.axes.y.deadzone.center = 0.25;
                c.axes.rz.deadzone.center = 0.05;
            }),
        ),
        (
            "deadzone_min",
            with(&|c| {
                c.axes.x.deadzone.min = 0.2;
                c.axes.rz.deadzone.min = 0.5;
                c.axes.slider.deadzone.min = 0.15;
            }),
        ),
        (
            "deadzone_max",
            with(&|c| {
                c.axes.y.deadzone.max = 0.2;
                c.axes.rz.deadzone.max = 0.333;
                c.axes.slider.deadzone.max = 0.15;
            }),
        ),
        (
            "exponent_0_5",
            with(&|c| all(c, &|a| a.curve = Curve::Exponent { exponent: 0.5 })),
        ),
        (
            "exponent_2_0",
            with(&|c| all(c, &|a| a.curve = Curve::Exponent { exponent: 2.0 })),
        ),
        ("points_5", with(&|c| all(c, &|a| a.curve = five.clone()))),
        (
            "sensitivity_2_0_clamp",
            with(&|c| all(c, &|a| a.sensitivity = 2.0)),
        ),
        (
            "swapped_targets",
            with(&|c| {
                c.axes.x.target = Some(AxisId::Y);
                c.axes.y.target = Some(AxisId::X);
                c.axes.rz.target = Some(AxisId::Slider);
                c.axes.slider.target = Some(AxisId::Rz);
            }),
        ),
        (
            "target_collision",
            with(&|c| {
                c.axes.rz.target = Some(AxisId::X);
                c.axes.slider.target = Some(AxisId::X);
            }),
        ),
        (
            "disabled_axis",
            with(&|c| {
                c.axes.y.target = None;
                c.axes.slider.target = None;
            }),
        ),
        ("hat_buttons", with(&|c| c.hat_mode = HatMode::Buttons)),
        ("hat_both", with(&|c| c.hat_mode = HatMode::Both)),
        (
            "shift_layer",
            with(&|c| {
                c.shift_button = Some(12);
                c.buttons_shifted[0].output = Some(20);
                c.buttons_shifted[1].output = Some(21);
                c.buttons_shifted[4].output = Some(1);
                c.buttons_shifted[11].output = Some(32);
            }),
        ),
        (
            "button_drop",
            with(&|c| {
                c.buttons[1].output = None;
                c.buttons[4].output = None;
                c.buttons[2].output = Some(1);
                c.buttons[11].output = Some(17);
            }),
        ),
        (
            "mixed",
            with(&|c| {
                c.axes.x.invert = true;
                c.axes.x.deadzone.center = 0.08;
                c.axes.x.deadzone.max = 0.1;
                c.axes.x.curve = Curve::Exponent { exponent: 1.7 };
                c.axes.x.sensitivity = 1.3;
                c.axes.y.curve = five.clone();
                c.axes.y.deadzone.min = 0.05;
                c.axes.y.sensitivity = 0.6;
                c.axes.rz.target = Some(AxisId::Slider);
                c.axes.slider.target = None;
                c.axes.slider.invert = true;
                c.hat_mode = HatMode::Both;
                c.shift_button = Some(3);
                c.buttons_shifted[0].output = Some(25);
            }),
        ),
    ]
}

/// The committed `vectors.json` content.
pub fn generate() -> Result<String, Box<dyn Error>> {
    let raws = raw_cases();
    let mut out = Vec::new();
    for (name, cfg) in scenarios() {
        cfg.validate().map_err(|e| format!("{name}: {e}"))?;
        let compiled = CompiledConfig::from_config(&cfg);
        let cases = raws
            .iter()
            .map(|raw| {
                let r = process(&compiled, raw);
                Case {
                    raw: hex(raw),
                    x: r.x,
                    y: r.y,
                    rz: r.rz,
                    slider: r.slider,
                    hat: r.hat,
                    buttons: r.buttons,
                }
            })
            .collect();
        out.push(Vector {
            name,
            blob: hex(&compile(&cfg)),
            cases,
        });
    }
    Ok(serde_json::to_string_pretty(&out)? + "\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committed_vectors_are_fresh() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(PATH);
        // A Windows checkout with autocrlf may have rewritten line endings.
        let committed = std::fs::read_to_string(&path)
            .unwrap_or_default()
            .replace("\r\n", "\n");
        assert!(
            committed == generate().unwrap(),
            "{} is stale; run `cargo run -p x3d-core --bin gen-vectors`",
            path.display()
        );
    }

    #[test]
    fn every_scenario_has_cases_and_valid_blob() {
        let v: serde_json::Value = serde_json::from_str(&generate().unwrap()).unwrap();
        let v = v.as_array().unwrap();
        assert!(v.len() >= 15);
        for s in v {
            assert_eq!(s["blob"].as_str().unwrap().len(), 480);
            assert!(s["cases"].as_array().unwrap().len() >= 3);
            let blob: Vec<u8> = (0..240)
                .map(|i| u8::from_str_radix(&s["blob"].as_str().unwrap()[2 * i..2 * i + 2], 16))
                .collect::<Result<_, _>>()
                .unwrap();
            crate::blob::parse(&blob).unwrap();
        }
    }
}
