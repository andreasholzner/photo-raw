//! Where the land meets the sky (US-23), from a sky segmentation model.
//!
//! The model (SkyWater-Seg: SegFormer fine-tuned on ADE20K for sky, water and people; MIT
//! licence) is downloaded on first use and run on the CPU with tract. It sees the photo twice,
//! from two small check renders: the 300 px render keeps a clear Milky Way sky "sky", the 768 px
//! render averaged down keeps an aurora that reaches the land "sky" (journal 2026-11-27). Each is
//! shown as colour, as greyscale and darker; a pixel is sky when any of the six views says so.
//! Bright aurora the model takes for cloud stays harmless above the land, because the skyline is
//! found from the bottom up.

use crate::aurora::Image;
use anyhow::{Context, Result, bail, ensure};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;
use tract_onnx::prelude::*;

/// The model file, pinned to a revision of its repository, and its checksum.
const MODEL_URL: &str = "https://huggingface.co/Realcat/skywater_seg/resolve/\
dac255883ec5faf508561a47172096bfd8708db0/skywater_segformer_b2_fp32.onnx";
const MODEL_SHA256: &str = "e4e9a6927c2d910c3243f86e392b18da715b41c03e6e6f41672f8f6b8eaa71b5";
/// The model's input size and its sky class.
const INPUT: usize = 384;
const SKY_CLASS: usize = 1;
/// Longest edges of the two check renders.
pub const RENDER_SIZES: [u32; 2] = [300, 768];
/// Display mappings: the luminance at this percentile becomes white.
const BRIGHT: f32 = 0.995;
const DARK: f32 = 0.9999;
/// A pixel is sky at this probability.
const SKY: f32 = 0.5;
/// Sky and land count from this much of the frame height on (smaller specks are noise).
const MIN_PART: f32 = 0.01;

/// The skyline: per column of the model's grid, from left to right, the height (fraction of the
/// frame from the top) where the land starts; 1.0 where the column is sky to the bottom, None
/// where it has no sky at all.
#[derive(Debug, Clone, PartialEq)]
pub struct Skyline {
    pub columns: Vec<Option<f32>>,
}

impl Skyline {
    /// From a sky probability grid (rows from the top): scanning up from the bottom, the first
    /// stretch of sky in each column ([`MIN_PART`] of the height). Sky the model missed above it
    /// (bright aurora) doesn't matter.
    pub fn from_sky(width: usize, height: usize, sky: &[f32]) -> Self {
        let run = ((height as f32 * MIN_PART).round() as usize).max(1);
        let columns = (0..width)
            .map(|x| {
                (run - 1..height)
                    .rev()
                    .find(|&y| (y + 1 - run..=y).all(|yy| sky[yy * width + x] >= SKY))
                    .map(|y| (y + 1) as f32 / height as f32)
            })
            .collect();
        Self { columns }
    }

    /// Whether any column has land below its sky ([`MIN_PART`] of the height or more).
    pub fn has_land(&self) -> bool {
        self.columns
            .iter()
            .any(|c| c.is_some_and(|y| y <= 1.0 - MIN_PART))
    }

    /// The middle height of the skyline (median over the columns with sky), or None without sky.
    pub fn middle(&self) -> Option<f32> {
        let mut v: Vec<f32> = self.columns.iter().flatten().copied().collect();
        v.sort_by(f32::total_cmp);
        v.get(v.len() / 2).copied()
    }

    /// Highest and lowest point of the skyline (fractions from the top).
    pub fn extent(&self) -> Option<(f32, f32)> {
        let v = self.columns.iter().flatten();
        let lo = v.clone().copied().reduce(f32::min)?;
        let hi = v.copied().reduce(f32::max)?;
        Some((lo, hi))
    }
}

/// The model, ready to run.
pub struct SkyModel {
    plan: TypedRunnableModel<TypedModel>,
}

impl SkyModel {
    /// Loads the model from `dir`, downloading it there first if needed.
    pub fn load(dir: &Path) -> Result<Self> {
        let path = fetch(dir)?;
        let plan = tract_onnx::onnx()
            .with_ignore_output_shapes(true)
            .model_for_path(&path)
            .with_context(|| format!("cannot read the sky model {}", path.display()))?
            .with_input_fact(0, f32::fact([1, 3, INPUT, INPUT]).into())?
            .into_optimized()?
            .into_runnable()?;
        Ok(Self { plan })
    }

