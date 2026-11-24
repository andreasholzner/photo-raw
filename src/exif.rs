//! Exposure data from a raw file's EXIF.

use anyhow::{Context, Result, bail};
use exif::{In, Tag, Value};
use std::path::Path;

/// How a photo was exposed, as recorded by the camera.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Exposure {
    /// Exposure time in seconds.
    pub time: f64,
    /// f-number; `None` when the lens doesn't report one (manual lens).
    pub aperture: Option<f64>,
    pub iso: f64,
}

pub fn read(path: &Path) -> Result<Exposure> {
    let file =
        std::fs::File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    let exif = exif::Reader::new()
        .read_from_container(&mut std::io::BufReader::new(file))
        .with_context(|| format!("cannot read EXIF of {}", path.display()))?;
    let number = |tag: Tag| -> Option<f64> {
        let field = exif.get_field(tag, In::PRIMARY)?;
        match &field.value {
            Value::Rational(v) => v.first().map(|r| r.to_f64()),
            Value::SRational(v) => v.first().map(|r| r.to_f64()),
            Value::Short(v) => v.first().map(|&x| f64::from(x)),
            Value::Long(v) => v.first().map(|&x| f64::from(x)),
            _ => None,
        }
        .filter(|x| x.is_finite())
    };
    let time = number(Tag::ExposureTime).filter(|&t| t > 0.0);
    let iso = number(Tag::PhotographicSensitivity).filter(|&i| i > 0.0);
    let (Some(time), Some(iso)) = (time, iso) else {
        bail!("{}: EXIF has no exposure time or ISO", path.display());
    };
    let aperture = number(Tag::FNumber).filter(|&f| f > 0.0);
    Ok(Exposure {
        time,
        aperture,
        iso,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compares with exiftool on real raws: `PHOTOFLOW_TEST_RAWS=<dir> cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn matches_exiftool_on_real_raws() {
        let dir = std::env::var("PHOTOFLOW_TEST_RAWS").expect("set PHOTOFLOW_TEST_RAWS");
        let mut n = 0;
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "ARW") {
                continue;
            }
            let out = std::process::Command::new("exiftool")
                .args(["-T", "-n", "-ExposureTime", "-FNumber", "-ISO"])
                .arg(&path)
                .output()
                .unwrap();
            let text = String::from_utf8(out.stdout).unwrap();
            let v: Vec<f64> = text
                .trim()
                .split('\t')
                .map(|x| x.parse().unwrap_or(0.0))
                .collect();
            let e = read(&path).unwrap();
            assert!(
                (e.time - v[0]).abs() < 1e-6 * v[0].max(1.0),
                "{}",
                path.display()
            );
            assert_eq!(e.aperture.unwrap_or(0.0), v[1], "{}", path.display());
            assert_eq!(e.iso, v[2], "{}", path.display());
            n += 1;
        }
        assert!(n > 0);
    }
}
