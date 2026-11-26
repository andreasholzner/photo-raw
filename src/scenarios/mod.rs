//! The executable scenarios in `features/` (ADR-0006): step code and runner.
//!
//! The same steps run against two drivers: a simulated darktable (the normal tests) and the real
//! darktable application (the self-test, US-20), which decides what the `@darktable` scenarios
//! are checked against. The `@darktable-cli` scenarios render real photos of the reference set
//! with darktable-cli and run only in the self-test. The feature files are built into the binary, so the self-test needs no
//! source checkout.

pub mod app;
pub mod sim;

use crate::classify::{self, AuroraCheck, Class};
use crate::edit_state::EditState;
use crate::exif::Exposure;
use crate::locate::locate;
use crate::scene::{self, Scene};
use anyhow::Result;
use cucumber::gherkin;
use cucumber::writer::Stats as _;
use cucumber::{World, given, then, when};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// The feature files, by file name.
pub const FEATURES: &[(&str, &str)] = &[
    (
        "us-07-classify-day-night-and-aurora.feature",
        include_str!("../../features/us-07-classify-day-night-and-aurora.feature"),
    ),
    (
        "us-08-only-touch-what-was-handed-over.feature",
        include_str!("../../features/us-08-only-touch-what-was-handed-over.feature"),
    ),
];

/// A change the owner makes in the darkroom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Exposure,
    ModuleOn,
    ModuleOff,
    Mask,
}

/// What the US-8 steps do with darktable; one photo under test per scenario.
pub trait Driver: Send {
    /// A freshly imported photo.
    fn unedited(&mut self) -> Result<()>;
    /// The tool hands its edit to the photo and tags it with the fingerprint.
    fn tool_edit(&mut self) -> Result<()>;
    /// Opens the photo in the darkroom (if it isn't yet) and makes the change there.
    fn change_in_darkroom(&mut self, change: Change) -> Result<()>;
    fn undo(&mut self) -> Result<()>;
    fn leave_darkroom(&mut self) -> Result<()>;
    fn apply_style_in_lighttable(&mut self) -> Result<()>;
    fn compress_history(&mut self) -> Result<()>;
    fn open_other_photo_in_darkroom(&mut self) -> Result<()>;
    /// What the tool decides about the photo, from what darktable reports.
    fn edit_state(&mut self) -> Result<EditState>;

    /// Imports a new folder of `n` photos; the tool processes newly imported photos.
    fn import_new_folder(&mut self, n: usize) -> Result<Vec<PathBuf>>;
    /// A folder of `n` photos imported earlier, without the tool.
    fn old_folder(&mut self, n: usize) -> Result<Vec<PathBuf>>;
    /// The owner starts the tool on these photos.
    fn start_tool_on(&mut self, photos: &[PathBuf]) -> Result<()>;
    /// Every photo in the library the tool has processed (it carries the tool's class, open-step
    /// or report tags).
    fn processed(&mut self) -> Result<Vec<PathBuf>>;
}

/// Which driver the next scenarios use.
static DRIVER: Mutex<Option<Arc<Mutex<dyn Driver>>>> = Mutex::new(None);

/// What the `@darktable-cli` scenarios render with: darktable-cli and the owner's photos.
pub struct CliContext {
    pub check: AuroraCheck,
    pub photo_root: PathBuf,
}

static CLI: Mutex<Option<Arc<CliContext>>> = Mutex::new(None);

#[derive(World)]
#[world(init = Self::new)]
pub struct ScenarioWorld {
    exposure: Option<Exposure>,
    /// A real photo under test (`@darktable-cli`).
    photo: Option<PathBuf>,
    class: Option<Class>,
    /// Photos of the scenario's folder, and those processed before the scenario's action.
    folder: Vec<PathBuf>,
    processed_before: Vec<PathBuf>,
    driver: Arc<Mutex<dyn Driver>>,
}

impl std::fmt::Debug for ScenarioWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScenarioWorld")
            .field("exposure", &self.exposure)
            .field("photo", &self.photo)
            .field("class", &self.class)
            .finish()
    }
}

