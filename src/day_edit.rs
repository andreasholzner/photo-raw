//! The day edit (US-9): the photo's import history plus the day look, with an exposure found by
//! rendering (ADR-0004).
//!
//! The tool never rewrites what darktable applied on import: it appends entries to the photo's
//! history, as darktable does for every change. Appended are: a disabled copy of the import's
//! display transform and of earlier tool steps that are no longer wanted, the exposure, the look
//! (AgX "punchy", or sigmoid "ACES 100-nit like" with color balance rgb "vibrant colors"), local
//! contrast "clarity" and, when switched on, diffuse or sharpen "dehaze". The tool's own steps are
//! recognised by their instance names, which start with "photoflow".

use crate::aurora::Image;
use crate::sidecar;
use anyhow::{Context, Result, bail};

/// The modules the day edit adds, as darktable's built-in presets (darktable 5.6.1).
pub const MODULES_XMP: &str = include_str!("../data/day-modules.xmp");
/// Longest edge of the renders the exposure is solved on; the targets depend on it (US-6).
pub const RENDER_SIZE: u32 = 500;
/// AgX: the brightest 10 % at this luminance (US-6).
pub const AGX_P90: f64 = 0.675;
/// sigmoid: the brightest 1 % at this luminance (US-6).
pub const SIGMOID_P99: f64 = 0.885;
/// Share of bright, colourless pixels above which a photo counts as a snow scene.
pub const SNOW_SHARE: f64 = 0.14;
/// Tag prefixes of the owner's switches (ADR-0002).
pub const LOOK_TAG: &str = "photoflow|look|";
pub const DEHAZE_TAG: &str = "photoflow|dehaze|";
const OWN: &str = "photoflow";
/// Switched off when darktable applied them on import: display transforms (the look replaces
/// them) and sharpening (print sharpening comes with the export, US-5).
const DISPLAY: [&str; 6] = [
    "filmicrgb",
    "sigmoid",
    "agx",
    "basecurve",
    "filmic",
    "sharpen",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    Agx,
    Sigmoid,
}

/// What the owner's tags ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Switches {
    /// `None`: no look tag, the tool's default.
    pub look: Option<Look>,
    /// Both look tags are set.
    pub look_conflict: bool,
    pub dehaze: bool,
}

pub fn switches(tags: &[String]) -> Switches {
    let has = |t: &str| tags.iter().any(|x| x == t);
    let agx = has(&format!("{LOOK_TAG}agx"));
    let sigmoid = has(&format!("{LOOK_TAG}sigmoid"));
    Switches {
        look: match (agx, sigmoid) {
            (true, false) => Some(Look::Agx),
            (false, true) => Some(Look::Sigmoid),
            _ => None,
        },
        look_conflict: agx && sigmoid,
        // "off" wins over "on": the safe side
        dehaze: has(&format!("{DEHAZE_TAG}on")) && !has(&format!("{DEHAZE_TAG}off")),
    }
}

/// The edit the switches ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wanted {
    pub look: Look,
    pub dehaze: bool,
}

impl From<Switches> for Wanted {
    fn from(s: Switches) -> Self {
        Wanted {
            look: s.look.unwrap_or(Look::Agx),
            dehaze: s.dehaze,
        }
    }
}

/// The day edit a history carries, if any: from the tool's active steps.
pub fn current(history: &sidecar::History) -> Option<Wanted> {
    let own = |op: &str| {
        history
            .steps
            .iter()
            .any(|s| s.operation == op && s.multi_name.starts_with(OWN))
    };
    let look = match (own("agx"), own("sigmoid")) {
        (true, false) => Look::Agx,
        (false, true) => Look::Sigmoid,
        _ => return None,
    };
    Some(Wanted {
        look,
        dehaze: own("diffuse"),
    })
}

/// One history entry: its darktable attributes in order.
#[derive(Debug, Clone, PartialEq)]
struct Entry(Vec<(String, String)>);

