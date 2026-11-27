//! The night edit (US-21): the photo's import history plus the night look, with exposure, white
//! balance, sky tints and the foreground solved by rendering (ADR-0004), from the owner's six
//! favourite night edits (journal 2026-11-26/27, rounds 1–13).
//!
//! Appended (see `look_edit`): raw chromatic aberrations, chroma-only and luma denoise, local
//! contrast "clarity" twice and diffuse "local contrast | fine", above the horizon a tone
//! equalizer that darkens the sky between the stars, a tone curve after AgX that lifts the stars,
//! color balance rgb tints for the dark sky and the Milky Way glow, below the horizon a
//! brightness step and a colour correction to neutral, AgX punchy.

use crate::aurora::Image;
use crate::history_edit::Entry;
use crate::horizon::Skyline;
use crate::look_edit::{Look, Shape, set_floats};
use anyhow::Result;

/// The look's template, from round 13's K1 edit (darktable 5.6.1).
pub const MODULES_XMP: &str = include_str!("../data/night-modules.xmp");
/// Longest edge of most solving renders; the star colour is measured on larger ones.
pub const RENDER_SIZE: u32 = 300;
pub const STAR_RENDER_SIZE: u32 = 1500;
/// Sky median in the edit's rendering (round 11, G1).
pub const SKY_TARGET: f64 = 0.065;
/// The stars' colour, R/G and B/G: the median over the six favourite edits (round 6).
pub const STAR_TARGET: (f64, f64) = (0.95, 1.12);
/// The dark sky's and the Milky Way glow's colour, ln(R/G) and ln(B/G): the medians over the six
/// favourite edits (round 13).
pub const DARK_SKY_TARGET: (f64, f64) = (-0.0878, 0.1981);
pub const MILKY_WAY_TARGET: (f64, f64) = (-0.1219, 0.1738);
/// The foreground's brightness and colour (round 13, K1).
pub const LAND_TARGET: (f64, f64, f64) = (0.10, 1.0, 1.0);
/// A sky median at or above this on the aurora check render is twilight or blue hour, not a
/// night sky: no automatic edit (owner, 2026-11-27; twilight 0.20–0.62, the brightest star
/// photo, a moonrise, 0.17).
pub const BRIGHT_SKY: f64 = 0.19;

const REPLACED: [&str; 7] = [
    "filmicrgb",
    "sigmoid",
    "agx",
    "basecurve",
    "filmic",
    "sharpen",
    "denoiseprofile",
];
const LOOK: Look = Look {
    modules: MODULES_XMP,
    replaced: &REPLACED,
};
/// The tool's masks (ids in the template).
const SKY_PATH: &str = "1710017177";
const LAND_PATH: &str = "1810017177";
/// The masks cross-fade below the skyline (accurate to about 1 % of the frame): the sky's fades
/// out over this band, the land's fades in over the same band. Hard or overlapping edges left
/// seams along distant ridges where the land's step is large (journal 2026-11-27).
const FADE: f32 = 0.012;
/// color balance rgb: chroma and hue (degrees) of the shadows and midtones wheels (float indices).
const SHADOWS_C: usize = 1;
const MIDTONES_C: usize = 4;

/// What the edit is built from.
#[derive(Debug, Clone, PartialEq)]
pub struct Params {
    pub ev: f64,
    /// White balance coefficients for red and blue (green 1).
    pub wb: (f64, f64),
    /// The dark-sky tint (shadows wheel) and the Milky Way tint (midtones wheel), as (u, v)
    /// vectors: chroma is the length, hue the angle.
    pub shadows: (f64, f64),
    pub midtones: (f64, f64),
    pub skyline: Skyline,
    /// The land's brightness step (EV) and the illuminant its colour correction removes.
    pub land: Option<(f64, (f64, f64))>,
}

impl Params {
    fn start(skyline: &Skyline) -> Self {
        Params {
            ev: 2.0,
            wb: (2.4, 2.0),
            shadows: (0.0, 0.0),
            midtones: (0.0, 0.0),
            skyline: skyline.clone(),
            land: None,
        }
    }
}

fn wheel(e: &mut Entry, at: usize, (u, v): (f64, f64)) -> Result<()> {
    let chroma = u.hypot(v).min(0.5);
    let hue = v.atan2(u).to_degrees().rem_euclid(360.0);
    set_floats(e, 4 * at, &[chroma as f32, hue as f32])
}