impl ScenarioWorld {
    fn new() -> Self {
        let driver = DRIVER.lock().unwrap().clone();
        Self {
            exposure: None,
            photo: None,
            class: None,
            folder: Vec::new(),
            processed_before: Vec::new(),
            // without a configured driver every scenario gets a fresh simulated darktable
            driver: driver.unwrap_or_else(|| Arc::new(Mutex::new(sim::Sim::default()))),
        }
    }

    fn with<T>(&self, what: &str, f: impl FnOnce(&mut dyn Driver) -> Result<T>) -> T {
        // a failed step must not fail the following scenarios
        let mut d = self.driver.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut *d).unwrap_or_else(|e| panic!("{what}: {e:#}"))
    }
}

/// Which scenarios to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    /// Everything that is built (not `@wip`), against the simulated darktable.
    Simulated,
    /// The `@darktable` scenarios that can run unattended, against the given driver.
    Darktable,
    /// The `@darktable-cli` scenarios, with the context set by [`run_cli`].
    DarktableCli,
}

fn tagged(tags: &[String], t: &str) -> bool {
    tags.iter().any(|x| x == t)
}

/// Writes the built-in feature files to `dir`.
pub fn write_features(dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    for (name, text) in FEATURES {
        std::fs::write(dir.join(name), text)?;
    }
    Ok(dir.to_path_buf())
}

/// Runs the scenarios in `features`; with `driver` they run against it, otherwise each scenario
/// gets a fresh simulated darktable. Returns whether all passed.
pub fn run(features: &Path, selection: Selection, driver: Option<Arc<Mutex<dyn Driver>>>) -> bool {
    *DRIVER.lock().unwrap() = driver;
    let writer = futures::executor::block_on(
        ScenarioWorld::cucumber()
            // not the program's own command line (photoflow's arguments aren't cucumber's)
            .with_cli(cucumber::cli::Opts::<_, _, _, cucumber::cli::Empty>::default())
            .max_concurrent_scenarios(1)
            .fail_on_skipped()
            .filter_run(features, move |feature, rule, scenario| {
                let all: Vec<&[String]> = [
                    Some(&feature.tags[..]),
                    rule.map(|r| &r.tags[..]),
                    Some(&scenario.tags[..]),
                ]
                .into_iter()
                .flatten()
                .collect();
                let has = |t: &str| all.iter().any(|tags| tagged(tags, t));
                match selection {
                    Selection::Simulated => !has("wip") && !has("darktable-cli"),
                    Selection::Darktable => has("darktable") && !has("wip") && !has("manual"),
                    Selection::DarktableCli => {
                        has("darktable-cli") && !has("wip") && !has("manual")
                    }
                }
            }),
    );
    *DRIVER.lock().unwrap() = None;
    !writer.execution_has_failed() && writer.parsing_errors() == 0
}

/// Runs the `@darktable-cli` scenarios in `features` with `ctx`. Returns whether all passed.
pub fn run_cli(features: &Path, ctx: CliContext) -> bool {
    *CLI.lock().unwrap() = Some(Arc::new(ctx));
    let ok = run(features, Selection::DarktableCli, None);
    *CLI.lock().unwrap() = None;
    ok
}

