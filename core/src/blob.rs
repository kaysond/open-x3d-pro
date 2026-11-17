//! The 240-byte `X3D_CONFIG` blob the driver consumes (CONTRACT §4.2).

use crate::config::{DeviceConfig, HatMode};
use crate::pipeline::build_lut;

pub const BLOB_LEN: usize = 240;
pub const MAGIC: u32 = 0x5044_3358; // "X3DP"
pub const VERSION: u16 = 2;

pub const TARGET_DISABLED: u8 = 0xFF;
pub const HAT_HAT: u8 = 0;
pub const HAT_BUTTONS: u8 = 1;
pub const HAT_BOTH: u8 = 2;

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
            v.extend_from_slice(&[a.invert, a.target, 0, 0]);
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
    use crate::config::AxisId;

    #[test]
    fn crc32_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn identity_blob_layout() {
        let b = compile(&DeviceConfig::default());
        assert_eq!(&b[0..4], b"X3DP");
        assert_eq!(&b[4..8], &[2, 0, 240, 0]);
        // The driver's built-in identity blob must carry the same CRC.
        assert_eq!(blob_crc(&b), 0x0A5E_7748);
        // X axis: cal 0/32768/65472, dz 0, invert 0, target 0, lut[0] = 0, lut[16] = 32767.
        assert_eq!(&b[12..18], &[0, 0, 0, 0x80, 0xC0, 0xFF]);
        assert_eq!(&b[24..28], &[0, 0, 0, 0]);
        assert_eq!(&b[60..62], &[0xFF, 0x7F]);
        // Slider (axis 3) at 12 + 150: cal max 65280, target 3.
        assert_eq!(&b[162..168], &[0, 0, 0, 0x80, 0x00, 0xFF]);
        assert_eq!(&b[174..178], &[0, 3, 0, 0]);
        assert_eq!(b[212], HAT_HAT);
        assert_eq!(b[213], 0);
        assert_eq!(&b[214..226], &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
        assert_eq!(&b[226..240], &[0; 14]);
    }

    #[test]
    fn non_default_fields_land_at_their_offsets() {
        let mut c = DeviceConfig::default();
        c.axes.x.invert = true;
        c.axes.x.deadzone.center = 0.05;
        c.axes.y.target = None;
        c.axes.slider.target = Some(AxisId::X);
        c.hat_mode = HatMode::Both;
        c.shift_button = Some(3);
        c.buttons_shifted[0].output = Some(32);
        let b = compile(&c);
        assert_eq!(&b[18..20], &[50, 0]);
        assert_eq!(b[24], 1);
        assert_eq!(b[12 + 50 + 13], TARGET_DISABLED);
        assert_eq!(b[12 + 150 + 13], 0);
        assert_eq!(&b[212..214], &[HAT_BOTH, 3]);
        assert_eq!(b[226], 32);
        assert_eq!(blob_crc(&b), crc32(&b[12..]));
    }
}
