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
use crate::history_edit::{Entry, active, parse, with_instances, write};
use crate::horizon::Skyline;
use crate::sidecar;
use anyhow::{Context, Result, bail, ensure};

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

const OWN: &str = "photoflow";
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
/// The tool's masks (ids from the template): the path along the skyline and its group.
const LAND_PATH: &str = "1710017177";
const LAND_GROUP: &str = "1710017183";
/// The land mask starts this far (fraction of the frame height) below the skyline, with a soft
/// edge of the same width.
const LAND_OFFSET: f32 = 0.01;
/// Points of the land path along the skyline.
const PATH_POINTS: usize = 48;

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

fn template(op: &str, name: &str) -> Result<Entry> {
    parse(MODULES_XMP)?
        .entries
        .into_iter()
        .find(|e| e.get("operation") == op && (name.is_empty() || e.get("multi_name") == name))
        .with_context(|| format!("no {op} {name:?} in the aurora modules"))
}

/// The look's own steps, in order (without the land step).
fn look_steps() -> Result<Vec<Entry>> {
    Ok(parse(MODULES_XMP)?
        .entries
        .into_iter()
        .filter(|e| {
            let op = e.get("operation");
            op != "mask_manager" && op != "channelmixerrgb" && op != "temperature"
        })
        .collect())
}

fn set_floats(e: &mut Entry, at: usize, values: &[f32]) -> Result<()> {
    let mut p = sidecar::decode(e.get("params"))?;
    ensure!(
        p.len() >= at + 4 * values.len(),
        "unexpected {} parameters",
        e.get("operation")
    );
    for (i, v) in values.iter().enumerate() {
        p[at + 4 * i..at + 4 * i + 4].copy_from_slice(&v.to_le_bytes());
    }
    e.set("params", sidecar::encode(&p));
    Ok(())
}

/// The photo's history with the aurora edit appended.
pub fn build(sidecar_text: &str, params: &Params) -> Result<String> {
    let p = parse(sidecar_text)?;
    let mut entries = p.entries.clone();
    let was_active: Vec<Entry> = active(&p.entries).into_iter().cloned().collect();
    let own = |e: &Entry| e.get("multi_name").starts_with(OWN);

    let mut steps = look_steps()?;
    if params.land.is_some() {
        steps.push(template("channelmixerrgb", "")?);
    }
    let wanted = |e: &Entry| {
        steps.iter().any(|s| {
            s.get("operation") == e.get("operation") && s.get("multi_name") == e.get("multi_name")
        })
    };
    let mut add: Vec<Entry> = Vec::new();
    for e in &was_active {
        let op = e.get("operation");
        if (own(e) && !wanted(e)) || (!own(e) && REPLACED.contains(&op)) {
            let mut off = e.clone();
            off.set("enabled", "0".into());
            add.push(off);
        }
    }
    // white balance and exposure: the import's main instances
    let main = |op: &str| {
        was_active
            .iter()
            .find(|e| e.get("operation") == op && e.prio() == 0 && !own(e))
            .cloned()
    };
    let mut wb = match main("temperature") {
        Some(e) => e,
        None => template("temperature", "")?,
    };
    set_floats(&mut wb, 0, &[params.wb.0 as f32, 1.0, params.wb.1 as f32])?;
    add.push(wb);
    let mut exposure = main("exposure").map(Ok).unwrap_or_else(|| {
        crate::history_edit::parse(crate::day_edit::MODULES_XMP)?
            .entries
            .into_iter()
            .find(|e| e.get("operation") == "exposure")
            .context("no exposure template")
    })?;
    set_floats(&mut exposure, 8, &[params.ev as f32])?;
    exposure.set("enabled", "1".into());
    add.push(exposure);

    // the land mask: a new snapshot of the forms, the earlier ones plus the tool's path
    let mut masks = p.masks.clone();
    if let Some((skyline, _)) = &params.land {
        let mut mm = template("mask_manager", "")?;
        mm.set("enabled", "1".into());
        let num = p.entries.len() + add.len();
        masks = with_land_forms(&p.masks, num, skyline)?;
        add.push(mm);
    }
    // the look: the tool's instance of each step where there is one, else a free one
    let mut new_instances = Vec::new();
    for mut e in steps {
        let op = e.get("operation").to_string();
        let name = e.get("multi_name").to_string();
        let same = |x: &Entry| x.get("operation") == op && x.get("multi_name") == name;
        let prio = match entries.iter().chain(add.iter()).find(|x| same(x)) {
            Some(x) => x.prio(),
            None => entries
                .iter()
                .chain(add.iter())
                .filter(|x| x.get("operation") == op)
                .map(Entry::prio)
                .max()
                .map_or(0, |m| m + 1),
        };
        e.set("multi_priority", prio.to_string());
        if op == "channelmixerrgb" {
            let (_, (x, y)) = params.land.as_ref().context("land step without land")?;
            set_floats(&mut e, 96 + 10 * 4, &[*x as f32, *y as f32])?;
        }
        if prio > 0 && !new_instances.contains(&(op.clone(), prio)) {
            new_instances.push((op, prio));
        }
        add.push(e);
    }
    for (i, mut e) in add.into_iter().enumerate() {
        e.set("num", (p.entries.len() + i).to_string());
        entries.push(e);
    }

    let list = match &p.order_list {
        Some(l) => with_instances(l, &new_instances)?,
        None if p.order_version == "4" || p.order_version.is_empty() => with_instances(
            &parse(MODULES_XMP)?.order_list.context("no order list")?,
            &new_instances,
        )?,
        None => bail!("can't add module instances to this history"),
    };
    let attrs = vec![
        ("xmp_version", "5".to_string()),
        ("auto_presets_applied", "1".to_string()),
        ("history_end", entries.len().to_string()),
        ("iop_order_version", "4".to_string()),
        ("iop_order_list", list),
    ];
    Ok(write(&attrs, &entries, &masks))
}

