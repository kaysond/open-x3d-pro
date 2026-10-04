//! Tracks raw min/max per axis between `start_calibration` and `finish_calibration`.

use x3d_core::config::{AxisId, Calibration};
use x3d_core::pipeline::RawReport;

#[derive(Debug, Clone, Copy)]
struct Track {
    min: u16,
    max: u16,
    last: Option<u16>,
}

#[derive(Debug, Default)]
pub struct Calibrator {
    tracks: [Option<Track>; 4],
}

impl Calibrator {
    pub fn start(&mut self, axis: AxisId) {
        self.tracks[axis.index()] = Some(Track {
            min: u16::MAX,
            max: 0,
            last: None,
        });
    }

    pub fn update(&mut self, raw: &RawReport) {
        for (track, v) in self.tracks.iter_mut().zip(raw.raw16()) {
            if let Some(t) = track {
                t.min = t.min.min(v);
                t.max = t.max.max(v);
                t.last = Some(v);
            }
        }
    }

    /// Min/max seen since `start`, center = the value at finish.
    pub fn finish(&mut self, axis: AxisId) -> Result<Calibration, &'static str> {
        let t = self.tracks[axis.index()]
            .take()
            .ok_or("calibration was not started for this axis")?;
        let center = t.last.ok_or("no input received from the joystick")?;
        if t.min >= t.max {
            return Err("the axis did not move; sweep it through its full range");
        }
        Ok(Calibration {
            min: t.min,
            center,
            max: t.max,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(x: u16, slider: u8) -> RawReport {
        RawReport {
            x,
            slider,
            ..RawReport::default()
        }
    }

    #[test]
    fn tracks_min_max_and_center() {
        let mut c = Calibrator::default();
        assert!(c.finish(AxisId::X).is_err());
        c.start(AxisId::X);
        assert_eq!(
            c.finish(AxisId::X),
            Err("no input received from the joystick")
        );
        c.start(AxisId::X);
        c.update(&raw(512, 0));
        assert!(c.finish(AxisId::X).is_err());
        c.start(AxisId::X);
        c.start(AxisId::Slider);
        for (x, s) in [(512, 10), (3, 200), (1020, 40), (509, 0)] {
            c.update(&raw(x, s));
        }
        assert_eq!(
            c.finish(AxisId::X),
            Ok(Calibration {
                min: 3 << 6,
                center: 509 << 6,
                max: 1020 << 6
            })
        );
        assert_eq!(
            c.finish(AxisId::Slider),
            Ok(Calibration {
                min: 0,
                center: 0,
                max: 200 << 8
            })
        );
    }
}
