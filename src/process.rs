//! One processing run over the photos darktable hands over (US-8).
//!
//! The Lua layer in darktable decides which photos are handed over (the newly imported ones, or
//! the ones the owner starts the tool on), makes darktable rewrite their sidecars, and writes a
//! hand-over file: what darktable knows about each photo. The tool decides every photo on its
//! own and writes a plan; the Lua layer carries it out in darktable, which stays the only writer
//! of its library and of the sidecars (ADR-0001). Open steps and reports are tags (ADR-0002).
//!
//! Both files are tab-separated text in UTF-8; tag lists are separated by U+001F.
//!
//! Hand-over:
//! ```text
//! darkroom<TAB>0|1
//! photo<TAB><raw path><TAB><sidecar path><TAB>0|1 (altered)<TAB><tags>
//! ```
//! Plan, one or more lines per photo, in this order:
//! ```text
//! <raw path><TAB>clear            remove the tool's earlier class, open-step and report tags
//! <raw path><TAB>tag<TAB><name>   attach a tag
//! ```

use crate::classify::{self, Class};
use crate::edit_state::{EditState, LibraryState, edit_state};
use crate::sidecar;
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

/// The photo's class, as decided by the tool.
pub const CLASS_TAG: &str = "photoflow|class|";
/// An open step for the owner (ADR-0002).
pub const OPEN_TAG: &str = "photoflow|open|";
/// Why the tool left a photo alone.
pub const LEFT_TAG: &str = "photoflow|left alone|";
/// Tags a `clear` removes: what a new run decides afresh.
pub const CLEARED_PREFIXES: [&str; 3] = [CLASS_TAG, OPEN_TAG, LEFT_TAG];

/// What darktable knows about one handed-over photo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Photo {
    pub raw: PathBuf,
    pub sidecar: PathBuf,
    pub altered: bool,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handover {
    /// darktable showed the darkroom when it handed the photos over.
    pub darkroom_open: bool,
    pub photos: Vec<Photo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Clear,
    Tag(String),
}

pub fn parse_handover(text: &str) -> Result<Handover> {
    let mut darkroom_open = None;
    let mut photos = Vec::new();
    for (n, line) in text.lines().enumerate().filter(|(_, l)| !l.is_empty()) {
        let f: Vec<&str> = line.split('\t').collect();
        match f.as_slice() {
            ["darkroom", v] => darkroom_open = Some(*v == "1"),
            ["photo", raw, sidecar, altered, tags] => photos.push(Photo {
                raw: PathBuf::from(raw),
                sidecar: PathBuf::from(sidecar),
                altered: *altered == "1",
                tags: tags
                    .split('\u{1f}')
                    .filter(|t| !t.is_empty())
                    .map(String::from)
                    .collect(),
            }),
            _ => bail!("hand-over line {}: unexpected {line:?}", n + 1),
        }
    }
    Ok(Handover {
        darkroom_open: darkroom_open.context("hand-over without a darkroom line")?,
        photos,
    })
}

/// The plan for the handed-over photos. `class_of` classifies a photo the tool may edit.
pub fn plan(
    handover: &Handover,
    mut class_of: impl FnMut(&Photo) -> Result<Class>,
) -> Vec<(PathBuf, Vec<Step>)> {
    handover
        .photos
        .iter()
        .map(|p| {
            let mut steps = vec![Step::Clear];
            match decide(handover.darkroom_open, p, &mut class_of) {
                Ok(tags) => steps.extend(tags.into_iter().map(Step::Tag)),
                Err(e) => {
                    eprintln!("{}: {e:#}", p.raw.display());
                    steps.push(Step::Tag(format!("{LEFT_TAG}could not be processed")));
                }
            }
            (p.raw.clone(), steps)
        })
        .collect()
}

fn decide(
    darkroom_open: bool,
    p: &Photo,
    class_of: &mut impl FnMut(&Photo) -> Result<Class>,
) -> Result<Vec<String>> {
    let library = LibraryState {
        is_altered: p.altered,
        tags: p.tags.clone(),
        darkroom_open,
    };
    // the sidecar is read only when its history matters; while the darkroom is open it may be
    // stale (journal 2026-11-24)
    let state = if darkroom_open || !p.altered {
        edit_state(&library, &sidecar::History { steps: Vec::new() })
    } else {
        edit_state(&library, &sidecar::read(&p.sidecar)?)
    };
    if let Some(report) = state.report() {
        return Ok(vec![format!("{LEFT_TAG}{report}")]);
    }
    debug_assert!(matches!(state, EditState::Unedited | EditState::OwnEdit));
    // the edit itself comes with US-9 and US-10
    Ok(match class_of(p)? {
        Class::Day => vec![format!("{CLASS_TAG}day")],
        Class::Night => vec![format!("{CLASS_TAG}night")],
        Class::Aurora => vec![format!("{CLASS_TAG}aurora")],
        Class::Ask(q) => vec![format!("{OPEN_TAG}{}", q.open_step())],
    })
}

