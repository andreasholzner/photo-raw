//! A simulated darktable as the scenarios see it: a photo's history, what the Lua layer reports
//! about it, and the sidecar darktable last wrote. It behaves like darktable 5.6.1 did in the
//! US-20 spike (journal 2026-11-24): darkroom changes are written only when the darkroom is left,
//! lighttable changes at once. Sidecars are real XMP text, read back with the tool's own reader.

use super::{Change, Driver};
use crate::classify::Class;
use crate::edit_state::{EditState, LibraryState, edit_state, edit_tag};
use crate::process::{self, Handover, Step};
use crate::sidecar::{self, History};
use anyhow::{Context, Result};
use base64::Engine;
use std::io::Write;
use std::path::PathBuf;

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

/// The simulated darktable as a scenario driver: one photo per scenario for the edit-state
/// rules, plus a library of folders for the hand-over (US-8). The hand-over follows photoflow's
/// darktable script: newly imported photos at once, others when the owner starts the tool on
/// them; the plan comes from the tool's own planning code.
#[derive(Debug, Default)]
pub struct Sim {
    photo: Photo,
    library: Vec<(PathBuf, Photo)>,
    folders: usize,
    /// Edits the tool handed over (as XMP text), by photo: they replace the photo's sidecar.
    edits: std::collections::HashMap<PathBuf, String>,
}

/// The tools of a simulated run: every photo is a day photo, renders aren't simulated (the
/// exposure is left at 0 EV and counts as reached).
struct SimTools {
    sidecars: std::collections::HashMap<PathBuf, String>,
    written: std::collections::HashMap<PathBuf, String>,
}

impl process::Tools for SimTools {
    fn classify(&mut self, _: &process::Photo) -> Result<Class> {
        Ok(Class::Day)
    }
    fn sidecar_text(&mut self, p: &process::Photo) -> Result<String> {
        self.sidecars.get(&p.raw).cloned().context("no sidecar")
    }
    fn day_edit(
        &mut self,
        _: &process::Photo,
        sidecar: &str,
        wanted: crate::day_edit::Wanted,
    ) -> Result<crate::day_edit::Solved> {
        Ok(crate::day_edit::Solved {
            xmp: crate::day_edit::build(sidecar, wanted, 0.0)?,
            ev: 0.0,
            reached: true,
            snow: false,
        })
    }
    fn write_edit(&mut self, p: &process::Photo, xmp: &str) -> Result<PathBuf> {
        let path = p.raw.with_extension("edit.xmp");
        self.written.insert(path.clone(), xmp.to_string());
        Ok(path)
    }
    fn edits_blocked(&mut self) -> Result<Option<String>> {
        Ok(None)
    }
}

impl Sim {
    fn folder(&mut self, n: usize) -> Vec<PathBuf> {
        self.folders += 1;
        let photos: Vec<PathBuf> = (1..=n)
            .map(|i| PathBuf::from(format!("/sim/{}/{i}.ARW", self.folders)))
            .collect();
        for p in &photos {
            self.library.push((p.clone(), Photo::unedited()));
        }
        photos
    }

    /// Hands `photos` over and carries out the plan (every simulated photo is a day photo).
    fn hand_over(&mut self, photos: &[PathBuf]) {
        let handover = Handover {
            darkroom_open: false,
            photos: photos
                .iter()
                .map(|raw| {
                    let p = &self.library.iter().find(|(r, _)| r == raw).unwrap().1;
                    process::Photo {
                        raw: raw.clone(),
                        sidecar: raw.with_extension("ARW.xmp"),
                        altered: p.library.is_altered,
                        tags: p.library.tags.clone(),
                    }
                })
                .collect(),
        };
        let mut tools = SimTools {
            sidecars: photos
                .iter()
                .map(|raw| {
                    let p = &self.library.iter().find(|(r, _)| r == raw).unwrap().1;
                    let text = self.edits.get(raw).cloned().unwrap_or_else(|| {
                        p.sidecar.as_ref().map(Sidecar::xmp).unwrap_or_default()
                    });
                    (raw.clone(), text)
                })
                .collect(),
            written: Default::default(),
        };
        for (raw, steps) in process::plan(&handover, &mut tools) {
            let photo = &mut self.library.iter_mut().find(|(r, _)| *r == raw).unwrap().1;
            for s in steps {
                let tags = &mut photo.library.tags;
                match s {
                    Step::Clear => {
                        tags.retain(|t| !process::CLEARED_PREFIXES.iter().any(|p| t.starts_with(p)))
                    }
                    Step::Apply(path) => {
                        tags.retain(|t| !t.starts_with(crate::edit_state::EDIT_TAG_PREFIX));
                        photo.library.is_altered = true;
                        self.edits.insert(raw.clone(), tools.written[&path].clone());
                    }
                    Step::Tag(t) => tags.push(t),
                }
            }
        }
    }
}

