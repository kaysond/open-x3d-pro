//! Joystick I/O. With the driver: raw reports from the vendor collection (Col02), processed
//! reports from the joystick collection (Col01), config pushes as feature report 3. Without it
//! (fallback): stock 7-byte reports, processed in-app with the reference pipeline.

use serde::Serialize;
use std::time::{Duration, Instant};
use x3d_core::pipeline::{JoyReport, RawReport};

/// Cap on `input` events for the UI (~60 Hz).
const MIN_EMIT_INTERVAL: Duration = Duration::from_micros(16_667);

/// Payload of the `input` event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct InputFrame {
    pub raw: RawReport,
    pub processed: JoyReport,
    pub seq: u16,
}

/// A stock-driver read: 7 bytes, or 8 if the zero report ID was left in.
pub fn fallback_frame(buf: &[u8]) -> Option<[u8; 7]> {
    match *buf {
        [a, b, c, d, e, f, g] | [0, a, b, c, d, e, f, g] => Some([a, b, c, d, e, f, g]),
        _ => None,
    }
}

/// Rate-limits a stream of frames while never dropping the last one.
#[derive(Debug, Default)]
pub struct Coalescer {
    last: Option<Instant>,
    pending: bool,
}

impl Coalescer {
    pub fn mark(&mut self) {
        self.pending = true;
    }

    /// True when a marked frame should be emitted now; call on every loop turn, including timeouts.
    pub fn due(&mut self, now: Instant) -> bool {
        let ready = self.pending
            && self
                .last
                .is_none_or(|t| now.duration_since(t) >= MIN_EMIT_INTERVAL);
        if ready {
            self.pending = false;
            self.last = Some(now);
        }
        ready
    }
}

#[cfg(windows)]
pub use imp::spawn;

#[cfg(windows)]
mod imp {
    use super::{fallback_frame, Coalescer, InputFrame};
    use crate::keys::{self, Chords};
    use crate::state::{DeviceStatus, Shared};
    use hidapi::{DeviceInfo, HidApi, HidDevice};
    use std::time::{Duration, Instant};
    use tauri::{AppHandle, Emitter, Manager};
    use x3d_core::hid::{
        config_report, parse_joystick_report, DriverInfo, VendorInput, CONFIG_REPORT_LEN, PID,
        REPORT_ID_INFO, USAGE_JOYSTICK, USAGE_PAGE_GENERIC_DESKTOP, USAGE_PAGE_VENDOR,
        USAGE_VENDOR, VID,
    };
    use x3d_core::pipeline::{process, JoyReport, RawReport};

    struct Conn {
        /// Col02 with the driver, else the stock joystick collection.
        input: HidDevice,
        /// Col01 (driver only); `None` = compute processed values in-app.
        joystick: Option<HidDevice>,
        driver: bool,
    }

    pub fn spawn(app: &AppHandle) {
        let app = app.clone();
        if let Err(e) = std::thread::Builder::new()
            .name("device".into())
            .spawn(move || run(&app))
        {
            log::error!("cannot start device thread: {e}");
        }
    }

    fn run(app: &AppHandle) {
        let mut api = match HidApi::new() {
            Ok(api) => api,
            Err(e) => {
                log::error!("HID init failed: {e}");
                return;
            }
        };
        loop {
            if let Err(e) = api.refresh_devices() {
                log::warn!("HID enumeration failed: {e}");
            }
            if let Some(conn) = open(&api) {
                session(app, &conn);
                app.state::<Shared>().lock().device = DeviceStatus::default();
                crate::notify(app, None);
            }
            // ponytail: polls enumeration every second; switch to device-arrival notifications if it ever shows in a profile.
            std::thread::sleep(Duration::from_secs(1));
        }
    }

    fn open(api: &HidApi) -> Option<Conn> {
        let find = |page: u16, usage: u16| {
            api.device_list().find(|d| {
                d.vendor_id() == VID
                    && d.product_id() == PID
                    && d.usage_page() == page
                    && d.usage() == usage
            })
        };
        let open = |info: &DeviceInfo| {
            info.open_device(api)
                .map_err(|e| log::warn!("cannot open {:?}: {e}", info.path()))
                .ok()
        };
        let joystick = find(USAGE_PAGE_GENERIC_DESKTOP, USAGE_JOYSTICK).and_then(open);
        match find(USAGE_PAGE_VENDOR, USAGE_VENDOR) {
            // Driver present: retry later rather than misread its joystick collection as stock.
            Some(vendor) => Some(Conn {
                input: open(vendor)?,
                joystick,
                driver: true,
            }),
            None => joystick.map(|input| Conn {
                input,
                joystick: None,
                driver: false,
            }),
        }
    }