    /// Sky probabilities for one view of the photo: (width, height, values, rows from the top).
    fn sky(&self, view: &Tensor) -> Result<(usize, usize, Vec<f32>)> {
        let out = self.plan.run(tvec!(view.clone().into()))?;
        let o = out[0].to_array_view::<f32>()?;
        let s = o.shape();
        ensure!(
            s.len() == 4 && s[1] > SKY_CLASS,
            "unexpected model output {s:?}"
        );
        let (h, w) = (s[2], s[3]);
        let mut p = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                let l: Vec<f32> = (0..s[1]).map(|c| o[[0, c, y, x]]).collect();
                let m = l.iter().copied().fold(f32::MIN, f32::max);
                let sum: f32 = l.iter().map(|v| (v - m).exp()).sum();
                p.push((l[SKY_CLASS] - m).exp() / sum);
            }
        }
        Ok((w, h, p))
    }

    /// The skyline from the two check renders (small, large).
    pub fn skyline(&self, small: &Image, large: &Image) -> Result<Skyline> {
        let mut union: Option<(usize, usize, Vec<f32>)> = None;
        for img in [small, large] {
            for (gray, pct) in [(false, BRIGHT), (true, BRIGHT), (false, DARK)] {
                let (w, h, p) = self.sky(&view(img, gray, pct))?;
                match &mut union {
                    None => union = Some((w, h, p)),
                    Some((uw, uh, u)) => {
                        ensure!(*uw == w && *uh == h, "model output size changed");
                        u.iter_mut().zip(p).for_each(|(a, b)| *a = a.max(b));
                    }
                }
            }
        }
        let (w, h, u) = union.context("no views")?;
        Ok(Skyline::from_sky(w, h, &u))
    }
}

/// One view of the photo for the model: brightened so the luminance at `pct` is white, sRGB,
/// optionally greyscale, resampled to the input grid (area average when shrinking, nearest pixel
/// when enlarging), normalised as the model was trained.
fn view(img: &Image, gray: bool, pct: f32) -> Tensor {
    let (w, h) = (img.width, img.height);
    let lum = |k: usize| 0.2126 * img.rgb[k] + 0.7152 * img.rgb[k + 1] + 0.0722 * img.rgb[k + 2];
    let mut l: Vec<f32> = (0..w * h).map(|i| lum(3 * i)).collect();
    l.sort_by(f32::total_cmp);
    let top = l[((l.len() as f32 * pct) as usize).min(l.len() - 1)].max(1e-6);
    let srgb = |v: f32| {
        let v = (v / top).clamp(0.0, 1.0);
        if v <= 0.003_130_8 {
            12.92 * v
        } else {
            1.055 * v.powf(1.0 / 2.4) - 0.055
        }
    };
    let (mean, std) = ([0.485, 0.456, 0.406], [0.229, 0.224, 0.225]);
    tract_ndarray::Array4::from_shape_fn((1, 3, INPUT, INPUT), |(_, c, y, x)| {
        let (x0, x1) = (x * w / INPUT, ((x + 1) * w / INPUT).max(x * w / INPUT + 1));
        let (y0, y1) = (y * h / INPUT, ((y + 1) * h / INPUT).max(y * h / INPUT + 1));
        let mut s = 0.0;
        for yy in y0..y1 {
            for xx in x0..x1 {
                let k = 3 * (yy * w + xx);
                s += srgb(if gray { lum(k) } else { img.rgb[k + c] });
            }
        }
        (s / ((x1 - x0) * (y1 - y0)) as f32 - mean[c]) / std[c]
    })
    .into()
}