fn change(h: &mut Hist, change: Change) {
    match change {
        Change::Exposure => h.push(Entry::new("exposure", &[0, 0, 0xe0, 0x3f])),
        Change::ModuleOn => h.push(Entry::new("vignette", &[0, 0, 0x80, 0x3f])),
        Change::ModuleOff => {
            let mut e = Entry::new("bilat", &[9, 10, 11, 12]);
            e.enabled = false;
            h.push(e);
        }
        Change::Mask => {
            let mut e = Entry::new("bilat", &[9, 10, 11, 12]);
            e.blend = vec![1; 8];
            h.push(e);
            let n = h.entries.len() - 1;
            h.masks.push((n, vec![0x3f, 0x00, 0x10, 0x3e]));
        }
    }
}

impl Driver for Sim {
    fn unedited(&mut self) -> Result<()> {
        self.photo = Photo::unedited();
        Ok(())
    }

    fn tool_edit(&mut self) -> Result<()> {
        self.photo = Photo::edited_by_tool();
        Ok(())
    }

    fn change_in_darkroom(&mut self, c: Change) -> Result<()> {
        self.photo.in_darkroom(|h| change(h, c));
        Ok(())
    }

    fn undo(&mut self) -> Result<()> {
        self.photo.in_darkroom(|h| h.end -= 1);
        Ok(())
    }

    fn leave_darkroom(&mut self) -> Result<()> {
        self.photo.leave_darkroom();
        Ok(())
    }

    fn apply_style_in_lighttable(&mut self) -> Result<()> {
        self.photo.in_lighttable(|h| {
            h.push(Entry::new("lens", &[1, 2]));
            h.push(Entry::new("denoiseprofile", &[3, 4]));
        });
        Ok(())
    }

    fn compress_history(&mut self) -> Result<()> {
        self.photo.compress();
        Ok(())
    }

    fn open_other_photo_in_darkroom(&mut self) -> Result<()> {
        self.photo.library.darkroom_open = true;
        Ok(())
    }

    fn edit_state(&mut self) -> Result<EditState> {
        let sidecar = self
            .photo
            .sidecar
            .as_ref()
            .context("darktable wrote no sidecar")?
            .read();
        Ok(edit_state(&self.photo.library, &sidecar))
    }

    fn import_new_folder(&mut self, n: usize) -> Result<Vec<PathBuf>> {
        let photos = self.folder(n);
        self.hand_over(&photos);
        Ok(photos)
    }

    fn old_folder(&mut self, n: usize) -> Result<Vec<PathBuf>> {
        Ok(self.folder(n))
    }

    fn start_tool_on(&mut self, photos: &[PathBuf]) -> Result<()> {
        self.hand_over(photos);
        Ok(())
    }

    fn processed(&mut self) -> Result<Vec<PathBuf>> {
        Ok(self
            .library
            .iter()
            .filter(|(_, p)| {
                p.library
                    .tags
                    .iter()
                    .any(|t| process::CLEARED_PREFIXES.iter().any(|x| t.starts_with(x)))
            })
            .map(|(r, _)| r.clone())
            .collect())
    }
}
