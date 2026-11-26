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
//! <raw path><TAB>apply<TAB><xmp>  replace the photo's history with this sidecar's, and remove
//!                                 the fingerprint tag of the tool's earlier edit
//! <raw path><TAB>tag<TAB><name>   attach a tag
//! <raw path><TAB>untag<TAB><prefix>  detach every tag starting with this
//! ```
//! A photo whose edit is already what its switches ask for gets no lines: nothing changes.

use crate::classify::{self, Class};
use crate::day_edit::{self, Wanted};
use crate::edit_state::edit_tag;
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
    Apply(PathBuf),
    Tag(String),
    Untag(String),
}

/// What a processing run needs besides the hand-over: darktable-cli, the photos' files.
pub trait Tools {
    /// Day, night, aurora or a question (EXIF, and the aurora check for night photos).
    fn classify(&mut self, p: &Photo) -> Result<Class>;
    fn sidecar_text(&mut self, p: &Photo) -> Result<String>;
    /// Solves the day edit's exposure by rendering (US-9).
    fn day_edit(&mut self, p: &Photo, sidecar: &str, wanted: Wanted) -> Result<day_edit::Solved>;
    /// Share of the frame with clipped raw highlights (US-9).
    fn clipped_share(&mut self, p: &Photo) -> Result<f64>;
    /// Writes an edit for darktable to apply; returns its path.
    fn write_edit(&mut self, p: &Photo, xmp: &str) -> Result<PathBuf>;
    /// When and with which focal length the photo was taken (US-12).
    fn capture(&mut self, p: &Photo) -> Result<crate::exif::Capture>;
    /// Why automatic edits are blocked, if they are (a darktable upgrade, US-9).
    fn edits_blocked(&mut self) -> Result<Option<String>>;
}

/// The open steps and reports of US-9.
pub const CHECK_EXPOSURE: &str = "check exposure";
pub const SNOW_CHECK_LOOK: &str = "snow: check look";
pub const LOOK_CONFLICT: &str = "look: agx or sigmoid?";

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

/// The plan for the handed-over photos; each photo is decided on its own, then the photos are
/// looked at together for panorama proposals (US-12).
pub fn plan(handover: &Handover, tools: &mut dyn Tools) -> Vec<(PathBuf, Vec<Step>)> {
    let mut plan: Vec<(PathBuf, Vec<Step>)> = handover
        .photos
        .iter()
        .map(|p| {
            let steps = match decide(handover.darkroom_open, p, tools) {
                Ok(Some(steps)) => steps,
                Ok(None) => Vec::new(),
                Err(e) => {
                    eprintln!("{}: {e:#}", p.raw.display());
                    vec![
                        Step::Clear,
                        Step::Tag(format!("{LEFT_TAG}could not be processed")),
                    ]
                }
            };
            (p.raw.clone(), steps)
        })
        .collect();
    if !handover.darkroom_open {
        propose_panoramas(handover, tools, &mut plan);
    }
    plan
}

/// Proposes panorama sets among the handed-over photos and replaces their earlier proposals.
fn propose_panoramas(
    handover: &Handover,
    tools: &mut dyn Tools,
    plan: &mut [(PathBuf, Vec<Step>)],
) {
    use crate::panorama::{self, Shot};
    let mut shots = Vec::new();
    for p in &handover.photos {
        let capture = tools.capture(p).ok();
        shots.push(Shot {
            name: p
                .raw
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            time: capture.map_or(f64::NAN, |c| c.time),
            focal_mm: capture.and_then(|c| c.focal_mm),
            excluded: capture.is_none() || panorama::excluded(&p.tags),
        });
    }
    let open = format!("{OPEN_TAG}{}", panorama::OPEN_STEP);
    for (i, shot) in shots.iter().enumerate() {
        if !shot.excluded {
            plan[i]
                .1
                .push(Step::Untag(panorama::PROPOSAL_TAG.to_string()));
            plan[i].1.push(Step::Untag(open.clone()));
        }
    }
    for set in panorama::proposals(&shots) {
        let name = &shots[set[0]].name;
        for &i in &set {
            plan[i]
                .1
                .push(Step::Tag(format!("{}{name}", panorama::PROPOSAL_TAG)));
            plan[i].1.push(Step::Tag(open.clone()));
        }
    }
}