/// The photo's history with the night edit appended.
pub fn build(sidecar_text: &str, params: &Params) -> Result<String> {
    let mut steps = Vec::new();
    for mut e in LOOK.steps()? {
        let land_step = matches!(
            e.get("multi_name"),
            "photoflow foreground brightness" | "photoflow foreground colour"
        );
        match (land_step, &params.land) {
            (true, None) => continue,
            (true, Some((dev, (x, y)))) => {
                if e.get("operation") == "toneequal" {
                    set_floats(&mut e, 0, &[*dev as f32; 9])?;
                } else {
                    set_floats(&mut e, 96 + 10 * 4, &[*x as f32, *y as f32])?;
                }
            }
            (false, _) if e.get("operation") == "colorbalancergb" => {
                wheel(&mut e, SHADOWS_C, params.shadows)?;
                wheel(&mut e, MIDTONES_C, params.midtones)?;
            }
            _ => {}
        }
        steps.push(e);
    }
    let mut shapes = vec![Shape {
        id: SKY_PATH,
        points: crate::look_edit::sky_path(&params.skyline, 0.0),
        border: FADE,
    }];
    if params.land.is_some() {
        shapes.push(Shape {
            id: LAND_PATH,
            points: crate::look_edit::land_path(&params.skyline, FADE),
            border: FADE,
        });
    }
    LOOK.append(sidecar_text, params.wb, params.ev, steps, &shapes)
}

// --- measuring renders ---

fn lum(p: [f32; 3]) -> f32 {
    0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]
}

fn pixel(img: &Image, x: usize, y: usize) -> [f32; 3] {
    let k = 3 * (y * img.width + x);
    [img.rgb[k], img.rgb[k + 1], img.rgb[k + 2]]
}

/// Whether the sky of the aurora check render is bright: twilight, blue hour.
pub fn bright_sky(check: &Image) -> bool {
    let h = (check.height as f64 * 0.55) as usize;
    let mut v: Vec<f32> = (0..h)
        .flat_map(|y| (0..check.width).step_by(2).map(move |x| (x, y)))
        .map(|(x, y)| lum(pixel(check, x, y)))
        .collect();
    v.sort_by(f32::total_cmp);
    v.get(v.len() / 2)
        .is_some_and(|&m| f64::from(m) >= BRIGHT_SKY)
}

/// ln(R/G) and ln(B/G) of a luminance band (fractions `lo`–`hi` of the pixels sorted by
/// luminance) of the upper sky, 5–50 % from the top.
fn sky_band(img: &Image, lo: f64, hi: f64) -> (f64, f64) {
    let (y0, y1) = (img.height / 20, img.height / 2);
    let mut px: Vec<[f32; 3]> = (y0..y1)
        .step_by(2)
        .flat_map(|y| (0..img.width).step_by(2).map(move |x| (x, y)))
        .map(|(x, y)| pixel(img, x, y))
        .collect();
    px.sort_by(|a, b| lum(*a).total_cmp(&lum(*b)));
    let m = px.len();
    let band = &px[(lo * m as f64) as usize..((hi * m as f64) as usize).max(1)];
    let s: Vec<f64> = (0..3)
        .map(|c| band.iter().map(|q| f64::from(q[c].max(1e-5))).sum())
        .collect();
    ((s[0] / s[1]).ln(), (s[2] / s[1]).ln())
}

/// The dark sky between the stars.
pub fn dark_sky(img: &Image) -> (f64, f64) {
    sky_band(img, 0.1, 0.3)
}

/// The Milky Way glow, below the stars' brightness.
pub fn milky_way(img: &Image) -> (f64, f64) {
    sky_band(img, 0.6, 0.8)
}

