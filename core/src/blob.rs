//! The 240-byte `X3D_CONFIG` blob the driver consumes (CONTRACT §4.2).

use crate::config::{CurveMode, DeviceConfig, HatMode};
use crate::pipeline::build_lut;
use std::fmt;

pub const BLOB_LEN: usize = 240;
pub const MAGIC: u32 = 0x5044_3358; // "X3DP"
pub const VERSION: u16 = 1;
const AXIS_LEN: usize = 50;
const AXES_OFFSET: usize = 12;

pub const MODE_SYMMETRIC: u8 = 0;
pub const MODE_FULL: u8 = 1;
pub const TARGET_DISABLED: u8 = 0xFF;
pub const HAT_HAT: u8 = 0;
pub const HAT_BUTTONS: u8 = 1;
pub const HAT_BOTH: u8 = 2;

/// CRC of `compile(&DeviceConfig::default())`; the driver's built-in identity blob must match.
pub const IDENTITY_BLOB_CRC: u32 = 0x6AE1_FFEB;

/// One axis in blob form. Deadzones are permille.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompiledAxis {
    pub cal_min: u16,
    pub cal_center: u16,
    pub cal_max: u16,
    pub dz_center: u16,
    pub dz_min: u16,
    pub dz_max: u16,
    pub invert: u8,
    pub mode: u8,
    pub target: u8,
    pub lut: [u16; 17],
}

/// The numeric config the pipeline runs on; field-for-field the blob.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompiledConfig {
    pub axes: [CompiledAxis; 4],
    pub hat_mode: u8,
    pub shift_button: u8,
    pub button_map: [u8; 12],
    pub button_map_shift: [u8; 12],
}

impl CompiledConfig {
    /// Does not validate; call [`DeviceConfig::validate`] first.
    pub fn from_config(cfg: &DeviceConfig) -> Self {
        let axes = cfg.axes.all().map(|a| {
            let [dz_center, dz_min, dz_max] = a.deadzone.permille();
            CompiledAxis {
                cal_min: a.calibration.min,
                cal_center: a.calibration.center,
                cal_max: a.calibration.max,
                dz_center,
                dz_min,
                dz_max,
                invert: a.invert.into(),
                mode: match a.curve_mode {
                    CurveMode::Symmetric => MODE_SYMMETRIC,
                    CurveMode::Full => MODE_FULL,
                },
                target: a.target.map_or(TARGET_DISABLED, |t| t.index() as u8),
                lut: build_lut(&a.curve, a.sensitivity),
            }
        });
        CompiledConfig {
            axes,
            hat_mode: match cfg.hat_mode {
                HatMode::Hat => HAT_HAT,
                HatMode::Buttons => HAT_BUTTONS,
                HatMode::Both => HAT_BOTH,
            },
            shift_button: cfg.shift_button.unwrap_or(0),
            button_map: cfg.buttons.map(|b| b.output.unwrap_or(0)),
            button_map_shift: cfg.buttons_shifted.map(|b| b.output.unwrap_or(0)),
        }
    }

    pub fn to_bytes(&self) -> [u8; BLOB_LEN] {
        let mut v = Vec::with_capacity(BLOB_LEN);
        v.extend_from_slice(&MAGIC.to_le_bytes());
        v.extend_from_slice(&VERSION.to_le_bytes());
        v.extend_from_slice(&(BLOB_LEN as u16).to_le_bytes());
        v.extend_from_slice(&[0; 4]); // crc, filled below
        for a in &self.axes {
            for w in [
                a.cal_min,
                a.cal_center,
                a.cal_max,
                a.dz_center,
                a.dz_min,
                a.dz_max,
            ] {
                v.extend_from_slice(&w.to_le_bytes());
            }
            v.extend_from_slice(&[a.invert, a.mode, a.target, 0]);
            for w in a.lut {
                v.extend_from_slice(&w.to_le_bytes());
            }
        }
        v.extend_from_slice(&[self.hat_mode, self.shift_button]);
        v.extend_from_slice(&self.button_map);
        v.extend_from_slice(&self.button_map_shift);
        v.extend_from_slice(&[0; 2]);
        let mut b = [0u8; BLOB_LEN];
        b.copy_from_slice(&v);
        let crc = crc32(&b[12..]);
        b[8..12].copy_from_slice(&crc.to_le_bytes());
        b
    }
}

