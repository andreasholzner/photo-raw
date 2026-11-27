//! Appending a look to a photo's history (US-10, US-21): the look's steps from a template XMP,
//! the white balance and the exposure, and drawn masks along the skyline (US-23).
//!
//! Like the day edit, the tool only appends: disabled copies of the import's steps the look
//! replaces and of the tool's earlier steps no longer wanted, then the look. The tool's own steps
//! and masks are recognised by their names, which start with "photoflow".

use crate::history_edit::{Entry, active, escape, parse, with_instances, write};
use crate::horizon::Skyline;
use crate::sidecar;
use anyhow::{Context, Result, bail, ensure};

const OWN: &str = "photoflow";
/// Points of a path along the skyline.
pub const PATH_POINTS: usize = 48;

/// A look: its template (darktable 5.6.1 presets and the tool's settings) and the import's
/// operations it switches off.
pub struct Look {
    pub modules: &'static str,
    pub replaced: &'static [&'static str],
}

/// A drawn path replacing the template's form with this id.
pub struct Shape {
    pub id: &'static str,
    pub points: Vec<(f32, f32)>,
    /// Feather width, as a fraction of the frame.
    pub border: f32,
}

pub fn set_floats(e: &mut Entry, at: usize, values: &[f32]) -> Result<()> {
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

impl Look {
    /// The template's entry for `op` (with this instance name, if given).
    pub fn template(&self, op: &str, name: &str) -> Result<Entry> {
        parse(self.modules)?
            .entries
            .into_iter()
            .find(|e| e.get("operation") == op && (name.is_empty() || e.get("multi_name") == name))
            .with_context(|| format!("no {op} {name:?} in the look's template"))
    }

    /// The template's own steps in order: everything but the mask manager and the white balance.
    pub fn steps(&self) -> Result<Vec<Entry>> {
        Ok(parse(self.modules)?
            .entries
            .into_iter()
            .filter(|e| !["mask_manager", "temperature"].contains(&e.get("operation")))
            .collect())
    }

    /// The photo's history with `steps` appended after the white balance `wb` (red and blue
    /// coefficients) and the exposure `ev`; `shapes` are the drawn masks the steps use.
    pub fn append(
        &self,
        sidecar_text: &str,
        wb: (f64, f64),
        ev: f64,
        steps: Vec<Entry>,
        shapes: &[Shape],
    ) -> Result<String> {
        let p = parse(sidecar_text)?;
        let mut entries = p.entries.clone();
        let was_active: Vec<Entry> = active(&p.entries).into_iter().cloned().collect();
        let own = |e: &Entry| e.get("multi_name").starts_with(OWN);
        let wanted = |e: &Entry| {
            steps.iter().any(|s| {
                s.get("operation") == e.get("operation")
                    && s.get("multi_name") == e.get("multi_name")
            })
        };
        let mut add: Vec<Entry> = Vec::new();
        for e in &was_active {
            let op = e.get("operation");
            if (own(e) && !wanted(e)) || (!own(e) && self.replaced.contains(&op)) {
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
        let mut temperature = match main("temperature") {
            Some(e) => e,
            None => self.template("temperature", "")?,
        };
        set_floats(&mut temperature, 0, &[wb.0 as f32, 1.0, wb.1 as f32])?;
        add.push(temperature);
        let mut exposure = match main("exposure") {
            Some(e) => e,
            None => parse(crate::day_edit::MODULES_XMP)?
                .entries
                .into_iter()
                .find(|e| e.get("operation") == "exposure")
                .context("no exposure template")?,
        };
        set_floats(&mut exposure, 8, &[ev as f32])?;
        exposure.set("enabled", "1".into());
        add.push(exposure);

        // drawn masks: a new snapshot of the forms, the earlier ones plus the tool's paths
        let mut masks = p.masks.clone();
        if !shapes.is_empty() {
            let mut mm = self.template("mask_manager", "")?;
            mm.set("enabled", "1".into());
            masks = self.with_forms(&p.masks, p.entries.len() + add.len(), shapes)?;
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
            e.set("enabled", "1".into());
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
                &parse(self.modules)?.order_list.context("no order list")?,
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

    /// The masks_history element: the earlier snapshots unchanged, and at history entry `num`
    /// the latest snapshot's forms (without the tool's earlier ones) plus the shapes and the
    /// template's groups that contain them.
    fn with_forms(&self, masks: &str, num: usize, shapes: &[Shape]) -> Result<String> {
        let template = forms(&parse(self.modules)?.masks)?;
        let tool_ids: Vec<&str> = template.iter().map(|f| attr(f, "mask_id")).collect();
        let old = forms(masks)?;
        let latest = old
            .iter()
            .filter_map(|f| attr(f, "mask_num").parse::<usize>().ok())
            .max();
        let mut out = old.clone();
        if let Some(l) = latest {
            for f in old.iter().filter(|f| attr(f, "mask_num") == l.to_string()) {
                if tool_ids.contains(&attr(f, "mask_id")) {
                    continue;
                }
                let mut f = f.clone();
                set_attr(&mut f, "mask_num", num.to_string());
                out.push(f);
            }
        }
        for t in &template {
            let mut f = t.clone();
            set_attr(&mut f, "mask_num", num.to_string());
            if let Some(shape) = shapes.iter().find(|s| s.id == attr(t, "mask_id")) {
                set_attr(&mut f, "mask_type", "2".into());
                set_attr(&mut f, "mask_nb", shape.points.len().to_string());
                set_attr(&mut f, "mask_points", polygon(&shape.points, shape.border));
            } else if attr(t, "mask_type") == "4" {
                // a group: kept when its member is one of the shapes
                let member = sidecar::decode(attr(t, "mask_points"))
                    .ok()
                    .filter(|b| b.len() >= 4)
                    .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]).to_string());
                if !shapes.iter().any(|s| Some(s.id.to_string()) == member) {
                    continue;
                }
            } else {
                continue;
            }
            out.push(f);
        }
        let lis: Vec<String> = out
            .iter()
            .map(|f| {
                let a: Vec<String> = f
                    .iter()
                    .map(|(k, v)| format!("darktable:{k}=\"{}\"", escape(v)))
                    .collect();
                format!("     <rdf:li {}/>", a.join(" "))
            })
            .collect();
        Ok(format!(
            "<darktable:masks_history>\n    <rdf:Seq>\n{}\n    </rdf:Seq>\n   </darktable:masks_history>",
            lis.join("\n")
        ))
    }
}

/// One masks_history entry: its darktable attributes.
pub type Form = Vec<(String, String)>;

pub fn forms(masks: &str) -> Result<Vec<Form>> {
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

pub fn attr<'a>(f: &'a Form, k: &str) -> &'a str {
    f.iter().find(|(n, _)| n == k).map_or("", |(_, v)| v)
}

fn set_attr(f: &mut Form, k: &str, v: String) {
    match f.iter_mut().find(|(n, _)| n == k) {
        Some(x) => x.1 = v,
        None => f.push((k.to_string(), v)),
    }
}

/// A path's points as darktable stores them: corner, both control points at the corner (a
/// polygon), border, state "user-set".
fn polygon(points: &[(f32, f32)], border: f32) -> String {
    let mut bytes = Vec::with_capacity(36 * points.len());
    for &(x, y) in points {
        for v in [x, y, x, y, x, y, border, border] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        bytes.extend_from_slice(&2i32.to_le_bytes());
    }
    bytes.iter().map(|x| format!("{x:02x}")).collect()
}

/// Points along the skyline, `offset` below it: per stretch of columns the highest land, so
/// peaks keep their tips; columns without sky count as land up to the top.
fn along(skyline: &Skyline, offset: f32) -> Vec<(f32, f32)> {
    let n = skyline.columns.len().max(1);
    (0..PATH_POINTS)
        .map(|i| {
            let (a, b) = (
                i * n / PATH_POINTS,
                ((i + 1) * n / PATH_POINTS).max(i * n / PATH_POINTS + 1),
            );
            let y = skyline.columns[a..b.min(n)]
                .iter()
                .map(|c| c.unwrap_or(0.0))
                .fold(1.0f32, f32::min);
            (
                (i as f32 + 0.5) / PATH_POINTS as f32,
                (y + offset).min(1.05),
            )
        })
        .collect()
}

/// The land as a closed polygon (fractions of the frame): along the skyline, `offset` below it,
/// then around the bottom outside the frame.
pub fn land_path(skyline: &Skyline, offset: f32) -> Vec<(f32, f32)> {
    let mut pts = along(skyline, offset);
    let (y0, y1) = (pts[0].1, pts[PATH_POINTS - 1].1);
    // corners on the frame's edge: the feather then lies outside the frame
    pts.insert(0, (0.0, y0));
    pts.push((1.0, y1));
    pts.push((1.0, 1.0));
    pts.push((0.0, 1.0));
    pts
}

/// The sky as a closed polygon: along the skyline, `margin` above it, then around the top.
pub fn sky_path(skyline: &Skyline, margin: f32) -> Vec<(f32, f32)> {
    let mut pts = along(skyline, -margin);
    let (y0, y1) = (pts[0].1, pts[PATH_POINTS - 1].1);
    pts.insert(0, (0.0, y0));
    pts.push((1.0, y1));
    pts.push((1.0, 0.0));
    pts.push((0.0, 0.0));
    pts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line() -> Skyline {
        Skyline {
            columns: (0..96)
                .map(|i| Some(if i < 48 { 0.6 } else { 0.7 }))
                .collect(),
        }
    }

    #[test]
    fn the_paths_follow_the_skyline_and_close_outside_the_frame() {
        let land = land_path(&line(), 0.01);
        assert!((land[1].1 - 0.61).abs() < 1e-6 && (land[PATH_POINTS].1 - 0.71).abs() < 1e-6);
        assert_eq!(land[land.len() - 1], (0.0, 1.0));
        let sky = sky_path(&line(), 0.01);
        assert!((sky[1].1 - 0.59).abs() < 1e-6);
        assert_eq!(sky[sky.len() - 1], (0.0, 0.0));
    }
}
