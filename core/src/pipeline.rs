//! Float reference implementation of the per-report pipeline (CONTRACT §4.1).
//! The driver's integer version must match it within ±1 LSB on axes, exactly on hat/buttons.

use crate::blob::{CompiledAxis, CompiledConfig, HAT_BUTTONS, HAT_HAT, MODE_FULL, TARGET_DISABLED};
use crate::config::Curve;
use serde::Serialize;

pub const HAT_CENTERED: u8 = 8;
const CENTER: u16 = 32768;

/// The device's 7-byte input report, in device units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct RawReport {
    /// 10-bit.
    pub x: u16,
    /// 10-bit.
    pub y: u16,
    pub rz: u8,
    pub slider: u8,
    /// 0..7 clockwise from north, 8 = centred.
    pub hat: u8,
    /// Physical buttons 1..12 in bits 0..11.
    pub buttons: u16,
}

impl RawReport {
    pub fn parse(b: &[u8; 7]) -> Self {
        RawReport {
            x: u16::from(b[0]) | (u16::from(b[1] & 0x03) << 8),
            y: u16::from(b[1] >> 2) | (u16::from(b[2] & 0x0F) << 6),
            hat: b[2] >> 4,
            rz: b[3],
            buttons: u16::from(b[4]) | (u16::from(b[6] & 0x0F) << 8),
            slider: b[5],
        }
    }

    /// Axes scaled to 16 bit (calibration units), in X, Y, Rz, Slider order.
    pub fn raw16(&self) -> [u16; 4] {
        [
            self.x << 6,
            self.y << 6,
            u16::from(self.rz) << 8,
            u16::from(self.slider) << 8,
        ]
    }

    pub fn to_bytes(&self) -> [u8; 7] {
        [
            self.x as u8,
            ((self.x >> 8) & 0x03) as u8 | ((self.y & 0x3F) << 2) as u8,
            ((self.y >> 6) & 0x0F) as u8 | (self.hat << 4),
            self.rz,
            self.buttons as u8,
            self.slider,
            ((self.buttons >> 8) & 0x0F) as u8,
        ]
    }
}

/// The joystick collection's report (ID 1) payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct JoyReport {
    pub x: u16,
    pub y: u16,
    pub rz: u16,
    pub slider: u16,
    pub hat: u8,
    pub buttons: u32,
}

/// 17-point LUT for inputs `i/16`, gain multiplied in.
pub fn build_lut(curve: &Curve, sensitivity: f64) -> [u16; 17] {
    std::array::from_fn(|i| {
        let t = i as f64 / 16.0;
        let y = match curve {
            Curve::Linear => t,
            Curve::Exponent { exponent } => t.powf(*exponent),
            Curve::Points { points } => interpolate(points, t),
        };
        to_u16(y * sensitivity * 65535.0)
    })
}

/// Piecewise-linear through `points`, flat beyond the first/last point.
fn interpolate(points: &[[f64; 2]], t: f64) -> f64 {
    let (Some(first), Some(last)) = (points.first(), points.last()) else {
        return t;
    };
    if t <= first[0] {
        return first[1];
    }
    for w in points.windows(2) {
        let ([x0, y0], [x1, y1]) = (w[0], w[1]);
        if t <= x1 {
            return y0 + (y1 - y0) * (t - x0) / (x1 - x0);
        }
    }
    last[1]
}

fn to_u16(v: f64) -> u16 {
    v.round().clamp(0.0, 65535.0) as u16
}

fn lookup(lut: &[u16; 17], t: f64) -> f64 {
    let pos = t * 16.0;
    let i = (pos as usize).min(15);
    let (a, b) = (f64::from(lut[i]), f64::from(lut[i + 1]));
    a + (b - a) * (pos - i as f64)
}

fn process_axis(a: &CompiledAxis, raw16: u16) -> u16 {
    let (min, center, max) = (
        f64::from(a.cal_min),
        f64::from(a.cal_center),
        f64::from(a.cal_max),
    );
    let raw = f64::from(raw16);
    let dz = |p: u16| f64::from(p) / 1000.0;
    if a.mode == MODE_FULL {
        let mut v = ((raw - min) / (max - min)).clamp(0.0, 1.0);
        if a.invert != 0 {
            v = 1.0 - v;
        }
        let v = ((v - dz(a.dz_min)) / (1.0 - dz(a.dz_min) - dz(a.dz_max))).clamp(0.0, 1.0);
        return to_u16(lookup(&a.lut, v));
    }
    let mut v = if raw <= center {
        -(center - raw) / (center - min)
    } else {
        (raw - center) / (max - center)
    }
    .clamp(-1.0, 1.0);
    if a.invert != 0 {
        v = -v;
    }
    let dzc = dz(a.dz_center);
    let dz_end = if v < 0.0 { dz(a.dz_min) } else { dz(a.dz_max) };
    let m = v.abs();
    if m <= dzc {
        // sign(v) counts as 0 here, so a LUT with lut[0] > 0 still rests at centre.
        return CENTER;
    }
    let m = ((m - dzc) / (1.0 - dzc - dz_end)).clamp(0.0, 1.0);
    to_u16(f64::from(CENTER) + v.signum() * lookup(&a.lut, m) / 2.0)
}