/// Scenarios (and example rows) tagged `@manual` in the built-in features, as "feature: scenario".
pub fn manual_checks() -> Vec<String> {
    let mut out = Vec::new();
    for (name, text) in FEATURES {
        let Ok(f) = gherkin::Feature::parse(text, gherkin::GherkinEnv::default()) else {
            out.push(format!("{name}: cannot be read"));
            continue;
        };
        let scenarios = f
            .scenarios
            .iter()
            .chain(f.rules.iter().flat_map(|r| r.scenarios.iter()));
        for sc in scenarios {
            if tagged(&sc.tags, "wip") {
                continue;
            }
            if tagged(&sc.tags, "manual") {
                out.push(format!("{}: {}", f.name, sc.name));
            }
            for ex in sc.examples.iter().filter(|e| tagged(&e.tags, "manual")) {
                let rows = ex.table.as_ref().map(|t| {
                    t.rows
                        .iter()
                        .skip(1)
                        .map(|r| r.join(", "))
                        .collect::<Vec<_>>()
                        .join("; ")
                });
                out.push(format!(
                    "{}: {} — {}",
                    f.name,
                    sc.name,
                    rows.unwrap_or_default()
                ));
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// US-7 — Classify day, night and aurora
// ---------------------------------------------------------------------------------------------

/// "1/250" or "4" (seconds).
fn shutter(s: &str) -> f64 {
    match s.split_once('/') {
        Some((a, b)) => a.parse::<f64>().unwrap() / b.parse::<f64>().unwrap(),
        None => s.parse().unwrap(),
    }
}

/// "f/8", or "manual" for a lens that doesn't report its aperture.
fn aperture(s: &str) -> Option<f64> {
    if s == "manual" {
        return None;
    }
    Some(
        s.strip_prefix("f/")
            .expect("aperture like f/8 or manual")
            .parse()
            .unwrap(),
    )
}

#[given(expr = "a photo taken at {word} s, aperture {word} and ISO {int}")]
fn photo_taken(w: &mut ScenarioWorld, t: String, f: String, iso: u32) {
    w.exposure = Some(Exposure {
        time: shutter(&t),
        aperture: aperture(&f),
        iso: f64::from(iso),
    });
}

#[then(expr = "its scene brightness is {float}")]
fn brightness_is(w: &mut ScenarioWorld, ev: f64) {
    let got = scene::ev100(w.exposure.as_ref().unwrap());
    assert!(
        (got - ev).abs() < 0.05,
        "scene brightness {got:.2}, expected {ev}"
    );
}

#[then(expr = "it is a {word} photo")]
fn class_is(w: &mut ScenarioWorld, class: String) {
    let expected = match class.as_str() {
        "day" => Scene::Day,
        "night" => Scene::Night,
        "unclear" => Scene::Unclear,
        other => panic!("unknown class {other}"),
    };
    assert_eq!(scene::classify(w.exposure.as_ref().unwrap()), expected);
}

#[given(expr = "a photo with scene brightness {float}")]
fn photo_with_brightness(w: &mut ScenarioWorld, ev: f64) {
    // 1 s at ISO 100: EV100 = log2(f²)
    w.exposure = Some(Exposure {
        time: 1.0,
        aperture: Some(2f64.powf(ev / 2.0)),
        iso: 100.0,
    });
}

/// A night photo of the reference set (data/reference-set.toml), found below the photo root.
fn reference_night_photo(w: &mut ScenarioWorld, file: &str, year: u16) {
    let ctx = CLI
        .lock()
        .unwrap()
        .clone()
        .expect("needs darktable-cli (self-test only)");
    let raw = locate(&ctx.photo_root, year, file).unwrap_or_else(|e| panic!("{e:#}"));
    w.exposure = Some(crate::exif::read(&raw).unwrap_or_else(|e| panic!("{e:#}")));
    w.photo = Some(raw);
}

#[given("a night photo with a green band in the sky")]
fn night_with_aurora(w: &mut ScenarioWorld) {
    reference_night_photo(w, "_DSC3735.ARW", 2019); // bright green aurora
}

#[given("a night photo with a dark, colourless sky")]
fn night_without_aurora(w: &mut ScenarioWorld) {
    reference_night_photo(w, "_DSC0906.ARW", 2018); // stars and a summit cross
}

#[when("the photos are processed")]
fn processed(w: &mut ScenarioWorld) {
    let exposure = w.exposure.expect("a photo");
    let photo = w.photo.clone();
    let class = classify::classify(&exposure, || {
        let ctx = CLI.lock().unwrap().clone();
        match (ctx, photo) {
            (Some(ctx), Some(raw)) => ctx.check.render(&raw),
            _ => anyhow::bail!("a night photo needs a real photo and darktable-cli"),
        }
    });
    w.class = Some(class.unwrap_or_else(|e| panic!("processing: {e:#}")));
}

#[then("it is an aurora photo")]
fn is_aurora(w: &mut ScenarioWorld) {
    assert_eq!(w.class, Some(Class::Aurora));
}

#[then("it is a night photo without aurora")]
fn is_night_without_aurora(w: &mut ScenarioWorld) {
    assert_eq!(w.class, Some(Class::Night));
}

#[then(expr = "the open step {string} lists the photo")]
fn open_step_lists(w: &mut ScenarioWorld, name: String) {
    match w.class {
        Some(Class::Ask(q)) => assert_eq!(q.open_step(), name),
        other => panic!("no open step for the photo: {other:?}"),
    }
}

// ---------------------------------------------------------------------------------------------
// US-8 — Only touch what was handed over
// ---------------------------------------------------------------------------------------------

#[given("a photo that darktable reports as unedited")]
fn unedited(w: &mut ScenarioWorld) {
    w.with("import a photo", |d| d.unedited());
}

#[given("a photo I edited by hand")]
fn edited_by_hand(w: &mut ScenarioWorld) {
    w.with("edit a photo by hand", |d| {
        d.unedited()?;
        d.change_in_darkroom(Change::Exposure)?;
        d.leave_darkroom()
    });
}

#[given("a photo the tool has edited")]
fn edited_by_tool(w: &mut ScenarioWorld) {
    w.with("hand over the tool's edit", |d| {
        d.unedited()?;
        d.tool_edit()
    });
}

#[given("I have another photo open in the darkroom")]
fn other_photo_in_darkroom(w: &mut ScenarioWorld) {
    w.with("open another photo in the darkroom", |d| {
        d.open_other_photo_in_darkroom()
    });
}

#[when(expr = "I {} in the darkroom")]
fn change_in_darkroom(w: &mut ScenarioWorld, what: String) {
    let change = match what.as_str() {
        "change the exposure" => Change::Exposure,
        "switch a module on" => Change::ModuleOn,
        "switch a module off" => Change::ModuleOff,
        "draw a mask on a module" => Change::Mask,
        other => panic!("unknown change: {other}"),
    };
    w.with(&what, |d| d.change_in_darkroom(change));
}

#[when("I undo that change")]
fn undo(w: &mut ScenarioWorld) {
    w.with("undo", |d| d.undo());
}

#[when("I leave the darkroom")]
fn leave(w: &mut ScenarioWorld) {
    w.with("leave the darkroom", |d| d.leave_darkroom());
}

#[when("I apply a style to it in the lighttable")]
fn apply_style(w: &mut ScenarioWorld) {
    w.with("apply a style", |d| d.apply_style_in_lighttable());
}

#[when("I compress its history stack")]
fn compress(w: &mut ScenarioWorld) {
    w.with("compress the history", |d| d.compress_history());
}

#[given("I import a folder of new photos")]
fn import_new(w: &mut ScenarioWorld) {
    w.processed_before = w.with("list processed photos", |d| d.processed());
    w.folder = w.with("import a folder", |d| d.import_new_folder(3));
}

#[given("a folder with photos imported last year")]
fn old_folder(w: &mut ScenarioWorld) {
    w.folder = w.with("import a folder without the tool", |d| d.old_folder(4));
    w.processed_before = w.with("list processed photos", |d| d.processed());
}

#[when("I select two of them and start the tool")]
fn start_on_two(w: &mut ScenarioWorld) {
    let two = w.folder[1..3].to_vec();
    w.with("start the tool", |d| d.start_tool_on(&two));
    w.folder = two;
}

/// The photos processed since the scenario's action, compared with the expected ones.
fn assert_processed_exactly(w: &mut ScenarioWorld) {
    let now = w.with("list processed photos", |d| d.processed());
    let mut new: Vec<&PathBuf> = now
        .iter()
        .filter(|p| !w.processed_before.contains(p))
        .collect();
    let mut expected: Vec<&PathBuf> = w.folder.iter().collect();
    new.sort();
    expected.sort();
    assert_eq!(new, expected, "processed photos");
}

#[then("the tool processes exactly these photos")]
fn processes_these(w: &mut ScenarioWorld) {
    assert_processed_exactly(w);
}

#[then("the tool processes only the two selected photos")]
fn processes_two(w: &mut ScenarioWorld) {
    assert_processed_exactly(w);
}

#[then("the tool may edit it")]
fn may_edit(w: &mut ScenarioWorld) {
    let s = w.with("ask the tool", |d| d.edit_state());
    assert!(s.may_edit(), "the tool may not edit it: {s:?}");
}

#[then(expr = "the tool leaves it alone and reports {string}")]
fn leaves_alone(w: &mut ScenarioWorld, report: String) {
    let s = w.with("ask the tool", |d| d.edit_state());
    assert!(!s.may_edit(), "the tool would edit it: {s:?}");
    assert_eq!(s.report(), Some(report.as_str()));
}
