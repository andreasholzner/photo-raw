//! Sorting a photo into day, aurora or a question for the owner (US-7).

use crate::aurora::{self, Image};
use crate::darktable::Darktable;
use crate::exif::Exposure;
use crate::scene::{self, Scene};
use anyhow::Result;
use std::path::{Path, PathBuf};

/// Something only the owner can decide; it becomes an open step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Question {
    DayOrNight,
}

impl Question {
    /// The open step that lists the photo.
    pub fn open_step(self) -> &'static str {
        match self {
            Question::DayOrNight => "day or night?",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Day,
    /// A night photo without aurora (stars, moonlight, city lights).
    Night,
    Aurora,
    Ask(Question),
}

/// Classifies a photo by its exposure; night photos are checked for aurora on a check render,
/// which is only made for them.
pub fn classify(
    exposure: &Exposure,
    check_render: impl FnOnce() -> Result<Image>,
) -> Result<Class> {
    Ok(match scene::classify(exposure) {
        Scene::Day => Class::Day,
        Scene::Unclear => Class::Ask(Question::DayOrNight),
        Scene::Night if aurora::has_aurora(&check_render()?) => Class::Aurora,
        Scene::Night => Class::Night,
    })
}

/// Makes the aurora check renders with darktable-cli.
pub struct AuroraCheck {
    dt: Darktable,
    xmp: PathBuf,
    out: PathBuf,
}

impl AuroraCheck {
    /// `work` holds the check edit and the renders.
    pub fn new(dt: Darktable, work: &Path) -> Result<Self> {
        std::fs::create_dir_all(work)?;
        let xmp = work.join("aurora-check.xmp");
        std::fs::write(&xmp, aurora::CHECK_XMP)?;
        Ok(Self {
            dt,
            xmp,
            out: work.to_path_buf(),
        })
    }

    pub fn render(&self, raw: &Path) -> Result<Image> {
        let name = raw.file_name().unwrap_or_default().to_string_lossy();
        let out = self.out.join(format!("{name}.pfm"));
        self.dt.render(raw, &self.xmp, &out, aurora::RENDER_SIZE)?;
        aurora::load_pfm(&out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exp(ev100: f64) -> Exposure {
        Exposure {
            time: 1.0,
            aperture: Some(2f64.powf(ev100 / 2.0)),
            iso: 100.0,
        }
    }

    fn flat() -> Result<Image> {
        Ok(Image {
            width: 300,
            height: 200,
            rgb: vec![0.1; 300 * 200 * 3],
        })
    }

    #[test]
    fn only_night_photos_are_rendered() {
        let never = || -> Result<Image> { panic!("rendered a photo that isn't night") };
        assert_eq!(classify(&exp(14.0), never).unwrap(), Class::Day);
        assert_eq!(
            classify(&exp(5.0), never).unwrap(),
            Class::Ask(Question::DayOrNight)
        );
        assert_eq!(classify(&exp(-2.0), flat).unwrap(), Class::Night);
    }
}
