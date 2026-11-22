//! Comparing two 8-bit PNG renders.

use anyhow::{Context, Result, bail};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Diff {
    /// Mean absolute difference over all pixels and channels, in 8-bit levels.
    pub mean: f64,
    /// 99th percentile of the absolute difference, in 8-bit levels.
    pub p99: u8,
    pub max: u8,
}

pub struct Rgb {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

pub fn load_png(path: &Path) -> Result<Rgb> {
    let file =
        std::fs::File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder
        .read_info()
        .with_context(|| format!("cannot read {}", path.display()))?;
    let mut buf = vec![0; reader.output_buffer_size().context("image too large")?];
    let info = reader.next_frame(&mut buf)?;
    buf.truncate(info.buffer_size());
    let data = match info.color_type {
        png::ColorType::Rgb => buf,
        png::ColorType::Rgba => buf
            .chunks_exact(4)
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g]).collect(),
        png::ColorType::GrayscaleAlpha => buf
            .chunks_exact(2)
            .flat_map(|p| [p[0], p[0], p[0]])
            .collect(),
        other => bail!("{}: unsupported PNG colour type {other:?}", path.display()),
    };
    Ok(Rgb {
        width: info.width,
        height: info.height,
        data,
    })
}

/// `None` if the images differ in size.
pub fn diff(a: &Rgb, b: &Rgb) -> Option<Diff> {
    if a.width != b.width || a.height != b.height {
        return None;
    }
    let mut hist = [0u64; 256];
    for (x, y) in a.data.iter().zip(&b.data) {
        hist[x.abs_diff(*y) as usize] += 1;
    }
    let n = a.data.len() as u64;
    if n == 0 {
        return Some(Diff {
            mean: 0.0,
            p99: 0,
            max: 0,
        });
    }
    let sum: u64 = hist.iter().enumerate().map(|(d, c)| d as u64 * c).sum();
    let target = (n * 99).div_ceil(100);
    let mut acc = 0;
    let mut p99 = 0;
    for (d, c) in hist.iter().enumerate() {
        acc += c;
        if acc >= target {
            p99 = d as u8;
            break;
        }
    }
    let max = hist.iter().rposition(|&c| c > 0).unwrap_or(0) as u8;
    Some(Diff {
        mean: sum as f64 / n as f64,
        p99,
        max,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(w: u32, h: u32, v: u8) -> Rgb {
        Rgb {
            width: w,
            height: h,
            data: vec![v; (w * h * 3) as usize],
        }
    }

    #[test]
    fn identical_images() {
        assert_eq!(
            diff(&img(4, 4, 10), &img(4, 4, 10)),
            Some(Diff {
                mean: 0.0,
                p99: 0,
                max: 0
            })
        );
    }

    #[test]
    fn size_mismatch() {
        assert_eq!(diff(&img(4, 4, 0), &img(4, 5, 0)), None);
    }

    #[test]
    fn statistics() {
        let a = img(10, 10, 100);
        let mut b = img(10, 10, 100);
        b.data[0] = 200; // one channel of one pixel far off
        let d = diff(&a, &b).unwrap();
        assert_eq!(d.max, 100);
        assert_eq!(d.p99, 0);
        assert!((d.mean - 100.0 / 300.0).abs() < 1e-9);
    }

    #[test]
    fn png_roundtrip() {
        let p = std::env::temp_dir().join(format!("photoflow-test-{}.png", std::process::id()));
        {
            let f = std::fs::File::create(&p).unwrap();
            let mut e = png::Encoder::new(f, 2, 1);
            e.set_color(png::ColorType::Rgba);
            e.set_depth(png::BitDepth::Eight);
            e.write_header()
                .unwrap()
                .write_image_data(&[1, 2, 3, 255, 4, 5, 6, 255])
                .unwrap();
        }
        let img = load_png(&p).unwrap();
        assert_eq!(
            (img.width, img.height, img.data),
            (2, 1, vec![1, 2, 3, 4, 5, 6])
        );
        std::fs::remove_file(p).unwrap();
    }
}