/// The steps for one photo; `None`: its edit is already right, nothing changes.
fn decide(darkroom_open: bool, p: &Photo, tools: &mut dyn Tools) -> Result<Option<Vec<Step>>> {
    let library = LibraryState {
        is_altered: p.altered,
        tags: p.tags.clone(),
        darkroom_open,
    };
    // the sidecar is read only when its history matters; while the darkroom is open it may be
    // stale (journal 2026-11-24)
    let history = if darkroom_open || !p.altered {
        sidecar::History { steps: Vec::new() }
    } else {
        sidecar::parse(&tools.sidecar_text(p)?)?
    };
    let state = edit_state(&library, &history);
    let tag = |t: String| Step::Tag(t);
    if let Some(report) = state.report() {
        return Ok(Some(vec![Step::Clear, tag(format!("{LEFT_TAG}{report}"))]));
    }
    debug_assert!(matches!(state, EditState::Unedited | EditState::OwnEdit));
    let class = match tools.classify(p)? {
        Class::Day => "day",
        Class::Night => "night",
        Class::Aurora => "aurora",
        Class::Ask(q) => {
            return Ok(Some(vec![
                Step::Clear,
                tag(format!("{OPEN_TAG}{}", q.open_step())),
            ]));
        }
    };
    let mut steps = vec![Step::Clear, tag(format!("{CLASS_TAG}{class}"))];
    if class != "day" {
        // the night and aurora edits come with US-10 and US-21
        return Ok(Some(steps));
    }

    // US-9: the day edit
    let switches = day_edit::switches(&p.tags);
    if switches.look_conflict {
        steps.push(tag(format!("{OPEN_TAG}{LOOK_CONFLICT}")));
        return Ok(Some(steps));
    }
    let wanted = Wanted::from(switches);
    if state == EditState::OwnEdit && day_edit::current(&history) == Some(wanted) {
        // the edit stays; a look tag that confirms it answers the snow question
        let snow = format!("{OPEN_TAG}{SNOW_CHECK_LOOK}");
        if switches.look.is_some() && p.tags.contains(&snow) {
            let keep = p
                .tags
                .iter()
                .filter(|t| (t.starts_with(OPEN_TAG) && **t != snow) || t.starts_with(LEFT_TAG));
            steps.extend(keep.cloned().map(tag));
            return Ok(Some(steps));
        }
        return Ok(None);
    }
    if let Some(why) = tools.edits_blocked()? {
        steps.push(tag(format!("{LEFT_TAG}{why}")));
        return Ok(Some(steps));
    }
    let text = tools.sidecar_text(p)?;
    let solved = tools.day_edit(p, &text, wanted)?;
    let path = tools.write_edit(p, &solved.xmp)?;
    steps.insert(1, Step::Apply(path));
    steps.push(tag(edit_tag(&sidecar::parse(&solved.xmp)?)));
    if !solved.reached || tools.clipped_share(p)? > day_edit::CLIP_SHARE {
        steps.push(tag(format!("{OPEN_TAG}{CHECK_EXPOSURE}")));
    }
    if solved.snow && switches.look.is_none() {
        steps.push(tag(format!("{OPEN_TAG}{SNOW_CHECK_LOOK}")));
    }
    Ok(Some(steps))
}

pub fn format_plan(plan: &[(PathBuf, Vec<Step>)]) -> String {
    let mut out = String::new();
    for (raw, steps) in plan {
        for s in steps {
            let raw = raw.display();
            match s {
                Step::Clear => out.push_str(&format!("{raw}\tclear\n")),
                Step::Apply(x) => out.push_str(&format!("{raw}\tapply\t{}\n", x.display())),
                Step::Tag(t) => out.push_str(&format!("{raw}\ttag\t{t}\n")),
                Step::Untag(t) => out.push_str(&format!("{raw}\tuntag\t{t}\n")),
            }
        }
    }
    out
}