/// One masks_history entry: its darktable attributes.
type Form = Vec<(String, String)>;

fn forms(masks: &str) -> Result<Vec<Form>> {
    if masks.trim().is_empty() {
        return Ok(Vec::new());
    }
    let wrapped = format!(
        r#"<r xmlns:darktable="http://darktable.sf.net/" xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">{masks}</r>"#
    );
    let doc = roxmltree::Document::parse(&wrapped)?;
    Ok(doc
        .descendants()
        .filter(|n| n.has_tag_name(("http://www.w3.org/1999/02/22-rdf-syntax-ns#", "li")))
        .map(|li| {
            li.attributes()
                .map(|a| (a.name().to_string(), a.value().to_string()))
                .collect()
        })
        .collect())
}

fn attr<'a>(f: &'a Form, k: &str) -> &'a str {
    f.iter().find(|(n, _)| n == k).map_or("", |(_, v)| v)
}

fn set_attr(f: &mut Form, k: &str, v: String) {
    match f.iter_mut().find(|(n, _)| n == k) {
        Some(x) => x.1 = v,
        None => f.push((k.to_string(), v)),
    }
}

/// The masks_history element: the earlier snapshots unchanged, and at history entry `num` the
/// latest snapshot's forms (without the tool's earlier ones) plus the land path and its group.
fn with_land_forms(masks: &str, num: usize, skyline: &Skyline) -> Result<String> {
    let old = forms(masks)?;
    let latest = old
        .iter()
        .filter_map(|f| attr(f, "mask_num").parse::<usize>().ok())
        .max();
    let mut out = old.clone();
    if let Some(l) = latest {
        for f in old.iter().filter(|f| attr(f, "mask_num") == l.to_string()) {
            if [LAND_PATH, LAND_GROUP].contains(&attr(f, "mask_id")) {
                continue;
            }
            let mut f = f.clone();
            set_attr(&mut f, "mask_num", num.to_string());
            out.push(f);
        }
    }
    let template = forms(&parse(MODULES_XMP)?.masks)?;
    for t in template {
        let mut f = t.clone();
        set_attr(&mut f, "mask_num", num.to_string());
        if attr(&t, "mask_id") == LAND_PATH {
            let pts = land_path(skyline);
            set_attr(&mut f, "mask_type", "2".into());
            set_attr(&mut f, "mask_nb", pts.len().to_string());
            let mut bytes = Vec::with_capacity(36 * pts.len());
            for (x, y) in pts {
                for v in [x, y, x, y, x, y, LAND_OFFSET, LAND_OFFSET] {
                    bytes.extend_from_slice(&v.to_le_bytes());
                }
                bytes.extend_from_slice(&2i32.to_le_bytes()); // user-set control points: a polygon
            }
            set_attr(&mut f, "mask_points", hex(&bytes));
        }
        out.push(f);
    }
    let lis: Vec<String> = out
        .iter()
        .map(|f| {
            let a: Vec<String> = f
                .iter()
                .map(|(k, v)| format!("darktable:{k}=\"{}\"", crate::history_edit::escape(v)))
                .collect();
            format!("     <rdf:li {}/>", a.join(" "))
        })
        .collect();
    Ok(format!(
        "<darktable:masks_history>\n    <rdf:Seq>\n{}\n    </rdf:Seq>\n   </darktable:masks_history>",
        lis.join("\n")
    ))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// The land as a closed polygon (fractions of the frame): along the skyline, [`LAND_OFFSET`]
/// below it, then around the bottom outside the frame. Columns without sky are land to the top.
pub fn land_path(skyline: &Skyline) -> Vec<(f32, f32)> {
    let n = skyline.columns.len().max(1);
    let mut pts = Vec::with_capacity(PATH_POINTS + 4);
    for i in 0..PATH_POINTS {
        // the highest land in this stretch of columns, so peaks keep their tips
        let (a, b) = (
            i * n / PATH_POINTS,
            ((i + 1) * n / PATH_POINTS).max(i * n / PATH_POINTS + 1),
        );
        let y = skyline.columns[a..b.min(n)]
            .iter()
            .map(|c| c.unwrap_or(0.0))
            .fold(1.0f32, f32::min);
        let x = (i as f32 + 0.5) / PATH_POINTS as f32;
        pts.push((x, (y + LAND_OFFSET).min(1.05)));
    }
    let (y0, y1) = (pts[0].1, pts[PATH_POINTS - 1].1);
    pts.insert(0, (-0.05, y0));
    pts.push((1.05, y1));
    pts.push((1.05, 1.05));
    pts.push((-0.05, 1.05));
    pts
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
    let mut pts: Vec<(f64, f64)> = Vec::new();
    let mut reached = false;
    for _ in 0..8 {
        let img = render(&build(sidecar_text, &params)?)?;
        let (l50, rg, bg) = sky_stats(&img)?;
        let y = l50.max(1e-4).log2();
        pts.push((params.ev, y));
        if (y - t).abs() < 0.08 && (rg / tr).ln().abs() < 0.06 && (bg / tb).ln().abs() < 0.06 {
            reached = true;
            break;
        }
        reached = (y - t).abs() < 0.15;
        let cl = |f: f64| f.clamp(0.87, 1.15);
        params.wb = (
            (params.wb.0 * cl((tr / rg).powf(0.6))).clamp(1.2, 5.0),
            (params.wb.1 * cl((tb / bg).powf(0.6))).clamp(0.8, 8.0),
        );
        let mut slope = 0.8;
        if let [.., a, b] = pts[..]
            && a.0 != b.0
        {
            slope = ((b.1 - a.1) / (b.0 - a.0)).clamp(0.3, 1.5);
        }
        params.ev = (params.ev + (t - y) / slope).clamp(-3.0, 6.0);
    }

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
        assert_eq!(pts[pts.len() - 1], (-0.05, 1.05));
    }

    #[test]
    fn a_bright_foreground_gets_the_dark_target() {
        assert_eq!(sky_target(1.2), SKY_DARK);
        assert_eq!(sky_target(0.8), SKY_BRIGHT);
    }
}
