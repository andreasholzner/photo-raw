//! The aurora edit (US-10): the photo's import history plus the aurora look of US-18, with an
//! exposure and a white balance found by rendering, so the sky has its target brightness and
//! colour (ADR-0004), and a colour correction of the land below the horizon (US-23).
//!
//! As with the day edit, the tool only appends to the history. Appended are: disabled copies of
//! the import's display transform, sharpening and denoise (the look brings its own) and of earlier
//! tool steps no longer wanted, the white balance, the exposure, chroma-only and luma denoise,
//! local contrast "clarity" twice, sigmoid "ACES 100-nit like", color balance rgb "vibrant colors"
//! with extra chroma and, where there is land, a color calibration masked to it with a drawn
//! path along the skyline. The tool's own steps and masks are recognised by their names, which
//! start with "photoflow".

use crate::aurora::Image;
use crate::history_edit::Entry;
use crate::horizon::Skyline;
use crate::look_edit::{Look, Shape, set_floats};
use anyhow::{Context, Result};

/// The modules the aurora edit adds, from the settled US-18 pipeline (darktable 5.6.1).
pub const MODULES_XMP: &str = include_str!("../data/aurora-modules.xmp");
/// Longest edge of the renders the edit is solved on; the targets depend on it (US-18, US-10).
pub const RENDER_SIZE: u32 = 300;
/// Sky median luminance in the edit's rendering (journal 2026-11-26).
pub const SKY_BRIGHT: f64 = 0.24;
/// … where the foreground is at least as bright as the sky.
pub const SKY_DARK: f64 = 0.14;
/// Foreground/sky ratio on the aurora check render from which [`SKY_DARK`] applies (owner,
/// 2026-11-26).
pub const DARK_FROM_RATIO: f64 = 1.0;
/// The land's colour: slightly blue snow, as R/G and B/G (US-18 round 4).
pub const LAND_TARGET: (f64, f64) = (0.95, 1.25);

/// Switched off when darktable applied them on import: display transforms and sharpening as for
/// the day edit, and denoise: the aurora look has its own.
const REPLACED: [&str; 7] = [
    "filmicrgb",
    "sigmoid",
    "agx",
    "basecurve",
    "filmic",
    "sharpen",
    "denoiseprofile",
];
/// The tool's land mask (its id in the template).
const LAND_PATH: &str = "1710017177";
/// The land mask starts this far (fraction of the frame height) below the skyline, with a soft
/// edge of the same width.
const LAND_OFFSET: f32 = 0.01;

/// The sky's background colour, chosen per night (US-10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkyColour {
    NeutralBlue,
    BlueViolet,
}

impl SkyColour {
    /// R/G and B/G of the sky background (journal 2026-11-26).
    pub fn target(self) -> (f64, f64) {
        match self {
            SkyColour::NeutralBlue => (0.95, 1.35),
            SkyColour::BlueViolet => (1.05, 1.50),
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            SkyColour::NeutralBlue => "neutral blue",
            SkyColour::BlueViolet => "blue-violet",
        }
    }
    pub fn other(self) -> SkyColour {
        match self {
            SkyColour::NeutralBlue => SkyColour::BlueViolet,
            SkyColour::BlueViolet => SkyColour::NeutralBlue,
        }
    }
}

/// What the edit is built from.
#[derive(Debug, Clone, PartialEq)]
pub struct Params {
    /// Exposure in EV.
    pub ev: f64,
    /// White balance coefficients for red and blue (green 1).
    pub wb: (f64, f64),
    /// The land: its skyline and the illuminant its colour correction removes (CIE x, y).
    pub land: Option<(Skyline, (f64, f64))>,
}

const LOOK: Look = Look {
    modules: MODULES_XMP,
    replaced: &REPLACED,
};

/// The photo's history with the aurora edit appended.
pub fn build(sidecar_text: &str, params: &Params) -> Result<String> {
    let mut steps: Vec<Entry> = LOOK
        .steps()?
        .into_iter()
        .filter(|e| e.get("operation") != "channelmixerrgb")
        .collect();
    let mut shapes = Vec::new();
    if let Some((skyline, (x, y))) = &params.land {
        let mut e = LOOK.template("channelmixerrgb", "")?;
        set_floats(&mut e, 96 + 10 * 4, &[*x as f32, *y as f32])?;
        steps.push(e);
        shapes.push(Shape {
            id: LAND_PATH,
            points: land_path(skyline),
            border: LAND_OFFSET,
        });
    }
    LOOK.append(sidecar_text, params.wb, params.ev, steps, &shapes)
}

