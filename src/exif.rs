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

/// When and with which focal length a photo was taken (US-12).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Capture {
    /// Seconds on the camera's clock (a monotonic scale, not tied to a time zone).
    pub time: f64,
    /// Focal length in millimetres; `None` when the lens doesn't report it (manual lens).
    pub focal_mm: Option<f64>,
}

pub fn capture(path: &Path) -> Result<Capture> {
    let file =
        std::fs::File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    let exif = exif::Reader::new()
        .read_from_container(&mut std::io::BufReader::new(file))
        .with_context(|| format!("cannot read EXIF of {}", path.display()))?;
    let text = |tag: Tag| -> Option<String> {
        let f = exif.get_field(tag, In::PRIMARY)?;
        match &f.value {
            Value::Ascii(v) => v
                .first()
                .map(|b| String::from_utf8_lossy(b).trim().to_string()),
            _ => None,
        }
    };
    let when = text(Tag::DateTimeOriginal)
        .with_context(|| format!("{}: EXIF has no capture time", path.display()))?;
    let mut time = clock_seconds(&when)
        .with_context(|| format!("{}: unreadable capture time {when:?}", path.display()))?;
    if let Some(sub) = text(Tag::SubSecTimeOriginal)
        .filter(|s| s.chars().all(|c| c.is_ascii_digit()) && !s.is_empty())
    {
        time += format!("0.{sub}").parse::<f64>().unwrap_or(0.0);
    }
    let focal_mm = exif
        .get_field(Tag::FocalLength, In::PRIMARY)
        .and_then(|f| match &f.value {
            Value::Rational(v) => v.first().map(|r| r.to_f64()),
            _ => None,
        })
        .filter(|f| f.is_finite() && *f > 0.0);
    Ok(Capture { time, focal_mm })
}

/// "YYYY:MM:DD HH:MM:SS" as seconds since 1970-01-01 on the same clock.
fn clock_seconds(s: &str) -> Option<f64> {
    let n: Vec<i64> = s
        .split([':', ' '])
        .map(|x| x.parse().ok())
        .collect::<Option<_>>()?;
    let [y, mo, d, h, mi, se] = n[..] else {
        return None;
    };
    // days from civil (Howard Hinnant's algorithm)
    let y = if mo <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (mo + if mo > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some((days * 86_400 + h * 3600 + mi * 60 + se) as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_times_on_one_clock() {
        assert_eq!(clock_seconds("1970:01:01 00:00:10"), Some(10.0));
        assert_eq!(clock_seconds("2000:03:01 00:00:00"), Some(951_868_800.0));
        let a = clock_seconds("2024:12:31 23:59:55").unwrap();
        let b = clock_seconds("2025:01:01 00:00:05").unwrap();
        assert_eq!(b - a, 10.0);
        assert_eq!(clock_seconds("2024:13"), None);
    }

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
