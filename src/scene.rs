//! Day, night or unclear, from the scene brightness in the EXIF (US-7).

use crate::exif::Exposure;

/// Scene brightness at or below this EV100 is night.
pub const NIGHT_MAX_EV100: f64 = 1.0;
/// Scene brightness at or above this EV100 is day.
pub const DAY_MIN_EV100: f64 = 8.0;
/// The owner's only lens without an f-number in the EXIF is a manual 12 mm f/2.0, used for
/// night shots, almost always wide open.
const MANUAL_LENS_APERTURE: f64 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scene {
    Day,
    Night,
    /// Between night and day: becomes a question for the owner.
    Unclear,
}

/// Scene brightness: the exposure value at ISO 100.
pub fn ev100(e: &Exposure) -> f64 {
    let f = e.aperture.unwrap_or(MANUAL_LENS_APERTURE);
    (f * f / e.time).log2() - (e.iso / 100.0).log2()
}

pub fn classify(e: &Exposure) -> Scene {
    let ev = ev100(e);
    if ev <= NIGHT_MAX_EV100 {
        Scene::Night
    } else if ev >= DAY_MIN_EV100 {
        Scene::Day
    } else {
        Scene::Unclear
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exp(time: f64, aperture: Option<f64>, iso: f64) -> Exposure {
        Exposure {
            time,
            aperture,
            iso,
        }
    }

    #[test]
    fn ev100_of_known_exposures() {
        // sunny 16: f/16, 1/100 s, ISO 100 ≈ EV 14.6
        assert!((ev100(&exp(0.01, Some(16.0), 100.0)) - 14.644).abs() < 0.01);
        // the manual lens counts as f/2.0: 4 s at ISO 400 → log2(1) − 2 = −2
        assert!((ev100(&exp(4.0, None, 400.0)) + 2.0).abs() < 1e-9);
    }

    #[test]
    fn thresholds() {
        assert_eq!(classify(&exp(0.01, Some(16.0), 100.0)), Scene::Day);
        assert_eq!(classify(&exp(4.0, None, 400.0)), Scene::Night);
        // f/4, 1/30 s, ISO 800: EV100 ≈ 5.9 → dusk, unclear
        assert_eq!(classify(&exp(1.0 / 30.0, Some(4.0), 800.0)), Scene::Unclear);
        // exactly on the limits: f/2, 2 s, ISO 100 → EV100 1; f/16, 1 s, ISO 100 → EV100 8
        assert_eq!(classify(&exp(2.0, Some(2.0), 100.0)), Scene::Night);
        assert_eq!(classify(&exp(1.0, Some(16.0), 100.0)), Scene::Day);
    }
}
