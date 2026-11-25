//! Aurora or not, for night photos (US-7): colour structure in the sky of a small check render.
//!
//! The photo is rendered small with a fixed, neutral check edit. In the upper part of the frame
//! the colour (chromaticity) of each block of pixels is compared with a smooth fit across the
//! sky: light pollution, twilight and moonlight change the sky's colour smoothly, aurora comes
//! in bands and curtains. Fitted on the owner's 130 night photos (journal 2026-11-25).

use anyhow::{Context, Result, bail};
use std::path::Path;

/// The check edit, handed to darktable-cli with the photo: linear (no tone mapping), fixed white
/// balance, no denoise; darktable adds the camera-specific raw preparation itself. The threshold
/// is fitted to renders with exactly this edit, so changing it means re-fitting. (darktable-cli
/// refuses XMP files with comments, hence the note here.)
pub const CHECK_XMP: &str = include_str!("../data/aurora-check.xmp");
/// Longest edge of the check render in pixels; the threshold depends on it.
pub const RENDER_SIZE: u32 = 300;
/// Sky colour structure at or above this means aurora. The highest non-aurora photo scored
/// 0.0102, the lowest clear aurora 0.0103; the margin sends borderline photos to the owner.
pub const MIN_SKY_STRUCTURE: f64 = 0.011;
/// Share of the frame, from the top, that counts as sky.
const SKY: f64 = 0.55;
/// Block size in pixels.
const BLOCK: usize = 10;

/// A linear RGB image, rows from top to bottom.
pub struct Image {
    pub width: usize,
    pub height: usize,
    pub rgb: Vec<f32>,
}

impl Image {
    fn pixel(&self, x: usize, y: usize) -> [f32; 3] {
        let k = 3 * (y * self.width + x);
        [self.rgb[k], self.rgb[k + 1], self.rgb[k + 2]]
    }
}

/// Reads a colour PFM as darktable-cli writes it.
pub fn load_pfm(path: &Path) -> Result<Image> {
    let data = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    parse_pfm(&data).with_context(|| format!("cannot read {} as PFM", path.display()))
}

fn parse_pfm(data: &[u8]) -> Result<Image> {
    // three header lines: "PF", "<width> <height>", "<scale>" (negative: little-endian)
    let mut lines = Vec::new();
    let mut start = 0;
    for (i, &b) in data.iter().enumerate() {
        if b == b'\n' {
            lines.push(std::str::from_utf8(&data[start..i])?.trim().to_string());
            start = i + 1;
            if lines.len() == 3 {
                break;
            }
        }
    }
    if lines.len() < 3 || lines[0] != "PF" {
        bail!("not a colour PFM");
    }
    let mut dims = lines[1].split_whitespace().map(str::parse::<usize>);
    let (Some(Ok(width)), Some(Ok(height))) = (dims.next(), dims.next()) else {
        bail!("bad size line {:?}", lines[1]);
    };
    let little = lines[2].parse::<f64>()? < 0.0;
    let body = &data[start..];
    if body.len() < width * height * 12 {
        bail!("truncated");
    }
    let mut rgb = vec![0f32; width * height * 3];
    for y in 0..height {
        // PFM stores the bottom row first
        let row = height - 1 - y;
        for i in 0..width * 3 {
            let k = 4 * (row * width * 3 + i);
            let b = [body[k], body[k + 1], body[k + 2], body[k + 3]];
            rgb[y * width * 3 + i] = if little {
                f32::from_le_bytes(b)
            } else {
                f32::from_be_bytes(b)
            };
        }
    }
    Ok(Image { width, height, rgb })
}