/// `photoflow process <hand-over> <plan>`.
pub fn run(handover: &Path, plan_file: &Path, tools: &mut dyn Tools) -> Result<()> {
    let text = std::fs::read_to_string(handover)
        .with_context(|| format!("cannot read {}", handover.display()))?;
    let h = parse_handover(&text)?;
    let p = plan(&h, tools);
    let tmp = plan_file.with_extension("tmp");
    std::fs::write(&tmp, format_plan(&p))?;
    std::fs::rename(&tmp, plan_file)?;
    Ok(())
}

/// The tools of a real run: EXIF, darktable-cli, files below `work`.
pub struct RealTools<'a> {
    pub cfg: &'a crate::config::Config,
    pub work: PathBuf,
    dt: Option<crate::darktable::Darktable>,
    check: Option<classify::AuroraCheck>,
}

impl<'a> RealTools<'a> {
    pub fn new(cfg: &'a crate::config::Config) -> Result<Self> {
        Ok(Self {
            cfg,
            work: cfg.data_dir()?.join("process"),
            dt: None,
            check: None,
        })
    }

    fn darktable(&mut self) -> Result<&crate::darktable::Darktable> {
        if self.dt.is_none() {
            self.dt = Some(crate::darktable::Darktable::prepare(
                &self.cfg.darktable.cli,
                &self.cfg.darktable_config_dir()?,
                &self.work.join("darktable-cli"),
            )?);
        }
        Ok(self.dt.as_ref().unwrap())
    }

    fn render_edit(&mut self, raw: &Path, xmp: &str) -> Result<crate::aurora::Image> {
        let dir = self.work.join("renders");
        std::fs::create_dir_all(&dir)?;
        let name = raw
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let x = dir.join(format!("{name}.day.xmp"));
        let o = dir.join(format!("{name}.day.pfm"));
        std::fs::write(&x, xmp)?;
        self.darktable()?
            .render(raw, &x, &o, day_edit::RENDER_SIZE)?;
        crate::aurora::load_pfm(&o)
    }
}

impl Tools for RealTools<'_> {
    fn classify(&mut self, p: &Photo) -> Result<Class> {
        let exposure = crate::exif::read(&p.raw)?;
        if self.check.is_none() && crate::scene::classify(&exposure) == crate::scene::Scene::Night {
            let dt = crate::darktable::Darktable::prepare(
                &self.cfg.darktable.cli,
                &self.cfg.darktable_config_dir()?,
                &self.work.join("aurora-check"),
            )?;
            self.check = Some(classify::AuroraCheck::new(
                dt,
                &self.work.join("aurora-renders"),
            )?);
        }
        let check = self.check.as_ref();
        classify::classify(&exposure, || {
            check
                .context("no aurora check")
                .and_then(|c| c.render(&p.raw))
        })
    }

    fn sidecar_text(&mut self, p: &Photo) -> Result<String> {
        std::fs::read_to_string(&p.sidecar)
            .with_context(|| format!("cannot read {}", p.sidecar.display()))
    }

    fn day_edit(&mut self, p: &Photo, sidecar: &str, wanted: Wanted) -> Result<day_edit::Solved> {
        let raw = p.raw.clone();
        day_edit::solve(sidecar, wanted, |xmp| self.render_edit(&raw, xmp))
    }

    fn clipped_share(&mut self, p: &Photo) -> Result<f64> {
        let dir = self.work.join("renders");
        std::fs::create_dir_all(&dir)?;
        let name = p
            .raw
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let x = dir.join("clip-check.xmp");
        let o = dir.join(format!("{name}.clip.pfm"));
        std::fs::write(&x, day_edit::CLIP_CHECK_XMP)?;
        self.darktable()?
            .render(&p.raw, &x, &o, crate::aurora::RENDER_SIZE)?;
        Ok(day_edit::clipped_share(&crate::aurora::load_pfm(&o)?))
    }

    fn write_edit(&mut self, p: &Photo, xmp: &str) -> Result<PathBuf> {
        let dir = self.work.join("edits");
        std::fs::create_dir_all(&dir)?;
        let name = p
            .raw
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let path = dir.join(format!("{name}.xmp"));
        std::fs::write(&path, xmp)?;
        Ok(path)
    }

    fn capture(&mut self, p: &Photo) -> Result<crate::exif::Capture> {
        crate::exif::capture(&p.raw)
    }

    fn edits_blocked(&mut self) -> Result<Option<String>> {
        // the self-test's own darktable: the self-test is what lifts the block
        if std::env::var_os("PHOTOFLOW_SELFTEST_IPC").is_some() {
            return Ok(None);
        }
        let installed = self.darktable()?.version()?;
        let accepted = crate::selftest::accepted_darktable(self.cfg)?;
        Ok(upgrade_block(accepted.as_deref(), &installed).map(str::to_string))
    }
}

