//! A simulated darktable as the scenarios see it: a photo's history, what the Lua layer reports
//! about it, and the sidecar darktable last wrote. It behaves like darktable 5.6.1 did in the
//! US-20 spike (journal 2026-11-24): darkroom changes are written only when the darkroom is left,
//! lighttable changes at once. Sidecars are real XMP text, read back with the tool's own reader.

use base64::Engine;
use photoflow::edit_state::{LibraryState, edit_tag};
use photoflow::sidecar::{self, History};
use std::io::Write;

#[derive(Debug, Clone)]
pub struct Entry {
    pub op: &'static str,
    pub enabled: bool,
    pub params: Vec<u8>,
    pub blend: Vec<u8>,
}

impl Entry {
    pub fn new(op: &'static str, params: &[u8]) -> Self {
        Entry {
            op,
            enabled: true,
            params: params.to_vec(),
            blend: vec![0; 8],
        }
    }
}

/// A history stack: entries above `end` are undone. `masks` holds (entry index, shape points).
#[derive(Debug, Clone, Default)]
pub struct Hist {
    pub entries: Vec<Entry>,
    pub end: usize,
    pub masks: Vec<(usize, Vec<u8>)>,
}

impl Hist {
    pub fn push(&mut self, e: Entry) {
        self.entries.truncate(self.end); // a new step after an undo drops the undone ones
        self.entries.push(e);
        self.end = self.entries.len();
    }

    /// Like darktable's "compress history stack": only the active entries, renumbered.
    fn compressed(&self) -> Hist {
        let mut keep: Vec<usize> = Vec::new();
        for i in 0..self.end {
            keep.retain(|&k| self.entries[k].op != self.entries[i].op);
            keep.push(i);
        }
        Hist {
            entries: keep.iter().map(|&i| self.entries[i].clone()).collect(),
            end: keep.len(),
            masks: self
                .masks
                .iter()
                .filter_map(|(i, p)| keep.iter().position(|k| k == i).map(|n| (n, p.clone())))
                .collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Sidecar {
    hist: Hist,
    /// Written after "compress history stack": only active entries, gz-encoded parameters.
    compressed: bool,
}

impl Sidecar {
    pub fn xmp(&self) -> String {
        let hist = if self.compressed {
            self.hist.compressed()
        } else {
            self.hist.clone()
        };
        let blob = |b: &[u8]| if self.compressed { gz(b) } else { hex(b) };
        let entries: String = hist
            .entries
            .iter()
            .enumerate()
            .map(|(n, e)| {
                format!(
                    r#"<rdf:li darktable:num="{n}" darktable:operation="{}" darktable:enabled="{}" darktable:modversion="1" darktable:params="{}" darktable:multi_name="" darktable:multi_priority="0" darktable:blendop_version="14" darktable:blendop_params="{}"/>"#,
                    e.op,
                    u8::from(e.enabled),
                    blob(&e.params),
                    blob(&e.blend)
                )
            })
            .collect();
        let masks: String = hist
            .masks
            .iter()
            .map(|(n, p)| {
                format!(
                    r#"<rdf:li darktable:mask_num="{n}" darktable:mask_id="1" darktable:mask_type="2" darktable:mask_name="drawn" darktable:mask_version="6" darktable:mask_points="{}" darktable:mask_nb="1" darktable:mask_src="0000"/>"#,
                    hex(p)
                )
            })
            .collect();
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<rdf:Description xmlns:darktable="http://darktable.sf.net/" darktable:history_end="{}">
<darktable:masks_history><rdf:Seq>{masks}</rdf:Seq></darktable:masks_history>
<darktable:history><rdf:Seq>{entries}</rdf:Seq></darktable:history>
</rdf:Description></rdf:RDF></x:xmpmeta>"#,
            hist.end
        )
    }

    pub fn read(&self) -> History {
        sidecar::parse(&self.xmp()).expect("the sidecar can be read")
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn gz(b: &[u8]) -> String {
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    e.write_all(b).unwrap();
    format!(
        "gz09{}",
        base64::engine::general_purpose::STANDARD.encode(e.finish().unwrap())
    )
}

#[derive(Debug, Default)]
pub struct Photo {
    pub hist: Hist,
    pub library: LibraryState,
    pub sidecar: Option<Sidecar>,
}

impl Photo {
    /// A fresh import: only what darktable applies by itself.
    pub fn unedited() -> Self {
        let mut p = Photo::default();
        for op in ["rawprepare", "demosaic", "colorin", "colorout"] {
            p.hist.push(Entry::new(op, &[0]));
        }
        p.write_sidecar(false);
        p
    }

    /// The tool's day edit as it hands it over, tagged with its fingerprint.
    pub fn edited_by_tool() -> Self {
        let mut p = Photo::unedited();
        p.in_lighttable(|h| {
            h.push(Entry::new("exposure", &[0, 0, 0xc0, 0x3f]));
            h.push(Entry::new("sigmoid", &[1, 2, 3, 4]));
            h.push(Entry::new("colorbalancergb", &[5, 6, 7, 8]));
            h.push(Entry::new("bilat", &[9, 10, 11, 12]));
        });
        let tag = edit_tag(&p.sidecar.as_ref().unwrap().read());
        p.library.tags.push(tag);
        p
    }

    /// A change in the lighttable (a style, pasted history): the sidecar is written at once.
    pub fn in_lighttable(&mut self, f: impl FnOnce(&mut Hist)) {
        f(&mut self.hist);
        self.library.is_altered = true;
        self.write_sidecar(false);
    }

    /// A change in the darkroom: it reaches the sidecar only when the darkroom is left.
    pub fn in_darkroom(&mut self, f: impl FnOnce(&mut Hist)) {
        self.library.darkroom_open = true;
        f(&mut self.hist);
    }

    pub fn leave_darkroom(&mut self) {
        self.library.darkroom_open = false;
        self.library.is_altered = true;
        self.write_sidecar(false);
    }

    /// "compress history stack" in the lighttable.
    pub fn compress(&mut self) {
        self.write_sidecar(true);
    }

    fn write_sidecar(&mut self, compressed: bool) {
        self.sidecar = Some(Sidecar {
            hist: self.hist.clone(),
            compressed,
        });
    }
}