fn bit(mask: u16, button: u8) -> bool {
    (1..=16).contains(&button) && mask & (1 << (button - 1)) != 0
}

pub fn process(cfg: &CompiledConfig, raw: &[u8; 7]) -> JoyReport {
    let r = RawReport::parse(raw);
    let raw16 = r.raw16();
    let mut out = cfg
        .axes
        .map(|a| if a.mode == MODE_FULL { 0 } else { CENTER });
    // Physical order, so the higher index wins when two axes share a target.
    for (a, &v) in cfg.axes.iter().zip(&raw16) {
        if a.target != TARGET_DISABLED {
            if let Some(slot) = out.get_mut(usize::from(a.target)) {
                *slot = process_axis(a, v);
            }
        }
    }

    let hat_in = if r.hat > 7 { HAT_CENTERED } else { r.hat };
    let mut buttons = 0u32;
    if cfg.hat_mode != HAT_HAT && hat_in != HAT_CENTERED {
        // Up, Right, Down, Left -> buttons 13..16 (bits 12..15) per direction N..NW.
        const DIRS: [u32; 8] = [
            0b0001, 0b0011, 0b0010, 0b0110, 0b0100, 0b1100, 0b1000, 0b1001,
        ];
        buttons |= DIRS[usize::from(hat_in)] << 12;
    }
    let map = if bit(r.buttons, cfg.shift_button) {
        &cfg.button_map_shift
    } else {
        &cfg.button_map
    };
    for (i, &o) in map.iter().enumerate() {
        if r.buttons & (1 << i) != 0 && (1..=32).contains(&o) {
            buttons |= 1 << (o - 1);
        }
    }
    JoyReport {
        x: out[0],
        y: out[1],
        rz: out[2],
        slider: out[3],
        hat: if cfg.hat_mode == HAT_BUTTONS {
            HAT_CENTERED
        } else {
            hat_in
        },
        buttons,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AxisId, Calibration, DeviceConfig, HatMode};

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

    fn run(cfg: &DeviceConfig, raw: [u8; 7]) -> JoyReport {
        cfg.validate().unwrap();
        process(&CompiledConfig::from_config(cfg), &raw)
    }

    fn rest() -> [u8; 7] {
        frame(512, 512, 8, 128, 0, 0)
    }

    #[test]
    fn parse_matches_hardware_captures() {
        let r = RawReport::parse(&[0xfc, 0x01, 0x88, 0x82, 0x00, 0x00, 0x00]);
        assert_eq!(
            (r.x, r.y, r.hat, r.rz, r.slider, r.buttons),
            (508, 512, 8, 130, 0, 0)
        );
        let r = RawReport::parse(&[0xfc, 0xf1, 0x87, 0x80, 0x00, 0x02, 0x00]);
        assert_eq!((r.x, r.y, r.hat, r.rz, r.slider), (508, 508, 8, 128, 2));
        let r = RawReport::parse(&[0, 0, 0, 0, 0xA5, 0, 0xFA]);
        assert_eq!(r.buttons, 0xAA5);
        for b in [[0xfc, 0xf1, 0x87, 0x80, 0x00, 0x99, 0x00], [0xff; 7]] {
            let b = [b[0], b[1], b[2], b[3], b[4], b[5], b[6] & 0x0F];
            assert_eq!(RawReport::parse(&b).to_bytes(), b);
        }
    }

    #[test]
    fn identity_lut() {
        let lut = build_lut(&Curve::Linear, 1.0);
        for (i, &v) in lut.iter().enumerate() {
            assert_eq!(v, (i as f64 * 65535.0 / 16.0).round() as u16);
        }
    }

    #[test]
    fn identity_passes_through() {
        let c = DeviceConfig::default();
        let r = run(&c, rest());
        assert_eq!(
            (r.x, r.y, r.rz, r.slider, r.hat, r.buttons),
            (32768, 32768, 32768, 0, 8, 0)
        );
        let r = run(&c, frame(1023, 0, 3, 255, 255, 0xFFF));
        // 1023<<6 = 65472 of 65535 calibrated range.
        assert!((65470..=65472).contains(&r.x), "{}", r.x);
        assert_eq!(r.y, 1);
        assert_eq!(r.hat, 3);
        assert_eq!(r.buttons, 0xFFF);
        assert!((65278..=65280).contains(&r.slider), "{}", r.slider);
    }

    #[test]
    fn full_scale_with_calibration() {
        let mut c = DeviceConfig::default();
        c.axes.x.calibration = Calibration {
            min: 0,
            center: 32768,
            max: 1023 << 6,
        };
        assert_eq!(run(&c, frame(1023, 512, 8, 128, 0, 0)).x, 65535);
        assert_eq!(run(&c, frame(0, 512, 8, 128, 0, 0)).x, 1);
        c.axes.x.calibration = Calibration {
            min: 100 << 6,
            center: 500 << 6,
            max: 900 << 6,
        };
        assert_eq!(run(&c, frame(50, 512, 8, 128, 0, 0)).x, 1);
        assert_eq!(run(&c, frame(950, 512, 8, 128, 0, 0)).x, 65535);
        assert_eq!(run(&c, frame(500, 512, 8, 128, 0, 0)).x, 32768);
        assert_eq!(run(&c, frame(700, 512, 8, 128, 0, 0)).x, 49152);
    }

    #[test]
    fn inversion() {
        let mut c = DeviceConfig::default();
        c.axes.x.invert = true;
        c.axes.slider.invert = true;
        let r = run(&c, frame(0, 512, 8, 128, 0, 0));
        assert!(r.x >= 65534, "{}", r.x);
        assert_eq!(r.slider, 65535);
        let r = run(&c, frame(768, 512, 8, 128, 255, 0));
        assert!((16384..=16385).contains(&r.x), "{}", r.x);
        assert_eq!(r.slider, 255);
    }

    #[test]
    fn center_deadzone_rescales() {
        let mut c = DeviceConfig::default();
        c.axes.x.deadzone.center = 0.1;
        // 0.0625 of the half-range is inside the deadzone.
        assert_eq!(run(&c, frame(544, 512, 8, 128, 0, 0)).x, 32768);
        assert_eq!(run(&c, frame(480, 512, 8, 128, 0, 0)).x, 32768);
        // v = 0.55 -> (0.55 - 0.1) / 0.9 = 0.5.
        let r = run(&c, frame(512 + 281, 512, 8, 128, 0, 0));
        let expect = 32768.0 + (281.0 * 64.0 / 32767.0 - 0.1) / 0.9 * 32767.5;
        assert!(
            (f64::from(r.x) - expect).abs() <= 1.0,
            "{} vs {expect}",
            r.x
        );
    }

    #[test]
    fn end_deadzones_saturate_per_side() {
        let mut c = DeviceConfig::default();
        c.axes.y.deadzone.max = 0.2;
        assert_eq!(run(&c, frame(512, 512 + 420, 8, 128, 0, 0)).y, 65535);
        assert!(run(&c, frame(512, 512 - 420, 8, 128, 0, 0)).y > 1);
        c.axes.y.deadzone.max = 0.0;
        c.axes.y.deadzone.min = 0.2;
        assert_eq!(run(&c, frame(512, 512 - 420, 8, 128, 0, 0)).y, 1);
        assert!(run(&c, frame(512, 512 + 420, 8, 128, 0, 0)).y < 65535);
    }

    #[test]
    fn full_mode_deadzones() {
        let mut c = DeviceConfig::default();
        c.axes.slider.deadzone.min = 0.1;
        c.axes.slider.deadzone.max = 0.1;
        assert_eq!(run(&c, frame(512, 512, 8, 128, 20, 0)).slider, 0);
        assert_eq!(run(&c, frame(512, 512, 8, 128, 235, 0)).slider, 65535);
        let r = run(&c, frame(512, 512, 8, 128, 128, 0));
        assert!((32767..=32769).contains(&r.slider), "{}", r.slider);
    }

    #[test]
    fn exponent_curve() {
        let mut c = DeviceConfig::default();
        c.axes.rz.curve = Curve::Exponent { exponent: 2.0 };
        // Twist at +0.5 -> 0.25 of the half-range.
        let r = run(&c, frame(512, 512, 8, 192, 0, 0));
        assert!((40959..=40961).contains(&r.rz), "{}", r.rz);
        c.axes.rz.curve = Curve::Exponent { exponent: 0.5 };
        let r = run(&c, frame(512, 512, 8, 64, 0, 0));
        let expect = 32768.0 - 0.5f64.sqrt() * 32767.5;
        assert!(
            (f64::from(r.rz) - expect).abs() <= 1.0,
            "{} vs {expect}",
            r.rz
        );
    }

    #[test]
    fn points_curve() {
        let mut c = DeviceConfig::default();
        c.axes.x.curve = Curve::Points {
            points: vec![[0.0, 0.0], [0.25, 0.1], [0.5, 0.3], [0.75, 0.6], [1.0, 1.0]],
        };
        let r = run(&c, frame(512 + 256, 512, 8, 128, 0, 0));
        assert!((f64::from(r.x) - (32768.0 + 0.3 * 32767.5)).abs() <= 1.0);
        let lut = build_lut(
            &Curve::Points {
                points: vec![[0.25, 0.5], [0.75, 0.5]],
            },
            1.0,
        );
        assert_eq!(lut[0], 32768);
        assert_eq!(lut[16], 32768);
    }

    #[test]
    fn sensitivity_clamps() {
        let lut = build_lut(&Curve::Linear, 2.0);
        assert_eq!(lut[8], 65535);
        assert_eq!(lut[16], 65535);
        assert_eq!(lut[4], 32768);
        let mut c = DeviceConfig::default();
        c.axes.x.sensitivity = 2.0;
        assert_eq!(run(&c, frame(1023, 512, 8, 128, 0, 0)).x, 65535);
        assert_eq!(run(&c, frame(768, 512, 8, 128, 0, 0)).x, 65535);
    }

    #[test]
    fn target_swap_and_collision() {
        let mut c = DeviceConfig::default();
        c.axes.x.target = Some(AxisId::Y);
        c.axes.y.target = Some(AxisId::X);
        let r = run(&c, frame(1023, 512, 8, 128, 0, 0));
        assert_eq!(r.x, 32768);
        assert!(r.y > 65400);
        // Rz and slider both on X: slider (higher index) wins; Rz's own slot rests at centre.
        let mut c = DeviceConfig::default();
        c.axes.rz.target = Some(AxisId::X);
        c.axes.slider.target = Some(AxisId::X);
        let r = run(&c, frame(512, 512, 8, 255, 255, 0));
        assert!(r.x > 65000);
        assert_eq!(r.rz, 32768);
        assert_eq!(r.slider, 0);
    }

    #[test]
    fn disabled_axes_rest() {
        let mut c = DeviceConfig::default();
        c.axes.rz.target = None;
        c.axes.slider.target = None;
        let r = run(&c, frame(512, 512, 8, 255, 255, 0));
        assert_eq!((r.rz, r.slider), (32768, 0));
    }

    #[test]
    fn hat_modes() {
        let mut c = DeviceConfig::default();
        let hb = |r: JoyReport| (r.hat, r.buttons >> 12);
        assert_eq!(hb(run(&c, frame(512, 512, 1, 128, 0, 0))), (1, 0));
        c.hat_mode = HatMode::Buttons;
        assert_eq!(hb(run(&c, frame(512, 512, 0, 128, 0, 0))), (8, 0b0001));
        assert_eq!(hb(run(&c, frame(512, 512, 3, 128, 0, 0))), (8, 0b0110));
        assert_eq!(hb(run(&c, frame(512, 512, 7, 128, 0, 0))), (8, 0b1001));
        assert_eq!(hb(run(&c, frame(512, 512, 8, 128, 0, 0))), (8, 0));
        c.hat_mode = HatMode::Both;
        assert_eq!(hb(run(&c, frame(512, 512, 5, 128, 0, 0))), (5, 0b1100));
        assert_eq!(hb(run(&c, frame(512, 512, 15, 128, 0, 0))), (8, 0));
    }

    #[test]
    fn shift_layer() {
        let mut c = DeviceConfig {
            shift_button: Some(12),
            ..DeviceConfig::default()
        };
        c.buttons_shifted[0].output = Some(20);
        // Unshifted: button 1 -> 1. Shift itself is mapped to 12 unshifted, nothing shifted.
        assert_eq!(run(&c, frame(512, 512, 8, 128, 0, 0b1)).buttons, 0b1);
        let r = run(&c, frame(512, 512, 8, 128, 0, 0b1000_0000_0001));
        assert_eq!(r.buttons, 1 << 19);
        c.buttons_shifted[11].output = Some(32);
        let r = run(&c, frame(512, 512, 8, 128, 0, 0b1000_0000_0000));
        assert_eq!(r.buttons, 1 << 31);
    }

    #[test]
    fn dropped_and_remapped_buttons() {
        let mut c = DeviceConfig::default();
        c.buttons[1].output = None;
        c.buttons[2].output = Some(1);
        let r = run(&c, frame(512, 512, 8, 128, 0, 0b110));
        assert_eq!(r.buttons, 0b1);
    }
}