/// The report when edits are blocked: the self-test hasn't been accepted with the installed
/// darktable (US-9).
pub fn upgrade_block(accepted: Option<&str>, installed: &str) -> Option<&'static str> {
    (accepted != Some(installed)).then_some("run the self-test after the darktable upgrade")
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

    /// Tools with a given classification; day edits are built at 0 EV without rendering.
    struct Fake<F: FnMut(&Photo) -> Result<Class>> {
        class: F,
        blocked: Option<String>,
        reached: bool,
        snow: bool,
    }

    fn fake<F: FnMut(&Photo) -> Result<Class>>(class: F) -> Fake<F> {
        Fake {
            class,
            blocked: None,
            reached: true,
            snow: false,
        }
    }

    const IMPORT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<rdf:Description xmlns:darktable="http://darktable.sf.net/" darktable:history_end="1" darktable:iop_order_version="4">
<darktable:history><rdf:Seq>
<rdf:li darktable:num="0" darktable:operation="exposure" darktable:enabled="1" darktable:modversion="7" darktable:params="00000000000000000000003f0000000000000000" darktable:multi_name="" darktable:multi_priority="0" darktable:blendop_version="14" darktable:blendop_params="00"/>
</rdf:Seq></darktable:history></rdf:Description></rdf:RDF></x:xmpmeta>"#;

    impl<F: FnMut(&Photo) -> Result<Class>> Tools for Fake<F> {
        fn classify(&mut self, p: &Photo) -> Result<Class> {
            (self.class)(p)
        }
        fn sidecar_text(&mut self, _: &Photo) -> Result<String> {
            Ok(IMPORT.to_string())
        }
        fn day_edit(
            &mut self,
            _: &Photo,
            sidecar: &str,
            wanted: Wanted,
        ) -> Result<day_edit::Solved> {
            Ok(day_edit::Solved {
                xmp: day_edit::build(sidecar, wanted, 0.0)?,
                ev: 0.0,
                reached: self.reached,
                snow: self.snow,
            })
        }
        fn clipped_share(&mut self, _: &Photo) -> Result<f64> {
            Ok(if self.reached { 0.0 } else { 0.01 })
        }
        fn write_edit(&mut self, p: &Photo, _: &str) -> Result<PathBuf> {
            Ok(p.raw.with_extension("edit.xmp"))
        }
        fn edits_blocked(&mut self) -> Result<Option<String>> {
            Ok(self.blocked.clone())
        }
        fn capture(&mut self, _: &Photo) -> Result<crate::exif::Capture> {
            bail!("no capture time")
        }
    }

    fn tags_of(steps: &[Step]) -> Vec<String> {
        steps
            .iter()
            .filter_map(|s| match s {
                Step::Tag(t) if !t.starts_with(crate::edit_state::EDIT_TAG_PREFIX) => {
                    Some(t.clone())
                }
                _ => None,
            })
            .collect()
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
        let p = plan(
            &h,
            &mut fake(|ph: &Photo| {
                Ok(if ph.raw.ends_with("a.ARW") {
                    Class::Night
                } else {
                    Class::Ask(Question::DayOrNight)
                })
            }),
        );
        assert_eq!(
            format_plan(&p),
            "/p/a.ARW\tclear\n/p/a.ARW\ttag\tphotoflow|class|night\n\
             /p/b.ARW\tclear\n/p/b.ARW\ttag\tphotoflow|open|day or night?\n"
        );
    }

    #[test]
    fn a_day_photo_gets_its_edit_applied_and_tagged() {
        let text = format!("darkroom\t0\n{}\n", photo("/p/a.ARW", false, &[]));
        let p = plan(
            &parse_handover(&text).unwrap(),
            &mut fake(|_: &Photo| Ok(Class::Day)),
        );
        let steps = &p[0].1;
        assert_eq!(steps[0], Step::Clear);
        assert_eq!(steps[1], Step::Apply(PathBuf::from("/p/a.edit.xmp")));
        assert!(
            steps
                .iter()
                .any(|s| matches!(s, Step::Tag(t) if t.starts_with("photoflow|edit|")))
        );
        assert_eq!(tags_of(steps), vec!["photoflow|class|day"]);
    }

    #[test]
    fn uncertain_day_photos_are_flagged() {
        let text = format!(
            "darkroom\t0\n{}\n{}\n",
            photo("/p/a.ARW", false, &[]),
            photo("/p/b.ARW", false, &["photoflow|look|agx"])
        );
        let mut t = fake(|_: &Photo| Ok(Class::Day));
        t.reached = false;
        t.snow = true;
        let p = plan(&parse_handover(&text).unwrap(), &mut t);
        assert_eq!(
            tags_of(&p[0].1),
            vec![
                "photoflow|class|day",
                "photoflow|open|check exposure",
                "photoflow|open|snow: check look"
            ]
        );
        // a look chosen: no snow question
        assert_eq!(
            tags_of(&p[1].1),
            vec!["photoflow|class|day", "photoflow|open|check exposure"]
        );
    }

    #[test]
    fn a_look_tag_confirming_the_edit_answers_the_snow_question_only() {
        // a photo the tool edited in AgX, with the snow question and another open step
        let edited = day_edit::build(
            IMPORT,
            Wanted {
                look: day_edit::Look::Agx,
                dehaze: false,
            },
            0.0,
        )
        .unwrap();
        let fp = edit_tag(&sidecar::parse(&edited).unwrap());
        struct Edited(String);
        impl Tools for Edited {
            fn classify(&mut self, _: &Photo) -> Result<Class> {
                Ok(Class::Day)
            }
            fn sidecar_text(&mut self, _: &Photo) -> Result<String> {
                Ok(self.0.clone())
            }
            fn day_edit(&mut self, _: &Photo, _: &str, _: Wanted) -> Result<day_edit::Solved> {
                panic!("re-edited")
            }
            fn clipped_share(&mut self, _: &Photo) -> Result<f64> {
                panic!("re-edited")
            }
            fn write_edit(&mut self, _: &Photo, _: &str) -> Result<PathBuf> {
                panic!("re-edited")
            }
            fn capture(&mut self, _: &Photo) -> Result<crate::exif::Capture> {
                bail!("no capture time")
            }
            fn edits_blocked(&mut self) -> Result<Option<String>> {
                Ok(None)
            }
        }
        let tags = [
            "photoflow|class|day",
            fp.as_str(),
            "photoflow|open|snow: check look",
            "photoflow|open|other",
        ];
        let text = format!(
            "darkroom\t0\n{}\n{}\n",
            photo("/p/a.ARW", true, &tags),
            photo(
                "/p/b.ARW",
                true,
                &[tags.as_slice(), &["photoflow|look|agx"]].concat()
            )
        );
        let p = plan(&parse_handover(&text).unwrap(), &mut Edited(edited));
        assert!(p[0].1.is_empty(), "nothing changed, nothing to do");
        assert_eq!(
            tags_of(&p[1].1),
            vec!["photoflow|class|day", "photoflow|open|other"]
        );
    }

    #[test]
    fn contradictory_look_tags_are_a_question_and_nothing_is_edited() {
        let text = format!(
            "darkroom\t0\n{}\n",
            photo(
                "/p/a.ARW",
                false,
                &["photoflow|look|agx", "photoflow|look|sigmoid"]
            )
        );
        let p = plan(
            &parse_handover(&text).unwrap(),
            &mut fake(|_: &Photo| Ok(Class::Day)),
        );
        assert!(!p[0].1.iter().any(|s| matches!(s, Step::Apply(_))));
        assert_eq!(
            tags_of(&p[0].1),
            vec![
                "photoflow|class|day",
                "photoflow|open|look: agx or sigmoid?"
            ]
        );
    }

    #[test]
    fn after_a_darktable_upgrade_nothing_is_edited() {
        assert_eq!(
            upgrade_block(Some("darktable 5.6.1"), "darktable 5.6.1"),
            None
        );
        assert!(upgrade_block(Some("darktable 5.6.1"), "darktable 5.8.0").is_some());
        assert!(upgrade_block(None, "darktable 5.6.1").is_some());
        let text = format!("darkroom\t0\n{}\n", photo("/p/a.ARW", false, &[]));
        let mut t = fake(|_: &Photo| Ok(Class::Day));
        t.blocked = Some("run the self-test after the darktable upgrade".into());
        let p = plan(&parse_handover(&text).unwrap(), &mut t);
        assert!(!p[0].1.iter().any(|s| matches!(s, Step::Apply(_))));
        assert_eq!(
            tags_of(&p[0].1),
            vec![
                "photoflow|class|day",
                "photoflow|left alone|run the self-test after the darktable upgrade"
            ]
        );
    }

    #[test]
    fn photos_in_quick_succession_are_proposed_as_a_panorama_set() {
        struct Timed;
        impl Tools for Timed {
            fn classify(&mut self, _: &Photo) -> Result<Class> {
                Ok(Class::Night)
            }
            fn sidecar_text(&mut self, _: &Photo) -> Result<String> {
                Ok(IMPORT.to_string())
            }
            fn day_edit(&mut self, _: &Photo, _: &str, _: Wanted) -> Result<day_edit::Solved> {
                unreachable!()
            }
            fn clipped_share(&mut self, _: &Photo) -> Result<f64> {
                unreachable!()
            }
            fn write_edit(&mut self, _: &Photo, _: &str) -> Result<PathBuf> {
                unreachable!()
            }
            fn edits_blocked(&mut self) -> Result<Option<String>> {
                Ok(None)
            }
            fn capture(&mut self, p: &Photo) -> Result<crate::exif::Capture> {
                let t = match p.raw.file_stem().unwrap().to_str().unwrap() {
                    "a" => 0.0,
                    "b" => 5.0,
                    "c" => 9.0,
                    _ => 100.0,
                };
                Ok(crate::exif::Capture {
                    time: t,
                    focal_mm: Some(12.0),
                })
            }
        }
        let text = format!(
            "darkroom\t0\n{}\n{}\n{}\n{}\n",
            photo("/p/a.ARW", false, &[]),
            photo("/p/b.ARW", false, &[]),
            photo("/p/c.ARW", false, &["photoflow|not a panorama"]),
            photo("/p/d.ARW", false, &[])
        );
        let p = plan(&parse_handover(&text).unwrap(), &mut Timed);
        let has = |i: usize, t: &str| p[i].1.contains(&Step::Tag(t.to_string()));
        assert!(has(0, "photoflow|panorama proposal|a") && has(1, "photoflow|panorama proposal|a"));
        assert!(has(0, "photoflow|open|panorama sets to confirm"));
        assert!(
            !p[2]
                .1
                .iter()
                .any(|s| matches!(s, Step::Tag(t) if t.contains("panorama")))
        );
        assert!(
            !p[3]
                .1
                .iter()
                .any(|s| matches!(s, Step::Tag(t) if t.contains("panorama")))
        );
        // earlier proposals are replaced, the rejected photo's tags aren't touched
        assert!(
            p[3].1
                .contains(&Step::Untag("photoflow|panorama proposal|".into()))
        );
        assert!(!p[2].1.iter().any(|s| matches!(s, Step::Untag(_))));
    }

    #[test]
    fn darkroom_open_leaves_everything_alone_unread() {
        let text = format!("darkroom\t1\n{}\n", photo("/nowhere/a.ARW", true, &[]));
        let p = plan(
            &parse_handover(&text).unwrap(),
            &mut fake(|_: &Photo| -> Result<Class> {
                panic!("classified a photo while the darkroom was open")
            }),
        );
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
        let p = plan(
            &parse_handover(&text).unwrap(),
            &mut fake(|ph: &Photo| {
                if ph.raw.ends_with("bad.ARW") {
                    bail!("no EXIF")
                }
                Ok(Class::Night)
            }),
        );
        assert_eq!(
            p[0].1[1],
            Step::Tag("photoflow|left alone|could not be processed".into())
        );
        assert_eq!(p[1].1[1], Step::Tag("photoflow|class|night".into()));
    }
}
