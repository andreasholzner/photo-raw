//! Appending to a photo's history in its sidecar text (US-9, US-10): the entries as darktable
//! writes them, the active state per module instance, the order list, and the XMP written back.

use anyhow::{Context, Result, bail};

/// One history entry: its darktable attributes in order.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry(Vec<(String, String)>);

impl Entry {
    pub fn get(&self, k: &str) -> &str {
        self.0
            .iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.as_str())
            .unwrap_or("")
    }
    pub fn set(&mut self, k: &str, v: String) {
        match self.0.iter_mut().find(|(n, _)| n == k) {
            Some(x) => x.1 = v,
            None => self.0.push((k.to_string(), v)),
        }
    }
    pub fn prio(&self) -> u32 {
        self.get("multi_priority").parse().unwrap_or(0)
    }
    pub fn xml(&self) -> String {
        let attrs: Vec<String> = self
            .0
            .iter()
            .map(|(k, v)| format!("darktable:{k}=\"{}\"", escape(v)))
            .collect();
        format!("     <rdf:li {}/>", attrs.join(" "))
    }
}

pub fn escape(v: &str) -> String {
    v.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

const DT: &str = "http://darktable.sf.net/";
const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";

/// A sidecar's history (up to history_end), order list and masks.
pub struct Parsed {
    pub entries: Vec<Entry>,
    pub order_version: String,
    pub order_list: Option<String>,
    /// The masks_history element as written, kept verbatim.
    pub masks: String,
}

pub fn parse(text: &str) -> Result<Parsed> {
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

/// Active state per instance: the last entry for (operation, priority).
pub fn active(entries: &[Entry]) -> Vec<&Entry> {
    let mut last: Vec<&Entry> = Vec::new();
    for e in entries {
        last.retain(|x| (x.get("operation"), x.prio()) != (e.get("operation"), e.prio()));
        last.push(e);
    }
    last.into_iter()
        .filter(|e| e.get("enabled") == "1")
        .collect()
}

/// `list` ("op,prio,op,prio,…") with each new instance placed right after its module.
pub fn with_instances(list: &str, new: &[(String, u32)]) -> Result<String> {
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

/// The sidecar text: the given attributes, history entries and masks_history element (verbatim).
pub fn write(attrs: &[(&str, String)], entries: &[Entry], masks: &str) -> String {
    let attrs: Vec<String> = attrs
        .iter()
        .map(|(k, v)| format!("darktable:{k}=\"{}\"", escape(v)))
        .collect();
    let history: Vec<String> = entries.iter().map(Entry::xml).collect();
    format!(
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
        masks
    )
}
