//! HID interface the driver publishes (CONTRACT §2.1): descriptor bytes, IDs and report codecs.

pub use crate::blob::BLOB_LEN;
use crate::pipeline::JoyReport;

pub const VID: u16 = 0x046D;
pub const PID: u16 = 0xC215;
pub const USAGE_PAGE_GENERIC_DESKTOP: u16 = 0x01;
pub const USAGE_JOYSTICK: u16 = 0x04;
pub const USAGE_PAGE_VENDOR: u16 = 0xFF00;
pub const USAGE_VENDOR: u16 = 0x01;

pub const REPORT_ID_JOYSTICK: u8 = 1;
pub const REPORT_ID_RAW: u8 = 2;
pub const REPORT_ID_CONFIG: u8 = 3;
pub const REPORT_ID_INFO: u8 = 4;

/// Report lengths including the report ID byte.
pub const JOYSTICK_REPORT_LEN: usize = 14;
pub const RAW_REPORT_LEN: usize = 11;
pub const CONFIG_REPORT_LEN: usize = 256;
pub const INFO_REPORT_LEN: usize = 32;

/// `flags` / `state` bits of reports 2 and 4.
pub const FLAG_FROM_REGISTRY: u8 = 0x01;
pub const FLAG_IDENTITY: u8 = 0x02;

/// Byte-identical to the driver's descriptor (driver/openx3d). Change both together.
#[rustfmt::skip]
pub const REPORT_DESCRIPTOR: &[u8] = &[
    // Collection 1: joystick, report ID 1.
    0x05, 0x01,                   // Usage Page (Generic Desktop)
    0x09, 0x04,                   // Usage (Joystick)
    0xA1, 0x01,                   // Collection (Application)
    0x85, 0x01,                   //   Report ID (1)
    0x09, 0x30,                   //   Usage (X)
    0x09, 0x31,                   //   Usage (Y)
    0x09, 0x35,                   //   Usage (Rz)
    0x09, 0x36,                   //   Usage (Slider)
    0x15, 0x00,                   //   Logical Minimum (0)
    0x27, 0xFF, 0xFF, 0x00, 0x00, //   Logical Maximum (65535)
    0x75, 0x10,                   //   Report Size (16)
    0x95, 0x04,                   //   Report Count (4)
    0x81, 0x02,                   //   Input (Data,Var,Abs)
    0x09, 0x39,                   //   Usage (Hat switch)
    0x25, 0x07,                   //   Logical Maximum (7)
    0x35, 0x00,                   //   Physical Minimum (0)
    0x46, 0x3B, 0x01,             //   Physical Maximum (315)
    0x65, 0x14,                   //   Unit (Eng Rot: Degrees)
    0x75, 0x04,                   //   Report Size (4)
    0x95, 0x01,                   //   Report Count (1)
    0x81, 0x42,                   //   Input (Data,Var,Abs,Null)
    0x45, 0x00,                   //   Physical Maximum (0)
    0x65, 0x00,                   //   Unit (None)
    0x81, 0x03,                   //   Input (Const,Var,Abs): 4-bit pad
    0x05, 0x09,                   //   Usage Page (Button)
    0x19, 0x01,                   //   Usage Minimum (1)
    0x29, 0x20,                   //   Usage Maximum (32)
    0x25, 0x01,                   //   Logical Maximum (1)
    0x75, 0x01,                   //   Report Size (1)
    0x95, 0x20,                   //   Report Count (32)
    0x81, 0x02,                   //   Input (Data,Var,Abs)
    0xC0,                         // End Collection
    // Collection 2: vendor channel.
    0x06, 0x00, 0xFF,             // Usage Page (Vendor 0xFF00)
    0x09, 0x01,                   // Usage (0x01)
    0xA1, 0x01,                   // Collection (Application)
    0x85, 0x02,                   //   Report ID (2)
    0x09, 0x01,                   //   Usage (0x01)
    0x15, 0x00,                   //   Logical Minimum (0)
    0x26, 0xFF, 0x00,             //   Logical Maximum (255)
    0x75, 0x08,                   //   Report Size (8)
    0x95, 0x0A,                   //   Report Count (10)
    0x81, 0x02,                   //   Input (Data,Var,Abs): raw[7], seq u16, flags u8
    0x85, 0x03,                   //   Report ID (3)
    0x09, 0x02,                   //   Usage (0x02)
    0x95, 0xFF,                   //   Report Count (255)
    0xB1, 0x02,                   //   Feature (Data,Var,Abs): config blob
    0x85, 0x04,                   //   Report ID (4)
    0x09, 0x03,                   //   Usage (0x03)
    0x95, 0x1F,                   //   Report Count (31)
    0xB1, 0x02,                   //   Feature (Data,Var,Abs): driver info
    0xC0,                         // End Collection
];