/// Compiles a config into the blob. Does not validate; call [`DeviceConfig::validate`] first.
pub fn compile(cfg: &DeviceConfig) -> [u8; BLOB_LEN] {
    CompiledConfig::from_config(cfg).to_bytes()
}

/// The CRC stored in a blob header.
pub fn blob_crc(blob: &[u8; BLOB_LEN]) -> u32 {
    u32::from_le_bytes([blob[8], blob[9], blob[10], blob[11]])
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlobError {
    TooShort(usize),
    BadMagic,
    BadVersion(u16),
    BadSize(u16),
    BadCrc,
    Invalid(&'static str),
}

impl fmt::Display for BlobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BlobError::TooShort(n) => write!(f, "config blob too short ({n} bytes)"),
            BlobError::BadMagic => f.write_str("config blob has a bad magic"),
            BlobError::BadVersion(v) => write!(f, "unsupported config blob version {v}"),
            BlobError::BadSize(s) => write!(f, "config blob size field is {s}"),
            BlobError::BadCrc => f.write_str("config blob CRC mismatch"),
            BlobError::Invalid(what) => write!(f, "config blob invalid: {what}"),
        }
    }
}

impl std::error::Error for BlobError {}

/// Parses and validates a blob. Accepts trailing padding (feature report 3 carries 255 bytes).
pub fn parse(bytes: &[u8]) -> Result<CompiledConfig, BlobError> {
    if bytes.len() < BLOB_LEN {
        return Err(BlobError::TooShort(bytes.len()));
    }
    let b = &bytes[..BLOB_LEN];
    let u16_at = |o: usize| u16::from_le_bytes([b[o], b[o + 1]]);
    if u32::from_le_bytes([b[0], b[1], b[2], b[3]]) != MAGIC {
        return Err(BlobError::BadMagic);
    }
    if u16_at(4) != VERSION {
        return Err(BlobError::BadVersion(u16_at(4)));
    }
    if u16_at(6) as usize != BLOB_LEN {
        return Err(BlobError::BadSize(u16_at(6)));
    }
    if u32::from_le_bytes([b[8], b[9], b[10], b[11]]) != crc32(&b[12..]) {
        return Err(BlobError::BadCrc);
    }
    let axes: [CompiledAxis; 4] = std::array::from_fn(|i| {
        let o = AXES_OFFSET + i * AXIS_LEN;
        CompiledAxis {
            cal_min: u16_at(o),
            cal_center: u16_at(o + 2),
            cal_max: u16_at(o + 4),
            dz_center: u16_at(o + 6),
            dz_min: u16_at(o + 8),
            dz_max: u16_at(o + 10),
            invert: b[o + 12],
            mode: b[o + 13],
            target: b[o + 14],
            lut: std::array::from_fn(|j| u16_at(o + 16 + 2 * j)),
        }
    });
    for a in &axes {
        validate_axis(a)?;
    }
    let mut button_map = [0u8; 12];
    let mut button_map_shift = [0u8; 12];
    button_map.copy_from_slice(&b[214..226]);
    button_map_shift.copy_from_slice(&b[226..238]);
    let cfg = CompiledConfig {
        axes,
        hat_mode: b[212],
        shift_button: b[213],
        button_map,
        button_map_shift,
    };
    if cfg.hat_mode > HAT_BOTH {
        return Err(BlobError::Invalid("hat_mode"));
    }
    if cfg.shift_button > 12 {
        return Err(BlobError::Invalid("shift_button"));
    }
    if button_map.iter().chain(&button_map_shift).any(|&o| o > 32) {
        return Err(BlobError::Invalid("button map"));
    }
    Ok(cfg)
}

fn validate_axis(a: &CompiledAxis) -> Result<(), BlobError> {
    if a.invert > 1 {
        return Err(BlobError::Invalid("invert"));
    }
    if a.target > 3 && a.target != TARGET_DISABLED {
        return Err(BlobError::Invalid("target"));
    }
    let cal_ok = match a.mode {
        MODE_SYMMETRIC => a.cal_min < a.cal_center && a.cal_center < a.cal_max,
        MODE_FULL => a.cal_min < a.cal_max,
        _ => return Err(BlobError::Invalid("mode")),
    };
    if !cal_ok {
        return Err(BlobError::Invalid("calibration"));
    }
    let (c, lo, hi) = (a.dz_center, a.dz_min, a.dz_max);
    if c > 500 || lo > 500 || hi > 500 || c + lo >= 1000 || c + hi >= 1000 || lo + hi >= 1000 {
        return Err(BlobError::Invalid("deadzone"));
    }
    Ok(())
}

