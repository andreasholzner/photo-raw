//! Reading the edit history from a darktable XMP sidecar, and its fingerprint (US-8).

use anyhow::{Context, Result, bail};
use base64::Engine;
use std::io::Read;
use std::path::Path;

const DT: &str = "http://darktable.sf.net/";

/// One processing step of the active history: the last history entry of a module instance
/// below `history_end`, enabled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub operation: String,
    pub multi_priority: u32,
    pub multi_name: String,
    pub modversion: u32,
    pub params: Vec<u8>,
    pub blendop_version: u32,
    pub blendop_params: Vec<u8>,
    /// Drawn or parametric shapes stored with this step's history entry.
    masks: Vec<Mask>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Mask {
    id: String,
    kind: String,
    version: String,
    points: Vec<u8>,
    nb: String,
    src: Vec<u8>,
}

/// The active history of a sidecar, in pipeline-independent order (by module and instance).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct History {
    pub steps: Vec<Step>,
}

pub fn read(path: &Path) -> Result<History> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    parse(&text).with_context(|| format!("cannot parse {}", path.display()))
}

/// Decodes a darktable parameter blob: hex, or `gzNN` + base64 of zlib data.
fn decode(s: &str) -> Result<Vec<u8>> {
    if let Some(rest) = s.strip_prefix("gz") {
        let b64 = rest.get(2..).context("short gz blob")?;
        let z = base64::engine::general_purpose::STANDARD.decode(b64)?;
        let mut out = Vec::new();
        flate2::read::ZlibDecoder::new(&z[..]).read_to_end(&mut out)?;
        return Ok(out);
    }
    if s.len() % 2 != 0 {
        bail!("odd-length hex blob");
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).context("bad hex blob"))
        .collect()
}

/// Reads a sidecar from its text.
pub fn parse(text: &str) -> Result<History> {
    let doc = roxmltree::Document::parse(text)?;
    let desc = doc
        .descendants()
        .find(|n| n.has_tag_name(("http://www.w3.org/1999/02/22-rdf-syntax-ns#", "Description")))
        .context("no rdf:Description")?;
    let attr = |n: roxmltree::Node, k: &str| n.attribute((DT, k)).unwrap_or("").to_string();
    let num = |n: roxmltree::Node, k: &str| -> Result<u32> {
        let v = attr(n, k);
        if v.is_empty() {
            return Ok(0);
        }
        v.parse().with_context(|| format!("bad {k}: {v}"))
    };
    let end = num(desc, "history_end")? as usize;
    let items = |tag: &str| -> Vec<roxmltree::Node> {
        desc.children()
            .find(|n| n.has_tag_name((DT, tag)))
            .and_then(|h| h.children().find(|n| n.is_element()))
            .map(|seq| seq.children().filter(|n| n.is_element()).collect())
            .unwrap_or_default()
    };

    let mut masks: Vec<(u32, Mask)> = Vec::new();
    for m in items("masks_history") {
        masks.push((
            num(m, "mask_num")?,
            Mask {
                id: attr(m, "mask_id"),
                kind: attr(m, "mask_type"),
                version: attr(m, "mask_version"),
                points: decode(&attr(m, "mask_points"))?,
                nb: attr(m, "mask_nb"),
                src: decode(&attr(m, "mask_src"))?,
            },
        ));
    }

    // Last entry per module instance below history_end wins.
    let mut last: Vec<(String, u32, roxmltree::Node)> = Vec::new();
    for li in items("history").into_iter().take(end) {
        let key = (attr(li, "operation"), num(li, "multi_priority")?);
        last.retain(|(o, p, _)| (o, p) != (&key.0, &key.1));
        last.push((key.0, key.1, li));
    }
    let mut steps = Vec::new();
    for (operation, multi_priority, li) in last {
        if attr(li, "enabled") != "1" {
            continue;
        }
        let n = num(li, "num")?;
        let mut own: Vec<Mask> = masks
            .iter()
            .filter(|(k, _)| *k == n)
            .map(|(_, m)| m.clone())
            .collect();
        own.sort();
        steps.push(Step {
            operation,
            multi_priority,
            multi_name: attr(li, "multi_name"),
            modversion: num(li, "modversion")?,
            params: decode(&attr(li, "params"))?,
            blendop_version: num(li, "blendop_version")?,
            blendop_params: decode(&attr(li, "blendop_params"))?,
            masks: own,
        });
    }
    steps.sort_by(|a, b| (&a.operation, a.multi_priority).cmp(&(&b.operation, b.multi_priority)));
    Ok(History { steps })
}