/// Input report 2 from the vendor collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VendorInput {
    pub raw: [u8; 7],
    pub seq: u16,
    pub flags: u8,
}

impl VendorInput {
    /// `buf` starts with the report ID.
    pub fn parse(buf: &[u8]) -> Option<Self> {
        if buf.len() < RAW_REPORT_LEN || buf[0] != REPORT_ID_RAW {
            return None;
        }
        let mut raw = [0u8; 7];
        raw.copy_from_slice(&buf[1..8]);
        Some(VendorInput {
            raw,
            seq: u16::from_le_bytes([buf[8], buf[9]]),
            flags: buf[10],
        })
    }
}

/// Feature report 4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DriverInfo {
    pub driver_version_bcd: u16,
    pub bcd_device: u16,
    pub active_config_crc: u32,
    pub raw_last: [u8; 7],
    pub state: u8,
}

impl DriverInfo {
    /// `buf` starts with the report ID.
    pub fn parse(buf: &[u8]) -> Option<Self> {
        if buf.len() < 17 || buf[0] != REPORT_ID_INFO {
            return None;
        }
        let mut raw_last = [0u8; 7];
        raw_last.copy_from_slice(&buf[9..16]);
        Some(DriverInfo {
            driver_version_bcd: u16::from_le_bytes([buf[1], buf[2]]),
            bcd_device: u16::from_le_bytes([buf[3], buf[4]]),
            active_config_crc: u32::from_le_bytes([buf[5], buf[6], buf[7], buf[8]]),
            raw_last,
            state: buf[16],
        })
    }

    /// `0x0010` -> `"0.1.0"`.
    pub fn version_string(&self) -> String {
        let v = self.driver_version_bcd;
        format!("{}.{}.{}", v >> 8, (v >> 4) & 0xF, v & 0xF)
    }
}

/// Input report 1 from the joystick collection; `buf` starts with the report ID.
pub fn parse_joystick_report(buf: &[u8]) -> Option<JoyReport> {
    if buf.len() < JOYSTICK_REPORT_LEN || buf[0] != REPORT_ID_JOYSTICK {
        return None;
    }
    let u16_at = |o: usize| u16::from_le_bytes([buf[o], buf[o + 1]]);
    Some(JoyReport {
        x: u16_at(1),
        y: u16_at(3),
        rz: u16_at(5),
        slider: u16_at(7),
        hat: buf[9] & 0x0F,
        buttons: u32::from_le_bytes([buf[10], buf[11], buf[12], buf[13]]),
    })
}