/// The model file in `dir`, prepared for tract: downloaded, checked against its checksum and with
/// its stored intermediate shapes removed (they name a symbolic batch size tract can't unify).
fn fetch(dir: &Path) -> Result<PathBuf> {
    let ready = dir.join("sky-segmentation.onnx");
    if ready.exists() {
        return Ok(ready);
    }
    std::fs::create_dir_all(dir)?;
    let download = dir.join("sky-segmentation.download");
    eprintln!("Downloading the sky segmentation model (95 MB) …");
    let status = Command::new("curl")
        .args(["-fsSL", "-o"])
        .arg(&download)
        .arg(MODEL_URL)
        .status()
        .context("cannot run curl to download the sky segmentation model")?;
    ensure!(
        status.success(),
        "downloading the sky model from {MODEL_URL} failed"
    );
    let data = std::fs::read(&download)?;
    let sum = format!("{:x}", Sha256::digest(&data));
    if sum != MODEL_SHA256 {
        let _ = std::fs::remove_file(&download);
        bail!("the downloaded sky model has checksum {sum}, expected {MODEL_SHA256}");
    }
    let tmp = dir.join("sky-segmentation.onnx.tmp");
    std::fs::write(&tmp, strip_value_info(&data)?)?;
    std::fs::rename(&tmp, &ready)?;
    std::fs::remove_file(&download)?;
    Ok(ready)
}

// --- minimal protobuf: drop GraphProto.value_info (field 13) inside ModelProto.graph (field 7) ---

fn varint(b: &[u8], i: &mut usize) -> Result<u64> {
    let mut r = 0u64;
    for shift in (0..64).step_by(7) {
        let c = *b.get(*i).context("truncated protobuf")?;
        *i += 1;
        r |= u64::from(c & 0x7f) << shift;
        if c < 0x80 {
            return Ok(r);
        }
    }
    bail!("bad varint")
}

fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let c = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(c);
            return;
        }
        out.push(c | 0x80);
    }
}

/// A protobuf field: number, wire type, the whole field's bytes, its payload.
type Field<'a> = (u64, u64, &'a [u8], &'a [u8]);

/// The fields of a protobuf message.
fn fields(b: &[u8]) -> Result<Vec<Field<'_>>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let start = i;
        let key = varint(b, &mut i)?;
        let (f, t) = (key >> 3, key & 7);
        let body = i;
        match t {
            0 => {
                varint(b, &mut i)?;
            }
            1 => i += 8,
            5 => i += 4,
            2 => {
                let n = varint(b, &mut i)? as usize;
                i += n;
            }
            _ => bail!("unsupported wire type {t}"),
        }
        ensure!(i <= b.len(), "truncated protobuf");
        out.push((f, t, &b[start..i], &b[body..i]));
    }
    Ok(out)
}

fn strip_value_info(model: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(model.len());
    for (f, t, whole, payload) in fields(model)? {
        if f == 7 && t == 2 {
            let mut i = 0;
            let n = varint(payload, &mut i)? as usize;
            let graph = &payload[i..i + n];
            let mut g = Vec::with_capacity(graph.len());
            for (gf, _, gwhole, _) in fields(graph)? {
                if gf != 13 {
                    g.extend_from_slice(gwhole);
                }
            }
            put_varint(&mut out, (7 << 3) | 2);
            put_varint(&mut out, g.len() as u64);
            out.extend_from_slice(&g);
        } else {
            out.extend_from_slice(whole);
        }
    }
    Ok(out)
}

/// Finds skylines of raw photos: the check renders with darktable-cli, then the model.
pub struct Finder {
    dt: crate::darktable::Darktable,
    xmp: PathBuf,
    work: PathBuf,
    model: SkyModel,
}

impl Finder {
    /// `work` holds the check edit and the renders; the model lives in `models`.
    pub fn new(dt: crate::darktable::Darktable, work: &Path, models: &Path) -> Result<Self> {
        std::fs::create_dir_all(work)?;
        let xmp = work.join("horizon-check.xmp");
        std::fs::write(&xmp, crate::aurora::CHECK_XMP)?;
        Ok(Self {
            dt,
            xmp,
            work: work.to_path_buf(),
            model: SkyModel::load(models)?,
        })
    }

    /// The skyline of `raw`, and the large check render it was found on.
    pub fn find(&self, raw: &Path) -> Result<(Skyline, Image)> {
        let name = raw
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let mut renders = Vec::new();
        for size in RENDER_SIZES {
            let out = self.work.join(format!("{name}.{size}.pfm"));
            self.dt.render(raw, &self.xmp, &out, size)?;
            renders.push(crate::aurora::load_pfm(&out)?);
        }
        let large = renders.pop().context("no render")?;
        let small = renders.pop().context("no render")?;
        let mut line = self.model.skyline(&small, &large)?;
        fill_edges(&mut line);
        snap(&mut line, &large);
        Ok((line, large))
    }
}