    fn read_info(dev: &HidDevice) -> Option<DriverInfo> {
        // hidclass wants a buffer as long as the longest feature report.
        let mut buf = [0u8; CONFIG_REPORT_LEN];
        buf[0] = REPORT_ID_INFO;
        match dev.get_feature_report(&mut buf) {
            Ok(n) => DriverInfo::parse(&buf[..n.min(buf.len())]),
            Err(e) => {
                log::warn!("cannot read driver info: {e}");
                None
            }
        }
    }

    fn session(app: &AppHandle, conn: &Conn) {
        let shared = app.state::<Shared>().inner().clone();
        let info = if conn.driver {
            read_info(&conn.input)
        } else {
            None
        };
        {
            let mut s = shared.lock();
            s.device = DeviceStatus {
                connected: true,
                driver_installed: conn.driver,
                driver_version: info.map(|i| i.version_string()),
            };
            s.switcher.set_device_crc(info.map(|i| i.active_config_crc));
        }
        log::info!(
            "joystick connected ({})",
            if conn.driver {
                "Open X3D driver"
            } else {
                "stock driver, fallback mode"
            }
        );
        crate::notify(app, None);

        let mut buf = [0u8; 64];
        let mut raw = RawReport::default();
        let mut processed = JoyReport::default();
        let mut seq = 0u16;
        let mut coalescer = Coalescer::default();
        let mut chords = Chords::default();
        'session: loop {
            if conn.driver {
                let blob = shared.lock().switcher.take_push();
                if let Some(blob) = blob {
                    match conn.input.send_feature_report(&config_report(&blob)) {
                        Ok(()) => log::debug!("config pushed"),
                        Err(e) => log::warn!("config push failed: {e}"),
                    }
                }
            }

            let n = match conn.input.read_timeout(&mut buf, 10) {
                Ok(n) => n,
                Err(e) => {
                    log::info!("joystick lost: {e}");
                    break;
                }
            };
            let frame = if n == 0 {
                None
            } else if conn.driver {
                VendorInput::parse(&buf[..n]).map(|v| {
                    seq = v.seq;
                    v.raw
                })
            } else {
                fallback_frame(&buf[..n]).inspect(|_| seq = seq.wrapping_add(1))
            };
            if let Some(bytes) = frame {
                raw = RawReport::parse(&bytes);
                let events = {
                    let mut s = shared.lock();
                    s.calibration.update(&raw);
                    if conn.joystick.is_none() {
                        processed = process(s.switcher.compiled(), &bytes);
                    }
                    chords.update(raw.buttons, s.switcher.key_bindings())
                };
                if !events.is_empty() {
                    keys::send(&events);
                }
                coalescer.mark();
            }

            if let Some(joy) = &conn.joystick {
                loop {
                    match joy.read_timeout(&mut buf, 0) {
                        Ok(0) => break,
                        Ok(n) => {
                            if let Some(p) = parse_joystick_report(&buf[..n]) {
                                processed = p;
                            }
                        }
                        Err(e) => {
                            log::info!("joystick collection lost: {e}");
                            break 'session;
                        }
                    }
                }
            }

            if coalescer.due(Instant::now()) {
                let frame = InputFrame {
                    raw,
                    processed,
                    seq,
                };
                if let Err(e) = app.emit("input", frame) {
                    log::debug!("input emit failed: {e}");
                }
            }
        }
        keys::send(&chords.release_all());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_frame_lengths() {
        let r = [0xfc, 0xf1, 0x87, 0x80, 0x00, 0x99, 0x00];
        assert_eq!(fallback_frame(&r), Some(r));
        let mut with_id = vec![0];
        with_id.extend_from_slice(&r);
        assert_eq!(fallback_frame(&with_id), Some(r));
        with_id[0] = 1;
        assert_eq!(fallback_frame(&with_id), None);
        assert_eq!(fallback_frame(&r[..6]), None);
    }

    #[test]
    fn coalescer_caps_rate_and_flushes_last_frame() {
        let t0 = Instant::now();
        let ms = |n| t0 + Duration::from_millis(n);
        let mut c = Coalescer::default();
        assert!(!c.due(t0));
        c.mark();
        assert!(c.due(t0));
        c.mark();
        assert!(!c.due(ms(10)));
        c.mark();
        assert!(c.due(ms(17)));
        assert!(!c.due(ms(40)));
        // 100 Hz input for one second emits at most 60 frames.
        let mut c = Coalescer::default();
        let emitted = (0..100)
            .filter(|i| {
                c.mark();
                c.due(ms(i * 10))
            })
            .count();
        assert!(emitted <= 60, "{emitted}");
    }
}