/// The stars' colour: median R/G and B/G (3×3 sums) of the 300 brightest local maxima in the
/// upper sky (10–90 % of the width, 5–50 % from the top).
pub fn star_colour(img: &Image) -> Option<(f64, f64)> {
    let (x0, x1) = (img.width / 10, img.width * 9 / 10);
    let (y0, y1) = (img.height / 20, img.height / 2);
    let l = |x: usize, y: usize| lum(pixel(img, x, y));
    let mut peaks = Vec::new();
    for y in y0.max(1)..y1.min(img.height - 1) {
        for x in x0.max(1)..x1.min(img.width - 1) {
            let v = l(x, y);
            if v > l(x - 1, y)
                && v >= l(x + 1, y)
                && v > l(x, y - 1)
                && v >= l(x, y + 1)
                && v >= l(x - 1, y - 1)
                    .max(l(x + 1, y - 1))
                    .max(l(x - 1, y + 1))
                    .max(l(x + 1, y + 1))
            {
                peaks.push((v, x, y));
            }
        }
    }
    if peaks.is_empty() {
        return None;
    }
    peaks.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut rg = Vec::new();
    let mut bg = Vec::new();
    for &(_, x, y) in peaks.iter().take(300) {
        let mut s = [0f64; 3];
        for yy in y - 1..=y + 1 {
            for xx in x - 1..=x + 1 {
                let p = pixel(img, xx, yy);
                for c in 0..3 {
                    s[c] += f64::from(p[c]);
                }
            }
        }
        rg.push(s[0] / s[1].max(1e-6));
        bg.push(s[2] / s[1].max(1e-6));
    }
    let med = |v: &mut Vec<f64>| {
        v.sort_by(f64::total_cmp);
        v[v.len() / 2]
    };
    Some((med(&mut rg), med(&mut bg)))
}

// --- solving by rendering (ADR-0004) ---

pub struct Solved {
    pub xmp: String,
    pub params: Params,
    /// The sky reached its target brightness.
    pub reached: bool,
}

/// A render of an edit at a size.
pub type Render<'a> = dyn FnMut(&str, u32) -> Result<Image> + 'a;

/// The exposure for the sky's median, by the response measured between renders.
fn solve_ev(text: &str, p: &mut Params, render: &mut Render) -> Result<bool> {
    let t = SKY_TARGET.log2();
    let mut pts: Vec<(f64, f64)> = Vec::new();
    for _ in 0..7 {
        let img = render(&build(text, p)?, RENDER_SIZE)?;
        let y = crate::aurora_edit::sky_stats(&img)?.0.max(1e-4).log2();
        if (y - t).abs() < 0.05 {
            return Ok(true);
        }
        pts.push((p.ev, y));
        let slope = match pts[..] {
            [.., a, b] if (b.0 - a.0).abs() > 1e-6 => ((b.1 - a.1) / (b.0 - a.0)).clamp(0.2, 1.5),
            _ => 0.8,
        };
        p.ev = (p.ev + ((t - y) / slope).clamp(-2.0, 2.0)).clamp(-4.0, 8.0);
    }
    let img = render(&build(text, p)?, RENDER_SIZE)?;
    let y = crate::aurora_edit::sky_stats(&img)?.0.max(1e-4).log2();
    Ok((y - t).abs() < 0.15)
}

/// The white balance for slightly blue stars, measured on larger renders.
fn solve_star_wb(text: &str, p: &mut Params, render: &mut Render) -> Result<()> {
    let (tr, tb) = STAR_TARGET;
    for _ in 0..6 {
        let img = render(&build(text, p)?, STAR_RENDER_SIZE)?;
        let Some((rg, bg)) = star_colour(&img) else {
            return Ok(());
        };
        if (rg / tr).ln().abs() < 0.03 && (bg / tb).ln().abs() < 0.03 {
            break;
        }
        p.wb = (
            (p.wb.0 * (tr / rg).powf(0.8)).clamp(1.0, 5.0),
            (p.wb.1 * (tb / bg).powf(0.8)).clamp(0.8, 8.0),
        );
    }
    Ok(())
}

/// A measurement for [`newton`]: the parameter to the measured value, if there is one.
type Measure<'a> = dyn FnMut((f64, f64)) -> Result<Option<(f64, f64)>> + 'a;