pub fn format_plan(plan: &[(PathBuf, Vec<Step>)]) -> String {
    let mut out = String::new();
    for (raw, steps) in plan {
        for s in steps {
            let raw = raw.display();
            match s {
                Step::Clear => out.push_str(&format!("{raw}\tclear\n")),
                Step::Tag(t) => out.push_str(&format!("{raw}\ttag\t{t}\n")),
            }
        }
    }
    out
}

/// `photoflow process <hand-over> <plan>`: classifies with EXIF and, for night photos, the
/// aurora check (`check`).
pub fn run(
    handover: &Path,
    plan_file: &Path,
    mut check: impl FnMut(&Path) -> Result<crate::aurora::Image>,
) -> Result<()> {
    let text = std::fs::read_to_string(handover)
        .with_context(|| format!("cannot read {}", handover.display()))?;
    let h = parse_handover(&text)?;
    let p = plan(&h, |photo| {
        let exposure = crate::exif::read(&photo.raw)?;
        classify::classify(&exposure, || check(&photo.raw))
    });
    let tmp = plan_file.with_extension("tmp");
    std::fs::write(&tmp, format_plan(&p))?;
    std::fs::rename(&tmp, plan_file)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classify::Question;

    fn photo(raw: &str, altered: bool, tags: &[&str]) -> String {
        format!(
            "photo\t{raw}\t{raw}.xmp\t{}\t{}",
            altered as u8,
            tags.join("\u{1f}")
        )
    }

    #[test]
    fn handover_is_read() {
        let text = format!(
            "darkroom\t0\n{}\n{}\n",
            photo("/p/a.ARW", false, &[]),
            photo("/p/b.ARW", true, &["x|y", "darktable|changed"])
        );
        let h = parse_handover(&text).unwrap();
        assert!(!h.darkroom_open);
        assert_eq!(h.photos.len(), 2);
        assert_eq!(h.photos[1].tags, vec!["x|y", "darktable|changed"]);
        assert_eq!(h.photos[1].sidecar, PathBuf::from("/p/b.ARW.xmp"));
        assert!(parse_handover("photo\tx\n").is_err());
    }

    #[test]
    fn every_handed_over_photo_gets_a_plan_and_only_those() {
        let text = format!(
            "darkroom\t0\n{}\n{}\n",
            photo("/p/a.ARW", false, &[]),
            photo("/p/b.ARW", false, &[])
        );
        let h = parse_handover(&text).unwrap();
        let p = plan(&h, |ph| {
            Ok(if ph.raw.ends_with("a.ARW") {
                Class::Day
            } else {
                Class::Ask(Question::DayOrNight)
            })
        });
        assert_eq!(
            format_plan(&p),
            "/p/a.ARW\tclear\n/p/a.ARW\ttag\tphotoflow|class|day\n\
             /p/b.ARW\tclear\n/p/b.ARW\ttag\tphotoflow|open|day or night?\n"
        );
    }

    #[test]
    fn darkroom_open_leaves_everything_alone_unread() {
        let text = format!("darkroom\t1\n{}\n", photo("/nowhere/a.ARW", true, &[]));
        let p = plan(&parse_handover(&text).unwrap(), |_| {
            panic!("classified a photo while the darkroom was open")
        });
        assert_eq!(
            p[0].1,
            vec![
                Step::Clear,
                Step::Tag("photoflow|left alone|darkroom is open".into())
            ]
        );
    }

    #[test]
    fn a_photo_that_fails_is_reported_and_the_others_go_on() {
        let text = format!(
            "darkroom\t0\n{}\n{}\n",
            photo("/p/bad.ARW", false, &[]),
            photo("/p/good.ARW", false, &[])
        );
        let p = plan(&parse_handover(&text).unwrap(), |ph| {
            if ph.raw.ends_with("bad.ARW") {
                bail!("no EXIF")
            }
            Ok(Class::Night)
        });
        assert_eq!(
            p[0].1[1],
            Step::Tag("photoflow|left alone|could not be processed".into())
        );
        assert_eq!(p[1].1[1], Step::Tag("photoflow|class|night".into()));
    }
}