/// Colour structure of the sky: the 90th percentile of how far each block's green share
/// departs from a smooth (quadratic) fit across the sky.
pub fn sky_structure(img: &Image) -> f64 {
    let sky_rows = img.height - (((1.0 - SKY) * img.height as f64) as usize);
    let (bx, by) = (img.width / BLOCK, sky_rows / BLOCK);
    // (u, v, green share) per block; blocks are counted from the bottom of the sky
    let mut blocks = Vec::new();
    for j in 0..by {
        let y1 = sky_rows - j * BLOCK;
        for i in 0..bx {
            let mut s = [0f64; 3];
            for y in y1 - BLOCK..y1 {
                for x in i * BLOCK..(i + 1) * BLOCK {
                    let p = img.pixel(x, y);
                    for c in 0..3 {
                        s[c] += f64::from(p[c].max(0.0));
                    }
                }
            }
            let t = s[0] + s[1] + s[2];
            if t > 0.0 {
                blocks.push((i as f64 / bx as f64, j as f64 / by as f64, s[1] / t));
            }
        }
    }
    if blocks.len() < 12 {
        return 0.0;
    }
    let basis = |u: f64, v: f64| [1.0, u, v, u * u, v * v, u * v];
    let coef = least_squares(blocks.iter().map(|&(u, v, g)| (basis(u, v), g)));
    let mut res: Vec<f64> = blocks
        .iter()
        .map(|&(u, v, g)| {
            let fit: f64 = basis(u, v).iter().zip(&coef).map(|(b, c)| b * c).sum();
            (g - fit).abs()
        })
        .collect();
    res.sort_by(f64::total_cmp);
    res[(0.9 * res.len() as f64) as usize]
}

pub fn has_aurora(img: &Image) -> bool {
    sky_structure(img) >= MIN_SKY_STRUCTURE
}

/// Least-squares coefficients for rows of (basis values, target), via the normal equations.
fn least_squares(rows: impl Iterator<Item = ([f64; 6], f64)>) -> [f64; 6] {
    const N: usize = 6;
    let mut m = [[0f64; N]; N];
    let mut v = [0f64; N];
    for (x, y) in rows {
        for a in 0..N {
            for b in 0..N {
                m[a][b] += x[a] * x[b];
            }
            v[a] += x[a] * y;
        }
    }
    for a in 0..N {
        let piv = (a..N)
            .max_by(|&r, &s| m[r][a].abs().total_cmp(&m[s][a].abs()))
            .unwrap();
        m.swap(a, piv);
        v.swap(a, piv);
        if m[a][a] == 0.0 {
            continue;
        }
        for r in 0..N {
            if r != a {
                let f = m[r][a] / m[a][a];
                let pivot_row = m[a];
                for (x, p) in m[r].iter_mut().zip(pivot_row) {
                    *x -= f * p;
                }
                v[r] -= f * v[a];
            }
        }
    }
    std::array::from_fn(|a| if m[a][a] != 0.0 { v[a] / m[a][a] } else { 0.0 })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 300×200 sky whose colour is `f(x, y)` (fractions of width and height).
    fn sky(f: impl Fn(f64, f64) -> [f32; 3]) -> Image {
        let (width, height) = (300, 200);
        let mut rgb = Vec::with_capacity(width * height * 3);
        for y in 0..height {
            for x in 0..width {
                rgb.extend(f(x as f64 / width as f64, y as f64 / height as f64));
            }
        }
        Image { width, height, rgb }
    }

    #[test]
    fn smooth_light_pollution_is_no_aurora() {
        // yellow towards the horizon, bluer above: a smooth change of colour
        let img = sky(|_, y| {
            let t = y as f32;
            [0.2 + 0.2 * t, 0.2 + 0.15 * t, 0.3 - 0.1 * t]
        });
        assert!(sky_structure(&img) < 0.001, "{}", sky_structure(&img));
        assert!(!has_aurora(&img));
    }

    #[test]
    fn green_band_is_aurora() {
        // a diagonal green band across a dark blue sky
        let img = sky(|x, y| {
            let band = ((y - 0.1 - 0.3 * x).abs() < 0.06) as u8 as f32;
            [0.10, 0.12 + 0.25 * band, 0.18]
        });
        assert!(has_aurora(&img), "{}", sky_structure(&img));
    }

    #[test]
    fn pfm_rows_are_turned_top_down() {
        // 1×2 image: bottom row stored first
        let mut data = b"PF\n1 2\n-1.0\n".to_vec();
        for v in [0.0f32, 0.0, 0.0, 1.0, 1.0, 1.0] {
            data.extend(v.to_le_bytes());
        }
        let img = parse_pfm(&data).unwrap();
        assert_eq!(img.pixel(0, 0), [1.0, 1.0, 1.0]);
        assert_eq!(img.pixel(0, 1), [0.0, 0.0, 0.0]);
    }
}