/// A run of columns at the frame's side, at most this share of the width, whose skyline drops
/// by more than [`EDGE_DROP`] next to the rest is land: the model takes dark, vignetted corners
/// for night sky (US-21).
const EDGE_RUN: f32 = 0.08;
const EDGE_DROP: f32 = 0.15;

fn fill_edges(line: &mut Skyline) {
    let n = line.columns.len();
    let max = (n as f32 * EDGE_RUN) as usize;
    let y = |c: Option<f32>| c.unwrap_or(0.0);
    for from_left in [true, false] {
        let idx = |i: usize| if from_left { i } else { n - 1 - i };
        // the first column (from this side) whose skyline is well above the side's
        let side = y(line.columns[idx(0)]);
        let Some(run) = (1..=max.min(n - 1)).find(|&i| y(line.columns[idx(i)]) < side - EDGE_DROP)
        else {
            continue;
        };
        let neighbour = line.columns[idx(run)];
        for i in 0..run {
            if y(line.columns[idx(i)]) > y(neighbour) + EDGE_DROP {
                line.columns[idx(i)] = neighbour;
            }
        }
    }
}

/// How far (fraction of the frame height) a skyline point may move to the nearest edge.
const SNAP: f32 = 0.012;
/// … and only to an edge this much stronger than the one at the model's point.
const SNAP_GAIN: f32 = 1.5;

/// Moves each point of the skyline to the strongest brightness edge within [`SNAP`] of it, on
/// the large render: the model's grid is coarse, and a mask edge a little below the ridge shows
/// as a bright strip once the land is darkened (US-21).
fn snap(line: &mut Skyline, img: &Image) {
    let (w, h) = (img.width, img.height);
    let n = line.columns.len();
    let lum = |x: usize, y: usize| {
        let k = 3 * (y * w + x);
        (0.2126 * img.rgb[k] + 0.7152 * img.rgb[k + 1] + 0.0722 * img.rgb[k + 2])
            .max(1e-5)
            .ln()
    };
    let k = 2; // rows averaged on each side of an edge
    for (i, c) in line.columns.iter_mut().enumerate() {
        let Some(y) = *c else { continue };
        if y >= 1.0 - 2.0 * SNAP {
            continue; // no land, or only a strip at the bottom
        }
        let (x0, x1) = (i * w / n, ((i + 1) * w / n).max(i * w / n + 1));
        let profile: Vec<f32> = (0..h)
            .map(|yy| (x0..x1).map(|x| lum(x, yy)).sum::<f32>() / (x1 - x0) as f32)
            .collect();
        let centre = (y * h as f32) as isize;
        let reach = (SNAP * h as f32) as isize;
        let step_at = |yy: usize| {
            let above: f32 = profile[yy - k..yy].iter().sum::<f32>() / k as f32;
            let below: f32 = profile[yy..yy + k].iter().sum::<f32>() / k as f32;
            (above - below).abs()
        };
        let (lo, hi) = (k as isize, (h - k) as isize);
        if centre < lo || centre >= hi {
            continue;
        }
        let here = step_at(centre as usize);
        let mut best = (here, centre);
        for yy in (centre - reach).max(lo)..(centre + reach).min(hi) {
            let step = step_at(yy as usize);
            if step > best.0 {
                best = (step, yy);
            }
        }
        if best.0 > SNAP_GAIN * here {
            *c = Some(best.1 as f32 / h as f32);
        }
    }
}