/// The land as a closed polygon, [`LAND_OFFSET`] below the skyline.
pub fn land_path(skyline: &Skyline) -> Vec<(f32, f32)> {
    crate::look_edit::land_path(skyline, LAND_OFFSET)
}

// --- measuring renders ---

fn lum(p: [f32; 3]) -> f32 {
    0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]
}

/// Median luminance and the background colour (R/G, B/G of the 25–50 % luminance band) of the
/// given pixels.
fn region_stats(px: &mut [[f32; 3]], band: (usize, usize)) -> Option<(f64, f64, f64)> {
    if px.is_empty() {
        return None;
    }
    px.sort_by(|a, b| lum(*a).total_cmp(&lum(*b)));
    let m = px.len();
    let median = lum(px[m / 2]) as f64;
    let sel = &px[m * band.0 / 100..(m * band.1 / 100).max(m * band.0 / 100 + 1)];
    let s: Vec<f64> = (0..3)
        .map(|c| sel.iter().map(|q| q[c].max(1e-5) as f64).sum())
        .collect();
    Some((median, s[0] / s[1], s[2] / s[1]))
}

fn pixel(img: &Image, x: usize, y: usize) -> [f32; 3] {
    let k = 3 * (y * img.width + x);
    [img.rgb[k], img.rgb[k + 1], img.rgb[k + 2]]
}

/// The sky: the top 55 % of the frame (as the targets were fitted).
pub fn sky_stats(img: &Image) -> Result<(f64, f64, f64)> {
    let h = (img.height as f64 * 0.55) as usize;
    let mut px: Vec<[f32; 3]> = (0..h)
        .flat_map(|y| (0..img.width).step_by(2).map(move |x| (x, y)))
        .map(|(x, y)| pixel(img, x, y))
        .collect();
    region_stats(&mut px, (25, 50)).context("no sky pixels")
}

/// The land: below the skyline, from 0.05 of the frame height under it; colour from the middle
/// luminance band.
pub fn land_stats(img: &Image, skyline: &Skyline) -> Option<(f64, f64, f64)> {
    let n = skyline.columns.len();
    let mut px = Vec::new();
    for x in 0..img.width {
        let top = skyline.columns[x * n / img.width].unwrap_or(0.0) + 0.05;
        let y0 = ((top.min(0.97)) * img.height as f32) as usize;
        for y in y0..img.height {
            px.push(pixel(img, x, y));
        }
    }
    region_stats(&mut px, (25, 75))
}

/// Foreground (bottom 25 %) over sky (top 55 %) median luminance on the aurora check render
/// (as the threshold was fitted).
pub fn scene_ratio(check: &Image) -> f64 {
    let med = |y0: f64, y1: f64| {
        let mut v: Vec<f32> = ((check.height as f64 * y0) as usize
            ..(check.height as f64 * y1) as usize)
            .flat_map(|y| (0..check.width).map(move |x| (x, y)))
            .map(|(x, y)| lum(pixel(check, x, y)))
            .collect();
        v.sort_by(f32::total_cmp);
        v.get(v.len() / 2).copied().unwrap_or(0.0).max(1e-6) as f64
    };
    med(0.75, 1.0) / med(0.0, 0.55)
}

/// The sky target for a scene.
pub fn sky_target(scene_ratio: f64) -> f64 {
    if scene_ratio >= DARK_FROM_RATIO {
        SKY_DARK
    } else {
        SKY_BRIGHT
    }
}

// --- solving by rendering (ADR-0004) ---

pub struct Solved {
    pub xmp: String,
    pub params: Params,
    /// The sky reached its target brightness.
    pub reached: bool,
}

/// D50, where the land's colour correction starts.
const D50: (f64, f64) = (0.3457, 0.3585);