/// A two-dimensional solve by Newton steps with a numerical Jacobian (refreshed every third
/// step): `measure` maps the parameter to the measured value, `step` caps each step.
fn newton(
    start: (f64, f64),
    target: (f64, f64),
    h: f64,
    step: f64,
    iterations: usize,
    bounds: (f64, f64),
    measure: &mut Measure,
) -> Result<(f64, f64)> {
    let mut x = start;
    let Some(mut f) = measure(x)? else {
        return Ok(start);
    };
    let jacobian =
        |x: (f64, f64), f: (f64, f64), measure: &mut Measure| -> Result<Option<[[f64; 2]; 2]>> {
            let (Some(fu), Some(fv)) = (measure((x.0 + h, x.1))?, measure((x.0, x.1 + h))?) else {
                return Ok(None);
            };
            Ok(Some([
                [(fu.0 - f.0) / h, (fv.0 - f.0) / h],
                [(fu.1 - f.1) / h, (fv.1 - f.1) / h],
            ]))
        };
    let Some(mut j) = jacobian(x, f, measure)? else {
        return Ok(start);
    };
    for i in 0..iterations {
        let e = (target.0 - f.0, target.1 - f.1);
        let det = j[0][0] * j[1][1] - j[0][1] * j[1][0];
        if e.0.abs().max(e.1.abs()) < 0.04 || det.abs() < 1e-12 || !det.is_finite() {
            break;
        }
        let du = (j[1][1] * e.0 - j[0][1] * e.1) / det;
        let dv = (-j[1][0] * e.0 + j[0][0] * e.1) / det;
        let sc = (step / du.abs().max(dv.abs()).max(1e-9)).min(1.0);
        x = (
            (x.0 + du * sc).clamp(bounds.0, bounds.1),
            (x.1 + dv * sc).clamp(bounds.0, bounds.1),
        );
        let Some(nf) = measure(x)? else { break };
        f = nf;
        if i % 3 == 2 {
            match jacobian(x, f, measure)? {
                Some(nj) => j = nj,
                None => break,
            }
        }
    }
    Ok(x)
}