/// A preview for the owner: the large render (brightened, greyscale), sky tinted blue where the
/// skyline says sky, the skyline in yellow.
pub fn preview(img: &Image, line: &Skyline, out: &Path) -> Result<()> {
    let (w, h) = (img.width, img.height);
    let lum = |k: usize| 0.2126 * img.rgb[k] + 0.7152 * img.rgb[k + 1] + 0.0722 * img.rgb[k + 2];
    let mut l: Vec<f32> = (0..w * h).map(|i| lum(3 * i)).collect();
    l.sort_by(f32::total_cmp);
    let top = l[((l.len() as f32 * BRIGHT) as usize).min(l.len() - 1)].max(1e-6);
    let n = line.columns.len();
    let mut px = vec![0u8; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            let g = (lum(3 * (y * w + x)) / top).clamp(0.0, 1.0).powf(1.0 / 2.2);
            let col = line.columns[x * n / w];
            let fy = y as f32 / h as f32;
            let sky = col.is_some_and(|c| fy < c);
            let edge = col.is_some_and(|c| (fy - c).abs() * h as f32 <= 1.0);
            let rgb = if edge {
                [1.0, 0.9, 0.0]
            } else if sky {
                [g * 0.65, g * 0.65, (g + 0.35).min(1.0)]
            } else {
                [g, g, g]
            };
            for c in 0..3 {
                px[3 * (y * w + x) + c] = (rgb[c] * 255.0) as u8;
            }
        }
    }
    let file = std::fs::File::create(out)?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(&px)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 4×4 grid from rows of '#' (sky) and '.' (land).
    fn grid(rows: [&str; 4]) -> Vec<f32> {
        rows.iter()
            .flat_map(|r| r.chars().map(|c| if c == '#' { 1.0 } else { 0.0 }))
            .collect()
    }

    #[test]
    fn the_skyline_is_found_from_the_bottom_up() {
        // column 1: a patch the model didn't call sky (bright aurora) above the land is ignored
        let s = Skyline::from_sky(4, 4, &grid(["##.#", "#.##", "###.", "...."]));
        assert_eq!(
            s.columns,
            vec![Some(0.75), Some(0.75), Some(0.75), Some(0.5)]
        );
        assert!(s.has_land());
        assert_eq!(s.middle(), Some(0.75));
        assert_eq!(s.extent(), Some((0.5, 0.75)));
    }

    #[test]
    fn a_short_drop_at_the_frames_side_is_land() {
        let mut line = Skyline {
            columns: (0..100)
                .map(|i| Some(if i < 5 { 0.9 } else { 0.6 }))
                .collect(),
        };
        fill_edges(&mut line);
        assert_eq!(line.columns[0], Some(0.6));
        // a long run is real sky (a lake shore, the open sea)
        let mut open = Skyline {
            columns: (0..100)
                .map(|i| Some(if i < 30 { 1.0 } else { 0.6 }))
                .collect(),
        };
        fill_edges(&mut open);
        assert_eq!(open.columns[0], Some(1.0));
    }

    #[test]
    fn a_frame_without_land_has_no_skyline_inside_it() {
        let s = Skyline::from_sky(4, 4, &grid(["####", "####", "####", "####"]));
        assert!(!s.has_land());
        let all_land = Skyline::from_sky(4, 4, &grid(["....", "....", "....", "...."]));
        assert_eq!(all_land.middle(), None);
    }

    #[test]
    fn value_info_is_dropped_and_everything_else_kept() {
        // ModelProto { ir_version: 8 (field 1), graph (field 7) { node (1): "n", value_info (13): "v", input (11): "i" } }
        let graph = [&[0x0a, 1, b'n'][..], &[0x6a, 1, b'v'], &[0x5a, 1, b'i']].concat();
        let model = [&[0x08, 8, 0x3a, graph.len() as u8][..], &graph].concat();
        let out = strip_value_info(&model).unwrap();
        assert_eq!(out, vec![0x08, 8, 0x3a, 6, 0x0a, 1, b'n', 0x5a, 1, b'i']);
    }

    #[test]
    fn views_shrink_by_averaging_and_grow_by_repeating() {
        let img = Image {
            width: 2,
            height: 1,
            rgb: vec![1.0, 1.0, 1.0, 0.0, 0.0, 0.0],
        };
        let t = view(&img, false, 0.995);
        let a = t.to_array_view::<f32>().unwrap();
        let white = (1.0 - 0.485) / 0.229;
        assert!((a[[0, 0, 0, 0]] - white).abs() < 1e-4);
        assert!((a[[0, 0, 383, 383]] - (0.0 - 0.485) / 0.229).abs() < 1e-4);
    }
}