/// CRC-32/IEEE 802.3 (reflected, poly 0xEDB88320), as zlib/`RtlComputeCrc32`.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & (crc & 1).wrapping_neg());
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AxisId, Curve};

    #[test]
    fn crc32_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn identity_blob_layout() {
        let b = compile(&DeviceConfig::default());
        assert_eq!(&b[0..4], b"X3DP");
        assert_eq!(&b[4..8], &[1, 0, 240, 0]);
        assert_eq!(blob_crc(&b), IDENTITY_BLOB_CRC);
        // X axis: cal 0/32768/65535, dz 0, invert 0, mode 0, target 0, lut[0] = 0, lut[16] = 65535.
        assert_eq!(&b[12..18], &[0, 0, 0, 0x80, 0xFF, 0xFF]);
        assert_eq!(&b[24..28], &[0, 0, 0, 0]);
        assert_eq!(&b[60..62], &[0xFF, 0xFF]);
        // Slider (axis 3) at 12 + 150: center 0, mode full, target 3.
        assert_eq!(&b[162..168], &[0, 0, 0, 0, 0xFF, 0xFF]);
        assert_eq!(&b[174..178], &[0, 1, 3, 0]);
        assert_eq!(b[212], HAT_HAT);
        assert_eq!(b[213], 0);
        assert_eq!(&b[214..226], &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
        assert_eq!(&b[226..240], &[0; 14]);
    }

    #[test]
    fn round_trip() {
        let mut c = DeviceConfig::default();
        c.axes.x.invert = true;
        c.axes.x.deadzone.center = 0.05;
        c.axes.y.target = None;
        c.axes.rz.curve = Curve::Exponent { exponent: 2.0 };
        c.axes.slider.target = Some(AxisId::X);
        c.hat_mode = HatMode::Both;
        c.shift_button = Some(3);
        c.buttons_shifted[0].output = Some(32);
        let compiled = CompiledConfig::from_config(&c);
        let blob = compiled.to_bytes();
        assert_eq!(parse(&blob).unwrap(), compiled);
        let mut padded = blob.to_vec();
        padded.resize(255, 0);
        assert_eq!(parse(&padded).unwrap(), compiled);
        assert_eq!(compiled.axes[0].dz_center, 50);
        assert_eq!(compiled.axes[1].target, TARGET_DISABLED);
    }

    #[test]
    fn parse_rejects_corruption() {
        let blob = compile(&DeviceConfig::default());
        assert_eq!(parse(&blob[..239]), Err(BlobError::TooShort(239)));
        let mut b = blob;
        b[0] = b'Y';
        assert_eq!(parse(&b), Err(BlobError::BadMagic));
        let mut b = blob;
        b[4] = 2;
        assert_eq!(parse(&b), Err(BlobError::BadVersion(2)));
        let mut b = blob;
        b[6] = 241;
        assert_eq!(parse(&b), Err(BlobError::BadSize(241)));
        let mut b = blob;
        b[100] ^= 1;
        assert_eq!(parse(&b), Err(BlobError::BadCrc));
    }

    #[test]
    fn parse_rejects_invalid_fields() {
        let mut c = CompiledConfig::from_config(&DeviceConfig::default());
        c.axes[0].cal_center = c.axes[0].cal_max;
        assert_eq!(parse(&c.to_bytes()), Err(BlobError::Invalid("calibration")));
        let mut c = CompiledConfig::from_config(&DeviceConfig::default());
        c.axes[2].dz_center = 500;
        c.axes[2].dz_max = 500;
        assert_eq!(parse(&c.to_bytes()), Err(BlobError::Invalid("deadzone")));
        let mut c = CompiledConfig::from_config(&DeviceConfig::default());
        c.button_map[5] = 33;
        assert_eq!(parse(&c.to_bytes()), Err(BlobError::Invalid("button map")));
    }
}