impl Entry {
    fn get(&self, k: &str) -> &str {
        self.0
            .iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.as_str())
            .unwrap_or("")
    }
    fn set(&mut self, k: &str, v: String) {
        match self.0.iter_mut().find(|(n, _)| n == k) {
            Some(x) => x.1 = v,
            None => self.0.push((k.to_string(), v)),
        }
    }
    fn prio(&self) -> u32 {
        self.get("multi_priority").parse().unwrap_or(0)
    }
    fn xml(&self) -> String {
        let attrs: Vec<String> = self
            .0
            .iter()
            .map(|(k, v)| format!("darktable:{k}=\"{}\"", escape(v)))
            .collect();
        format!("     <rdf:li {}/>", attrs.join(" "))
    }
}

fn escape(v: &str) -> String {
    v.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

const DT: &str = "http://darktable.sf.net/";
const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";

/// A sidecar's history (up to history_end), order list and masks.
struct Parsed {
    entries: Vec<Entry>,
    order_version: String,
    order_list: Option<String>,
    /// The masks_history element as written, kept verbatim.
    masks: String,
}

fn parse(text: &str) -> Result<Parsed> {
    let doc = roxmltree::Document::parse(text)?;
    let desc = doc
        .descendants()
        .find(|n| n.has_tag_name((RDF, "Description")))
        .context("no rdf:Description")?;
    let end: usize = desc.attribute((DT, "history_end")).unwrap_or("0").parse()?;
    let seq = |tag: &str| {
        desc.children()
            .find(|n| n.has_tag_name((DT, tag)))
            .and_then(|h| h.children().find(|n| n.is_element()))
    };
    let entries: Vec<Entry> = seq("history")
        .map(|s| {
            s.children()
                .filter(|n| n.is_element())
                .take(end)
                .map(|li| {
                    Entry(
                        li.attributes()
                            .filter(|a| a.namespace() == Some(DT))
                            .map(|a| (a.name().to_string(), a.value().to_string()))
                            .collect(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    for (i, e) in entries.iter().enumerate() {
        if e.get("num") != i.to_string() {
            bail!("history entries aren't numbered in order");
        }
    }
    let masks = desc
        .children()
        .find(|n| n.has_tag_name((DT, "masks_history")))
        .map(|n| text[n.range()].to_string())
        .unwrap_or_default();
    Ok(Parsed {
        entries,
        order_version: desc
            .attribute((DT, "iop_order_version"))
            .unwrap_or("")
            .to_string(),
        order_list: desc.attribute((DT, "iop_order_list")).map(str::to_string),
        masks,
    })
}

/// The template entry for `op` from [`MODULES_XMP`].
fn template(op: &str) -> Result<Entry> {
    parse(MODULES_XMP)?
        .entries
        .into_iter()
        .find(|e| e.get("operation") == op)
        .with_context(|| format!("no {op} in the day modules"))
}

fn template_order() -> Result<String> {
    parse(MODULES_XMP)?
        .order_list
        .context("no order list in the day modules")
}

/// Active state per instance: the last entry for (operation, priority).
fn active(entries: &[Entry]) -> Vec<&Entry> {
    let mut last: Vec<&Entry> = Vec::new();
    for e in entries {
        last.retain(|x| (x.get("operation"), x.prio()) != (e.get("operation"), e.prio()));
        last.push(e);
    }
    last.into_iter()
        .filter(|e| e.get("enabled") == "1")
        .collect()
}

/// The photo's history with the day edit appended; `ev` is the exposure in EV.
pub fn build(sidecar_text: &str, wanted: Wanted, ev: f64) -> Result<String> {
    let p = parse(sidecar_text)?;
    let mut entries = p.entries.clone();
    let was_active: Vec<Entry> = active(&p.entries).into_iter().cloned().collect();
    let own = |e: &Entry| e.get("multi_name").starts_with(OWN);

    let mut add: Vec<Entry> = Vec::new();
    let mut steps: Vec<&str> = match wanted.look {
        Look::Agx => vec!["agx"],
        Look::Sigmoid => vec!["sigmoid", "colorbalancergb"],
    };
    steps.push("bilat");
    if wanted.dehaze {
        steps.push("diffuse");
    }
    // switch off: the import's display transform and the tool's steps no longer wanted
    for e in &was_active {
        let op = e.get("operation");
        let unwanted_own = own(e) && !steps.contains(&op);
        let import_display = !own(e) && DISPLAY.contains(&op);
        if unwanted_own || import_display {
            let mut off = (*e).clone();
            off.set("enabled", "0".into());
            add.push(off);
        }
    }
    // the exposure: the import's main instance, or the template's
    let mut exposure = was_active
        .iter()
        .find(|e| e.get("operation") == "exposure" && e.prio() == 0 && !own(e))
        .cloned()
        .unwrap_or(template("exposure")?);
    let mut params = sidecar::decode(exposure.get("params"))?;
    if params.len() < 12 {
        bail!("unexpected exposure parameters");
    }
    params[8..12].copy_from_slice(&(ev as f32).to_le_bytes());
    exposure.set("params", sidecar::encode(&params));
    exposure.set("enabled", "1".into());
    add.push(exposure);
    // the look: the tool's instance where there is one, else a free one
    let mut new_instances = Vec::new();
    for op in steps {
        let mut e = template(op)?;
        let prio = match entries.iter().find(|x| x.get("operation") == op && own(x)) {
            Some(x) => x.prio(),
            None => match entries
                .iter()
                .filter(|x| x.get("operation") == op)
                .map(Entry::prio)
                .max()
            {
                Some(m) => m + 1,
                None => 0,
            },
        };
        e.set("multi_priority", prio.to_string());
        if prio > 0 {
            new_instances.push((op.to_string(), prio));
        }
        add.push(e);
    }
    for (i, mut e) in add.into_iter().enumerate() {
        e.set("num", (p.entries.len() + i).to_string());
        entries.push(e);
    }

    // an extra instance needs an explicit order list (journal 2026-11-23)
    let (version, list) = match (&p.order_list, new_instances.is_empty()) {
        (Some(l), _) => (
            p.order_version.clone(),
            Some(with_instances(l, &new_instances)?),
        ),
        (None, true) => (p.order_version.clone(), None),
        (None, false) if p.order_version == "4" || p.order_version.is_empty() => (
            "4".to_string(),
            Some(with_instances(&template_order()?, &new_instances)?),
        ),
        (None, false) => bail!("can't add a second module instance to this history"),
    };
    let mut attrs = vec![
        ("xmp_version", "5".to_string()),
        ("auto_presets_applied", "1".to_string()),
        ("history_end", entries.len().to_string()),
    ];
    if !version.is_empty() {
        attrs.push(("iop_order_version", version));
    }
    if let Some(l) = list {
        attrs.push(("iop_order_list", l));
    }
    let attrs: Vec<String> = attrs
        .iter()
        .map(|(k, v)| format!("darktable:{k}=\"{}\"", escape(v)))
        .collect();
    let history: Vec<String> = entries.iter().map(Entry::xml).collect();
    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/" x:xmptk="XMP Core 4.4.0-Exiv2">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about="" xmlns:darktable="http://darktable.sf.net/" {}>
   <darktable:history>
    <rdf:Seq>
{}
    </rdf:Seq>
   </darktable:history>
   {}
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>
"#,
        attrs.join(" "),
        history.join("\n"),
        p.masks
    ))
}

/// `list` ("op,prio,op,prio,…") with each new instance placed right after its module.
fn with_instances(list: &str, new: &[(String, u32)]) -> Result<String> {
    let f: Vec<&str> = list.split(',').collect();
    let mut pairs: Vec<(String, String)> = f
        .chunks(2)
        .map(|c| (c[0].to_string(), c.get(1).unwrap_or(&"0").to_string()))
        .collect();
    for (op, prio) in new {
        let p = prio.to_string();
        if pairs.iter().any(|(o, q)| o == op && *q == p) {
            continue;
        }
        let at = pairs
            .iter()
            .rposition(|(o, _)| o == op)
            .with_context(|| format!("{op} missing from the order list"))?;
        pairs.insert(at + 1, (op.clone(), p));
    }
    Ok(pairs
        .iter()
        .map(|(o, p)| format!("{o},{p}"))
        .collect::<Vec<_>>()
        .join(","))
}

/// Luminance statistics of a render: the given upper percentile and the snow share.
pub struct Stats {
    pub percentile: f64,
    pub snow: f64,
}

pub fn stats(img: &Image, percentile: f64) -> Stats {
    let n = img.width * img.height;
    let mut lum = Vec::with_capacity(n);
    let mut snow = 0usize;
    for i in 0..n {
        let (r, g, b) = (
            f64::from(img.rgb[3 * i]),
            f64::from(img.rgb[3 * i + 1]),
            f64::from(img.rgb[3 * i + 2]),
        );
        let l = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        if l > 0.7 && r.max(g).max(b) - r.min(g).min(b) < 0.1 {
            snow += 1;
        }
        lum.push(l);
    }
    lum.sort_by(f64::total_cmp);
    Stats {
        percentile: lum[((percentile * n as f64) as usize).min(n - 1)],
        snow: snow as f64 / n as f64,
    }
}

/// Result of solving the exposure.
pub struct Solved {
    pub xmp: String,
    pub ev: f64,
    /// The target was met (within 0.1 in log2 of the statistic).
    pub reached: bool,
    pub snow: bool,
}

/// Solves the exposure for `wanted`, rendering with `render` (an edit's XMP text → image).
pub fn solve(
    sidecar_text: &str,
    wanted: Wanted,
    mut render: impl FnMut(&str) -> Result<Image>,
) -> Result<Solved> {
    let (pct, target, slope0, start) = match wanted.look {
        Look::Agx => (0.90, AGX_P90, 0.6, 1.0),
        Look::Sigmoid => (0.99, SIGMOID_P99, 0.3, 1.0),
    };
    let t = target.log2();
    let mut ev = start;
    let mut pts: Vec<(f64, f64)> = Vec::new();
    let mut last = None;
    for _ in 0..6 {
        let xmp = build(sidecar_text, wanted, ev)?;
        let s = stats(&render(&xmp)?, pct);
        let y = s.percentile.max(1e-4).log2();
        pts.push((ev, y));
        last = Some((xmp, ev, y, s.snow));
        if (y - t).abs() < 0.04 {
            break;
        }
        let slope = match pts.len() {
            n if n > 1 && pts[n - 1].0 != pts[n - 2].0 => {
                ((pts[n - 1].1 - pts[n - 2].1) / (pts[n - 1].0 - pts[n - 2].0)).clamp(0.1, 2.0)
            }
            _ => slope0,
        };
        ev += ((t - y) / slope).clamp(-3.0, 3.0);
    }
    let (xmp, ev, y, snow) = last.context("no render")?;
    Ok(Solved {
        xmp,
        ev,
        reached: (y - t).abs() <= 0.1,
        snow: snow > SNOW_SHARE,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(t: &[&str]) -> Vec<String> {
        t.iter().map(|s| s.to_string()).collect()
    }

    fn import(extra: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<rdf:Description xmlns:darktable="http://darktable.sf.net/" darktable:history_end="3" darktable:iop_order_version="4">
<darktable:history><rdf:Seq>
<rdf:li darktable:num="0" darktable:operation="lens" darktable:enabled="1" darktable:modversion="10" darktable:params="00" darktable:multi_name="" darktable:multi_priority="0" darktable:blendop_version="14" darktable:blendop_params="00"/>
<rdf:li darktable:num="1" darktable:operation="exposure" darktable:enabled="1" darktable:modversion="7" darktable:params="00000000000000000000003f0000000000000000" darktable:multi_name="" darktable:multi_priority="0" darktable:blendop_version="14" darktable:blendop_params="00"/>
<rdf:li darktable:num="2" darktable:operation="sigmoid" darktable:enabled="1" darktable:modversion="3" darktable:params="00" darktable:multi_name="" darktable:multi_priority="0" darktable:blendop_version="14" darktable:blendop_params="00"/>
{extra}</rdf:Seq></darktable:history>
</rdf:Description></rdf:RDF></x:xmpmeta>"#
        )
    }

    fn ops(xmp: &str) -> Vec<(String, u32, String)> {
        sidecar::parse(xmp)
            .unwrap()
            .steps
            .iter()
            .map(|s| (s.operation.clone(), s.multi_priority, s.multi_name.clone()))
            .collect()
    }

    #[test]
    fn switches_from_tags() {
        let s = switches(&tags(&[]));
        assert_eq!((s.look, s.look_conflict, s.dehaze), (None, false, false));
        assert_eq!(
            switches(&tags(&["photoflow|look|sigmoid"])).look,
            Some(Look::Sigmoid)
        );
        assert!(switches(&tags(&["photoflow|look|sigmoid", "photoflow|look|agx"])).look_conflict);
        assert!(switches(&tags(&["photoflow|dehaze|on"])).dehaze);
        assert!(!switches(&tags(&["photoflow|dehaze|on", "photoflow|dehaze|off"])).dehaze);
    }

    #[test]
    fn the_import_stays_and_the_look_replaces_its_display_transform() {
        let agx = Wanted {
            look: Look::Agx,
            dehaze: false,
        };
        let x = build(&import(""), agx, 1.5).unwrap();
        let active = ops(&x);
        assert!(active.iter().any(|(o, _, _)| o == "lens"));
        assert!(
            !active
                .iter()
                .any(|(o, _, n)| o == "sigmoid" && n.is_empty())
        );
        assert!(
            active
                .iter()
                .any(|(o, _, n)| o == "agx" && n.starts_with("photoflow"))
        );
        assert!(
            active
                .iter()
                .any(|(o, _, n)| o == "bilat" && n.starts_with("photoflow"))
        );
        assert!(!active.iter().any(|(o, _, _)| o == "sharpen"));
        // the import's own entries are untouched and come first
        let h = parse(&x).unwrap();
        assert_eq!(h.entries[..3], parse(&import("")).unwrap().entries[..]);
        assert_eq!(current(&sidecar::parse(&x).unwrap()), Some(agx));
        // the exposure is set
        let e = sidecar::parse(&x).unwrap();
        let exp = e.steps.iter().find(|s| s.operation == "exposure").unwrap();
        assert_eq!(
            f32::from_le_bytes(exp.params[8..12].try_into().unwrap()),
            1.5
        );
    }

    #[test]
    fn switching_replaces_the_tools_steps() {
        let first = build(
            &import(""),
            Wanted {
                look: Look::Agx,
                dehaze: true,
            },
            1.0,
        )
        .unwrap();
        let second = build(
            &first,
            Wanted {
                look: Look::Sigmoid,
                dehaze: false,
            },
            0.8,
        )
        .unwrap();
        let active = ops(&second);
        assert!(!active.iter().any(|(o, _, _)| o == "agx"));
        assert!(!active.iter().any(|(o, _, _)| o == "diffuse"));
        assert!(
            active
                .iter()
                .any(|(o, _, n)| o == "sigmoid" && n.starts_with("photoflow"))
        );
        assert!(
            active
                .iter()
                .any(|(o, _, n)| o == "colorbalancergb" && n.starts_with("photoflow"))
        );
        assert_eq!(
            current(&sidecar::parse(&second).unwrap()),
            Some(Wanted {
                look: Look::Sigmoid,
                dehaze: false
            })
        );
        // the same edit twice: the same active steps
        let again = build(
            &second,
            Wanted {
                look: Look::Sigmoid,
                dehaze: false,
            },
            0.8,
        )
        .unwrap();
        assert_eq!(
            sidecar::parse(&again).unwrap().fingerprint(),
            sidecar::parse(&second).unwrap().fingerprint()
        );
    }

    #[test]
    fn an_existing_instance_gets_a_second_one_with_an_order_list() {
        let own_bilat = r#"<rdf:li darktable:num="3" darktable:operation="bilat" darktable:enabled="1" darktable:modversion="3" darktable:params="00" darktable:multi_name="" darktable:multi_priority="0" darktable:blendop_version="14" darktable:blendop_params="00"/>
"#;
        let x = import(own_bilat).replace("history_end=\"3\"", "history_end=\"4\"");
        let e = build(
            &x,
            Wanted {
                look: Look::Agx,
                dehaze: false,
            },
            1.0,
        )
        .unwrap();
        assert!(ops(&e).contains(&("bilat".into(), 0, "".into())));
        assert!(ops(&e).contains(&("bilat".into(), 1, "photoflow clarity".into())));
        assert!(e.contains("bilat,0,bilat,1"));
    }
}