impl History {
    /// A stable fingerprint of what the history does: independent of how darktable encodes,
    /// numbers or orders the entries, and of undone or superseded entries. Sixteen hex digits.
    pub fn fingerprint(&self) -> String {
        // FNV-1a, 64 bit: stable across Rust versions, unlike std's hasher.
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut feed = |bytes: &[u8]| {
            // length prefix keeps field boundaries unambiguous
            for b in (bytes.len() as u64).to_le_bytes().iter().chain(bytes) {
                h ^= u64::from(*b);
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
        };
        for s in &self.steps {
            feed(s.operation.as_bytes());
            feed(&s.multi_priority.to_le_bytes());
            feed(s.multi_name.as_bytes());
            feed(&s.modversion.to_le_bytes());
            feed(&s.params);
            feed(&s.blendop_version.to_le_bytes());
            feed(&s.blendop_params);
            for m in &s.masks {
                for f in [
                    m.id.as_bytes(),
                    m.kind.as_bytes(),
                    m.version.as_bytes(),
                    &m.points,
                ] {
                    feed(f);
                }
                feed(m.nb.as_bytes());
                feed(&m.src);
            }
        }
        format!("{h:016x}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xmp(history_end: u32, entries: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<rdf:Description xmlns:darktable="http://darktable.sf.net/" darktable:history_end="{history_end}">
<darktable:history><rdf:Seq>{entries}</rdf:Seq></darktable:history>
</rdf:Description></rdf:RDF></x:xmpmeta>"#
        )
    }

    fn entry(num: u32, op: &str, enabled: u8, params: &str) -> String {
        format!(
            r#"<rdf:li darktable:num="{num}" darktable:operation="{op}" darktable:enabled="{enabled}"
 darktable:modversion="6" darktable:params="{params}" darktable:multi_name="" darktable:multi_priority="0"
 darktable:blendop_version="14" darktable:blendop_params="00ff"/>"#
        )
    }

    /// `gz` blob of the given bytes, as darktable writes it.
    fn gz(bytes: &[u8]) -> String {
        use std::io::Write;
        let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
        e.write_all(bytes).unwrap();
        let z = e.finish().unwrap();
        format!(
            "gz04{}",
            base64::engine::general_purpose::STANDARD.encode(z)
        )
    }

    #[test]
    fn active_history_is_the_last_enabled_entry_per_instance_below_the_end() {
        let t = xmp(
            3,
            &[
                entry(0, "exposure", 1, "00"),
                entry(1, "exposure", 1, "01"),
                entry(2, "sharpen", 0, "02"),
                entry(3, "bilat", 1, "03"), // above history_end: undone
            ]
            .concat(),
        );
        let h = parse(&t).unwrap();
        assert_eq!(h.steps.len(), 1);
        assert_eq!(h.steps[0].operation, "exposure");
        assert_eq!(h.steps[0].params, vec![1]);
    }

    #[test]
    fn fingerprint_ignores_encoding_numbering_and_order() {
        let a = xmp(
            2,
            &[entry(0, "exposure", 1, "0102"), entry(1, "bilat", 1, "03")].concat(),
        );
        let b = xmp(
            3,
            &[
                entry(0, "bilat", 1, &gz(&[3])),
                entry(1, "exposure", 1, "ffff"),
                entry(2, "exposure", 1, &gz(&[1, 2])),
            ]
            .concat(),
        );
        assert_eq!(
            parse(&a).unwrap().fingerprint(),
            parse(&b).unwrap().fingerprint()
        );
    }

    #[test]
    fn fingerprint_changes_with_any_parameter() {
        let a = xmp(1, &entry(0, "exposure", 1, "0102"));
        let b = xmp(1, &entry(0, "exposure", 1, "0103"));
        let c = xmp(
            2,
            &[
                entry(0, "exposure", 1, "0102"),
                entry(1, "sharpen", 1, "00"),
            ]
            .concat(),
        );
        let fa = parse(&a).unwrap().fingerprint();
        assert_ne!(fa, parse(&b).unwrap().fingerprint());
        assert_ne!(fa, parse(&c).unwrap().fingerprint());
        assert_eq!(fa.len(), 16);
    }

    /// Reads every sidecar below a folder: `PHOTOFLOW_TEST_PHOTOS=<root> cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn reads_all_real_sidecars() {
        let root = std::env::var("PHOTOFLOW_TEST_PHOTOS").expect("set PHOTOFLOW_TEST_PHOTOS");
        let mut n = 0;
        let mut stack = vec![std::path::PathBuf::from(root)];
        while let Some(dir) = stack.pop() {
            for e in std::fs::read_dir(dir).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.to_string_lossy().ends_with(".ARW.xmp") {
                    let h = read(&p).unwrap();
                    assert!(!h.fingerprint().is_empty());
                    println!(
                        "{}\t{}",
                        p.display(),
                        h.steps
                            .iter()
                            .map(|s| s.operation.as_str())
                            .collect::<Vec<_>>()
                            .join(",")
                    );
                    n += 1;
                }
            }
        }
        assert!(n > 0);
    }
}