/// Feature report 3 (ID + blob, zero padded) for `HidD_SetFeature`.
pub fn config_report(blob: &[u8; BLOB_LEN]) -> [u8; CONFIG_REPORT_LEN] {
    let mut r = [0u8; CONFIG_REPORT_LEN];
    r[0] = REPORT_ID_CONFIG;
    r[1..=BLOB_LEN].copy_from_slice(blob);
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const INPUT: u8 = 0x8;
    const FEATURE: u8 = 0xB;

    /// Data bits per (main item tag, report ID).
    type ReportBits = BTreeMap<(u8, u8), u32>;

    /// Walks short items; returns report sizes and the top-level collections' (page, usage).
    fn walk(desc: &[u8]) -> (ReportBits, Vec<(u32, u32)>) {
        let (mut size, mut count, mut id, mut page, mut usage) = (0u32, 0u32, 0u8, 0u32, 0u32);
        let (mut bits, mut tops, mut depth) = (BTreeMap::new(), Vec::new(), 0);
        let mut i = 0;
        while i < desc.len() {
            let prefix = desc[i];
            assert_ne!(prefix, 0xFE, "long items not expected");
            let len = [0, 1, 2, 4][usize::from(prefix & 3)];
            let data = desc[i + 1..i + 1 + len]
                .iter()
                .rev()
                .fold(0u32, |acc, &b| (acc << 8) | u32::from(b));
            match (prefix >> 2 & 3, prefix >> 4) {
                (0, INPUT | FEATURE | 0x9) => {
                    *bits.entry((prefix >> 4, id)).or_insert(0) += size * count;
                }
                (0, 0xA) => {
                    if depth == 0 {
                        tops.push((page, usage));
                    }
                    depth += 1;
                }
                (0, 0xC) => depth -= 1,
                (1, 0x0) => page = data,
                (1, 0x7) => size = data,
                (1, 0x8) => id = data as u8,
                (1, 0x9) => count = data,
                (2, 0x0) => usage = data,
                _ => {}
            }
            i += 1 + len;
        }
        assert_eq!(depth, 0, "unbalanced collections");
        (bits, tops)
    }

    #[test]
    fn descriptor_report_sizes() {
        let (bits, tops) = walk(REPORT_DESCRIPTOR);
        assert_eq!(
            tops,
            vec![
                (
                    u32::from(USAGE_PAGE_GENERIC_DESKTOP),
                    u32::from(USAGE_JOYSTICK)
                ),
                (u32::from(USAGE_PAGE_VENDOR), u32::from(USAGE_VENDOR)),
            ]
        );
        let bytes = |tag, id| bits[&(tag, id)] / 8;
        assert_eq!(
            bytes(INPUT, REPORT_ID_JOYSTICK) as usize,
            JOYSTICK_REPORT_LEN - 1
        );
        assert_eq!(bytes(INPUT, REPORT_ID_RAW) as usize, RAW_REPORT_LEN - 1);
        assert_eq!(
            bytes(FEATURE, REPORT_ID_CONFIG) as usize,
            CONFIG_REPORT_LEN - 1
        );
        assert_eq!(bytes(FEATURE, REPORT_ID_INFO) as usize, INFO_REPORT_LEN - 1);
        assert_eq!(bits.len(), 4, "no other reports: {bits:?}");
        assert!(bits.values().all(|b| b % 8 == 0));
    }

    #[test]
    fn report_codecs() {
        let mut b = [0u8; JOYSTICK_REPORT_LEN];
        b[0] = 1;
        b[1..3].copy_from_slice(&65535u16.to_le_bytes());
        b[7..9].copy_from_slice(&1234u16.to_le_bytes());
        b[9] = 0xF5;
        b[10..14].copy_from_slice(&0x8000_0001u32.to_le_bytes());
        let j = parse_joystick_report(&b).unwrap();
        assert_eq!(
            (j.x, j.slider, j.hat, j.buttons),
            (65535, 1234, 5, 0x8000_0001)
        );
        assert!(parse_joystick_report(&b[..13]).is_none());

        let v =
            VendorInput::parse(&[2, 0xfc, 0xf1, 0x87, 0x80, 0, 0x99, 0, 0x34, 0x12, 3]).unwrap();
        assert_eq!((v.raw[5], v.seq, v.flags), (0x99, 0x1234, 3));
        assert!(VendorInput::parse(&[1; 11]).is_none());

        let mut info = [0u8; INFO_REPORT_LEN];
        info[0] = 4;
        info[1] = 0x10;
        info[5..9].copy_from_slice(&0xDEAD_BEEFu32.to_le_bytes());
        info[16] = FLAG_IDENTITY;
        let d = DriverInfo::parse(&info).unwrap();
        assert_eq!(d.version_string(), "0.1.0");
        assert_eq!(d.active_config_crc, 0xDEAD_BEEF);
        assert_eq!(d.state, FLAG_IDENTITY);

        let blob = crate::blob::compile(&Default::default());
        let r = config_report(&blob);
        assert_eq!(r[0], REPORT_ID_CONFIG);
        assert_eq!(&r[1..=BLOB_LEN], &blob);
        assert!(r[BLOB_LEN + 1..].iter().all(|&b| b == 0));
    }
}