/// Finds exposure and white balance for the sky, then the land's colour correction.
pub fn solve(
    sidecar_text: &str,
    colour: SkyColour,
    sky_target: f64,
    skyline: Option<&Skyline>,
    render: &mut dyn FnMut(&str) -> Result<Image>,
) -> Result<Solved> {
    let (tr, tb) = colour.target();
    let t = sky_target.log2();
    let mut params = Params {
        ev: 1.0,
        wb: (2.4, 2.0),
        land: None,
    };
    // exposure and each white-balance coefficient are corrected by the response measured between
    // the last two renders (the look's extra chroma makes the colour react ~3× to the white
    // balance: fixed steps overshoot); the best render wins
    let mut pts: Vec<(f64, f64)> = Vec::new();
    let mut red: Vec<(f64, f64)> = Vec::new();
    let mut blue: Vec<(f64, f64)> = Vec::new();
    let mut best: Option<(f64, Params, bool)> = None;
    for _ in 0..14 {
        let img = render(&build(sidecar_text, &params)?)?;
        let (l50, rg, bg) = sky_stats(&img)?;
        let y = l50.max(1e-4).log2();
        if std::env::var_os("PF_TRACE").is_some() {
            eprintln!(
                "ev {:.2} wb {:.2}/{:.2} -> L {l50:.3} rg {rg:.3} bg {bg:.3}",
                params.ev, params.wb.0, params.wb.1
            );
        }
        let (er, eb) = ((tr / rg).ln(), (tb / bg).ln());
        let err = (y - t).abs() + er.abs() + eb.abs();
        let reached_here = (y - t).abs() < 0.15;
        if best.as_ref().is_none_or(|b| err < b.0) {
            best = Some((err, params.clone(), reached_here));
        }
        if (y - t).abs() < 0.08 && er.abs() < 0.04 && eb.abs() < 0.04 {
            break;
        }
        pts.push((params.ev, y));
        red.push((params.wb.0.ln(), rg.ln()));
        blue.push((params.wb.1.ln(), bg.ln()));
        let response = |h: &[(f64, f64)], default: f64, lo: f64, hi: f64| match h {
            [.., a, b] if (b.0 - a.0).abs() > 1e-3 => ((b.1 - a.1) / (b.0 - a.0)).clamp(lo, hi),
            _ => default,
        };
        let step = |e: f64, k: f64| (e / k).clamp(-0.22, 0.22).exp();
        params.wb = (
            (params.wb.0 * step(er, response(&red, 1.7, 0.5, 6.0))).clamp(1.0, 6.0),
            (params.wb.1 * step(eb, response(&blue, 1.7, 0.5, 6.0))).clamp(0.8, 10.0),
        );
        params.ev = (params.ev + (t - y) / response(&pts, 0.8, 0.3, 1.5)).clamp(-3.0, 6.0);
    }
    let (_, best_params, reached) = best.context("no render")?;
    params = best_params;

    if let Some(line) = skyline.filter(|l| l.has_land()) {
        let target = (LAND_TARGET.0.ln(), LAND_TARGET.1.ln());
        let mut meas = |xy: (f64, f64)| -> Result<Option<(f64, f64)>> {
            let p = Params {
                land: Some((line.clone(), xy)),
                ..params.clone()
            };
            let img = render(&build(sidecar_text, &p)?)?;
            Ok(land_stats(&img, line).map(|(_, rg, bg)| (rg.ln(), bg.ln())))
        };
        let (mut x, mut y) = D50;
        if let Some(f0) = meas((x, y))? {
            let h = 0.01;
            let fx = meas((x + h, y))?.context("no land")?;
            let fy = meas((x, y + h))?.context("no land")?;
            let j = [
                [(fx.0 - f0.0) / h, (fy.0 - f0.0) / h],
                [(fx.1 - f0.1) / h, (fy.1 - f0.1) / h],
            ];
            let det = j[0][0] * j[1][1] - j[0][1] * j[1][0];
            let mut f = f0;
            for _ in 0..6 {
                let e = (target.0 - f.0, target.1 - f.1);
                if e.0.abs().max(e.1.abs()) < 0.04 || det.abs() < 1e-12 {
                    break;
                }
                let dx = (j[1][1] * e.0 - j[0][1] * e.1) / det;
                let dy = (-j[1][0] * e.0 + j[0][0] * e.1) / det;
                let sc = (0.04 / dx.abs().max(dy.abs()).max(1e-9)).min(1.0);
                x = (x + dx * sc).clamp(0.2, 0.5);
                y = (y + dy * sc).clamp(0.2, 0.5);
                f = meas((x, y))?.context("no land")?;
            }
            params.land = Some((line.clone(), (x, y)));
        }
    }
    Ok(Solved {
        xmp: build(sidecar_text, &params)?,
        params,
        reached,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history_edit::{active, parse};
    use crate::look_edit::{PATH_POINTS, attr, forms};

    const IMPORT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<rdf:Description xmlns:darktable="http://darktable.sf.net/" darktable:history_end="4" darktable:iop_order_version="4">
<darktable:history><rdf:Seq>
<rdf:li darktable:num="0" darktable:operation="temperature" darktable:enabled="1" darktable:modversion="3" darktable:params="0000004000000040000000400000804000000000" darktable:multi_name="" darktable:multi_priority="0"/>
<rdf:li darktable:num="1" darktable:operation="exposure" darktable:enabled="1" darktable:modversion="7" darktable:params="00000000000080b9000000000000484200000000010000000100000000" darktable:multi_name="" darktable:multi_priority="0"/>
<rdf:li darktable:num="2" darktable:operation="denoiseprofile" darktable:enabled="1" darktable:modversion="11" darktable:params="00" darktable:multi_name="owner preset" darktable:multi_priority="0"/>
<rdf:li darktable:num="3" darktable:operation="lens" darktable:enabled="1" darktable:modversion="10" darktable:params="00" darktable:multi_name="" darktable:multi_priority="0"/>
</rdf:Seq></darktable:history>
</rdf:Description></rdf:RDF></x:xmpmeta>"#;

    fn line() -> Skyline {
        Skyline {
            columns: (0..96)
                .map(|i| Some(if i < 48 { 0.6 } else { 0.7 }))
                .collect(),
        }
    }

    fn ops(xmp: &str) -> Vec<(String, String, u32, bool)> {
        let p = parse(xmp).unwrap();
        active(&p.entries)
            .iter()
            .map(|e| {
                (
                    e.get("operation").into(),
                    e.get("multi_name").into(),
                    e.prio(),
                    true,
                )
            })
            .collect()
    }

    #[test]
    fn the_look_is_appended_and_the_import_denoise_switched_off() {
        let x = build(
            IMPORT,
            &Params {
                ev: 2.0,
                wb: (2.5, 1.9),
                land: None,
            },
        )
        .unwrap();
        let a = ops(&x);
        let has = |op: &str, name: &str| a.iter().any(|(o, n, _, _)| o == op && n == name);
        assert!(has("lens", ""));
        assert!(!has("denoiseprofile", "owner preset"));
        assert!(has("denoiseprofile", "photoflow chroma only"));
        assert!(has("denoiseprofile", "photoflow luma 0.7"));
        assert_eq!(a.iter().filter(|(o, ..)| o == "bilat").count(), 2);
        assert!(has("sigmoid", "photoflow sigmoid ACES"));
        assert!(!a.iter().any(|(o, ..)| o == "channelmixerrgb"));
        // the import's denoise keeps priority 0; the tool's take 1 and 2
        let prios: Vec<u32> = a
            .iter()
            .filter(|(o, ..)| o == "denoiseprofile")
            .map(|t| t.2)
            .collect();
        assert_eq!(prios, vec![1, 2]);
        let p = parse(&x).unwrap();
        assert!(p.order_list.unwrap().contains("denoiseprofile,2"));
    }

    #[test]
    fn rebuilding_reuses_the_tools_instances() {
        let params = Params {
            ev: 2.0,
            wb: (2.5, 1.9),
            land: Some((line(), (0.33, 0.34))),
        };
        let once = build(IMPORT, &params).unwrap();
        let twice = build(
            &once,
            &Params {
                land: None,
                ..params
            },
        )
        .unwrap();
        let a = ops(&twice);
        assert_eq!(a.iter().filter(|(o, ..)| o == "denoiseprofile").count(), 2);
        assert_eq!(a.iter().filter(|(o, ..)| o == "bilat").count(), 2);
        // the land step is switched off when there is no land any more
        assert!(!a.iter().any(|(o, ..)| o == "channelmixerrgb"));
    }

    #[test]
    fn the_land_gets_a_masked_colour_correction() {
        let x = build(
            IMPORT,
            &Params {
                ev: 2.0,
                wb: (2.5, 1.9),
                land: Some((line(), (0.33, 0.34))),
            },
        )
        .unwrap();
        assert!(
            ops(&x)
                .iter()
                .any(|(o, n, ..)| o == "channelmixerrgb" && n == "photoflow foreground")
        );
        let f = forms(&parse(&x).unwrap().masks).unwrap();
        let path = f
            .iter()
            .find(|f| attr(f, "mask_type") == "2")
            .expect("a path");
        assert_eq!(attr(path, "mask_nb"), (PATH_POINTS + 4).to_string());
        let mm = parse(&x)
            .unwrap()
            .entries
            .iter()
            .find(|e| e.get("operation") == "mask_manager")
            .map(|e| e.get("num").to_string());
        assert_eq!(Some(attr(path, "mask_num").to_string()), mm);
    }

    #[test]
    fn the_path_follows_the_skyline_and_closes_below_the_frame() {
        let pts = land_path(&line());
        assert!((pts[1].1 - 0.61).abs() < 1e-6 && (pts[PATH_POINTS].1 - 0.71).abs() < 1e-6);
        assert_eq!(pts[pts.len() - 1], (0.0, 1.0));
    }

    #[test]
    fn a_bright_foreground_gets_the_dark_target() {
        assert_eq!(sky_target(1.2), SKY_DARK);
        assert_eq!(sky_target(0.8), SKY_BRIGHT);
    }
}