/// Finds the exposure, the star white balance, the sky tints and the land's steps.
pub fn solve(sidecar_text: &str, skyline: &Skyline, render: &mut Render) -> Result<Solved> {
    let text = sidecar_text;
    let mut p = Params::start(skyline);
    solve_ev(text, &mut p, render)?;
    solve_star_wb(text, &mut p, render)?;
    // the dark-sky tint, then the Milky Way tint (a strong wheel: small steps)
    let base = p.clone();
    p.shadows = newton(
        (0.0, 0.0),
        DARK_SKY_TARGET,
        0.01,
        0.03,
        12,
        (-0.5, 0.5),
        &mut |uv| {
            let q = Params {
                shadows: uv,
                ..base.clone()
            };
            Ok(Some(dark_sky(&render(&build(text, &q)?, RENDER_SIZE)?)))
        },
    )?;
    let base = p.clone();
    let a = 330f64.to_radians();
    p.midtones = newton(
        (0.012 * a.cos(), 0.012 * a.sin()),
        MILKY_WAY_TARGET,
        0.002,
        0.005,
        12,
        (-0.5, 0.5),
        &mut |uv| {
            let q = Params {
                midtones: uv,
                ..base.clone()
            };
            Ok(Some(milky_way(&render(&build(text, &q)?, RENDER_SIZE)?)))
        },
    )?;
    let reached = solve_ev(text, &mut p, render)?;

    if skyline.has_land() && skyline.middle().is_some_and(|m| m < 0.97) {
        // the land's brightness: one gain on every band of a tone equalizer, masked to the land
        let land = |img: &Image| crate::aurora_edit::land_stats(img, skyline);
        let t = LAND_TARGET.0.log2();
        let mut dev = 0.0;
        let mut pts: Vec<(f64, f64)> = Vec::new();
        for _ in 0..7 {
            let q = Params {
                land: Some((dev, (0.3457, 0.3585))),
                ..p.clone()
            };
            let Some((l, _, _)) = land(&render(&build(text, &q)?, RENDER_SIZE)?) else {
                break;
            };
            let y = l.max(1e-4).log2();
            if (y - t).abs() < 0.05 {
                break;
            }
            pts.push((dev, y));
            let slope = match pts[..] {
                [.., a, b] if (b.0 - a.0).abs() > 1e-6 => {
                    ((b.1 - a.1) / (b.0 - a.0)).clamp(0.2, 1.5)
                }
                _ => 0.7,
            };
            dev = (dev + (t - y) / slope).clamp(-2.0, 2.0);
        }
        // its colour: a color calibration to neutral
        let base = p.clone();
        let xy = newton(
            (0.3457, 0.3585),
            (LAND_TARGET.1.ln(), LAND_TARGET.2.ln()),
            0.01,
            0.04,
            6,
            (0.2, 0.5),
            &mut |xy| {
                let q = Params {
                    land: Some((dev, xy)),
                    ..base.clone()
                };
                Ok(land(&render(&build(text, &q)?, RENDER_SIZE)?)
                    .filter(|s| s.1 > 0.0 && s.2 > 0.0)
                    .map(|(_, rg, bg)| (rg.ln(), bg.ln())))
            },
        )?;
        p.land = Some((dev, xy));
    }
    Ok(Solved {
        xmp: build(text, &p)?,
        params: p,
        reached,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history_edit::{active, parse};
    use crate::look_edit::{attr, forms};

    const IMPORT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<rdf:Description xmlns:darktable="http://darktable.sf.net/" darktable:history_end="2" darktable:iop_order_version="4">
<darktable:history><rdf:Seq>
<rdf:li darktable:num="0" darktable:operation="temperature" darktable:enabled="1" darktable:modversion="4" darktable:params="0000004000000040000000400000000004000000" darktable:multi_name="" darktable:multi_priority="0"/>
<rdf:li darktable:num="1" darktable:operation="sharpen" darktable:enabled="1" darktable:modversion="1" darktable:params="00" darktable:multi_name="" darktable:multi_priority="0"/>
</rdf:Seq></darktable:history>
</rdf:Description></rdf:RDF></x:xmpmeta>"#;

    fn skyline(y: f32) -> Skyline {
        Skyline {
            columns: vec![Some(y); 96],
        }
    }

    fn params(land: bool, y: f32) -> Params {
        Params {
            land: land.then_some((-0.5, (0.33, 0.34))),
            ..Params::start(&skyline(y))
        }
    }

    fn names(xmp: &str) -> Vec<String> {
        let p = parse(xmp).unwrap();
        active(&p.entries)
            .iter()
            .map(|e| e.get("multi_name").to_string())
            .collect()
    }

    #[test]
    fn the_night_look_with_sky_and_land_masks() {
        let x = build(IMPORT, &params(true, 0.7)).unwrap();
        let n = names(&x);
        for want in [
            "photoflow raw chromatic aberrations",
            "photoflow chroma only",
            "photoflow luma 0.7",
            "photoflow clarity",
            "photoflow clarity 2",
            "photoflow local contrast fine",
            "photoflow star punch",
            "photoflow star brightness",
            "photoflow sky tint",
            "photoflow foreground brightness",
            "photoflow foreground colour",
            "photoflow AgX punchy",
        ] {
            assert!(n.iter().any(|x| x == want), "no {want}");
        }
        // the import's sharpening is switched off
        let p = parse(&x).unwrap();
        assert!(
            !active(&p.entries)
                .iter()
                .any(|e| e.get("operation") == "sharpen")
        );
        let f = forms(&p.masks).unwrap();
        let paths: Vec<&str> = f
            .iter()
            .filter(|f| attr(f, "mask_type") == "2")
            .map(|f| attr(f, "mask_name"))
            .collect();
        assert_eq!(paths, vec!["photoflow sky", "photoflow land"]);
        assert_eq!(f.iter().filter(|f| attr(f, "mask_type") == "4").count(), 2);
    }

    #[test]
    fn without_land_only_the_sky_mask() {
        let x = build(IMPORT, &params(false, 1.0)).unwrap();
        let n = names(&x);
        assert!(!n.iter().any(|x| x.starts_with("photoflow foreground")));
        let f = forms(&parse(&x).unwrap().masks).unwrap();
        let ids: Vec<&str> = f.iter().map(|f| attr(f, "mask_id")).collect();
        assert_eq!(ids, vec![SKY_PATH, "1710017183"]);
    }

    #[test]
    fn a_bright_sky_is_twilight() {
        let img = |v: f32| Image {
            width: 10,
            height: 10,
            rgb: vec![v; 300],
        };
        assert!(bright_sky(&img(0.3)));
        assert!(!bright_sky(&img(0.1)));
    }

    #[test]
    fn star_colour_from_local_maxima() {
        let (w, h) = (40, 40);
        let mut rgb = vec![0.01f32; w * h * 3];
        for &(x, y) in &[(10usize, 8usize), (20, 12), (30, 15)] {
            let k = 3 * (y * w + x);
            rgb[k..k + 3].copy_from_slice(&[0.9, 1.0, 1.2]);
        }
        let (rg, bg) = star_colour(&Image {
            width: w,
            height: h,
            rgb,
        })
        .unwrap();
        assert!(rg < 1.0 && bg > 1.0);
    }
}
