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
    (
        "us-09-day-auto-edit.feature",
        include_str!("../../features/us-09-day-auto-edit.feature"),
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
    /// Fingerprints of the photos' edits (their sidecars, as darktable writes them) and their tags.
    fn edits(&mut self, photos: &[PathBuf]) -> Result<Vec<(String, Vec<String>)>>;

    // US-9: on the photo under test (only against the darktable application)
    /// A freshly imported copy of a photo of the reference set.
    fn unedited_from(&mut self, file: &str, year: u16) -> Result<()>;
    fn current_photo(&self) -> Result<PathBuf>;
    fn tag(&mut self, tag: &str) -> Result<()>;
    /// Detaches every tag starting with `prefix`.
    fn untag(&mut self, prefix: &str) -> Result<()>;
    fn tags(&mut self) -> Result<Vec<String>>;
    /// The photo's sidecar, after darktable has written it.
    fn sidecar_text(&mut self) -> Result<String>;
    /// Renders the photo with its sidecar (darktable-cli), longest edge `size`.
    fn render(&mut self, size: u32) -> Result<crate::aurora::Image>;
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
    /// US-9: the scenario works on the driver's photo under test.
    on_driver: bool,
    import_sidecar: Option<String>,
    edit_before: Option<String>,
    upgrade: (Option<String>, Option<String>),
    blocked: Option<&'static str>,
    /// US-8: the folder's edits (fingerprint, sorted tags) before the second run.
    edits_before: Vec<(String, Vec<String>)>,
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
            on_driver: false,
            import_sidecar: None,
            edit_before: None,
            upgrade: (None, None),
            blocked: None,
            edits_before: Vec::new(),
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
/// Whether a scenario belongs to `selection` and, with `only`, mentions it (case-insensitive) in
/// its feature file's name, feature, rule or scenario name.
fn wanted(
    selection: Selection,
    only: Option<&str>,
    file: &str,
    feature: &gherkin::Feature,
    rule: Option<&gherkin::Rule>,
    scenario: &gherkin::Scenario,
) -> bool {
    let all: Vec<&[String]> = [
        Some(&feature.tags[..]),
        rule.map(|r| &r.tags[..]),
        Some(&scenario.tags[..]),
    ]
    .into_iter()
    .flatten()
    .collect();
    let has = |t: &str| all.iter().any(|tags| tagged(tags, t));
    let in_selection = match selection {
        Selection::Simulated => !has("wip") && !has("darktable-cli") && !has("darktable-app"),
        Selection::Darktable => {
            (has("darktable") || has("darktable-app")) && !has("wip") && !has("manual")
        }
        Selection::DarktableCli => has("darktable-cli") && !has("wip") && !has("manual"),
    };
    let matches = only.is_none_or(|o| {
        let o = o.to_lowercase();
        [
            file,
            &feature.name,
            rule.map_or("", |r| &r.name),
            &scenario.name,
        ]
        .iter()
        .any(|t| t.to_lowercase().contains(&o))
    });
    in_selection && matches
}

/// How many built-in scenarios `selection` and `only` pick.
pub fn count(selection: Selection, only: Option<&str>) -> usize {
    let mut n = 0;
    for (name, text) in FEATURES {
        let Ok(f) = gherkin::Feature::parse(text, gherkin::GherkinEnv::default()) else {
            continue;
        };
        n += f
            .scenarios
            .iter()
            .filter(|s| wanted(selection, only, name, &f, None, s))
            .count();
        for r in &f.rules {
            n += r
                .scenarios
                .iter()
                .filter(|s| wanted(selection, only, name, &f, Some(r), s))
                .count();
        }
    }
    n
}

/// Runs the scenarios in `features` that `selection` and `only` pick; with `driver` they run
/// against it, otherwise each scenario gets a fresh simulated darktable. Returns whether all passed.
pub fn run(
    features: &Path,
    selection: Selection,
    only: Option<&str>,
    driver: Option<Arc<Mutex<dyn Driver>>>,
) -> bool {
    *DRIVER.lock().unwrap() = driver;
    let only = only.map(str::to_string);
    let writer = futures::executor::block_on(
        ScenarioWorld::cucumber()
            // not the program's own command line (photoflow's arguments aren't cucumber's)
            .with_cli(cucumber::cli::Opts::<_, _, _, cucumber::cli::Empty>::default())
            .max_concurrent_scenarios(1)
            .fail_on_skipped()
            .filter_run(features, move |feature, rule, scenario| {
                let file = feature
                    .path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                wanted(selection, only.as_deref(), &file, feature, rule, scenario)
            }),
    );
    *DRIVER.lock().unwrap() = None;
    !writer.execution_has_failed() && writer.parsing_errors() == 0
}

/// Runs the `@darktable-cli` scenarios in `features` with `ctx`. Returns whether all passed.
pub fn run_cli(features: &Path, only: Option<&str>, ctx: CliContext) -> bool {
    *CLI.lock().unwrap() = Some(Arc::new(ctx));
    let ok = run(features, Selection::DarktableCli, only, None);
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
    if let (Some(accepted), Some(installed)) = w.upgrade.clone() {
        w.blocked = crate::process::upgrade_block(Some(&accepted), &installed);
        return;
    }
    if w.on_driver {
        w.edit_before = Some(w.with("read the sidecar", |d| d.sidecar_text()));
        start_tool(w);
        return;
    }
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
    if w.on_driver {
        let tags = w.with("read the tags", |d| d.tags());
        let tag = format!("{}{name}", crate::process::OPEN_TAG);
        assert!(tags.contains(&tag), "no open step {name:?}: {tags:?}");
        return;
    }
    match w.class {
        Some(Class::Ask(q)) => assert_eq!(q.open_step(), name),
        other => panic!("no open step for the photo: {other:?}"),
    }
}

// ---------------------------------------------------------------------------------------------
// US-9 — Day auto-edit (against the darktable application: real imports, renders)
// ---------------------------------------------------------------------------------------------

fn start_tool(w: &mut ScenarioWorld) {
    let photo = w.with("the photo under test", |d| d.current_photo());
    w.with("start the tool", |d| d.start_tool_on(&[photo]));
}

fn day_photo(w: &mut ScenarioWorld, from: Option<(&str, u16)>) {
    w.on_driver = true;
    match from {
        None => w.with("import a photo", |d| d.unedited()),
        Some((file, year)) => w.with("import a photo", |d| d.unedited_from(file, year)),
    }
    w.import_sidecar = Some(w.with("read the sidecar", |d| d.sidecar_text()));
}

/// The reference set's bright snow scene with clipped highlights.
const SNOW: (&str, u16) = ("_DSC6299.ARW", 2023);

#[given("an unedited day photo")]
fn unedited_day(w: &mut ScenarioWorld) {
    day_photo(w, None);
}

#[given("a day photo the tool has edited")]
fn day_edited(w: &mut ScenarioWorld) {
    day_photo(w, None);
    start_tool(w);
}

#[given(expr = "a day photo the tool has edited {}")]
fn day_edited_with(w: &mut ScenarioWorld, how: String) {
    day_photo(w, None);
    let tag = match how.as_str() {
        "in sigmoid" => "photoflow|look|sigmoid",
        "with dehaze" => "photoflow|dehaze|on",
        other => panic!("unknown: {other}"),
    };
    w.with("tag", |d| d.tag(tag));
    start_tool(w);
}

#[given("a day photo whose brightest parts are clipped")]
fn day_clipped(w: &mut ScenarioWorld) {
    // backlit, blown sky next to the sun (1.5 % of the frame clipped)
    day_photo(w, Some(("_DSC0831.ARW", 2018)));
}

#[given("an unedited day photo of a bright snow scene")]
fn day_snow(w: &mut ScenarioWorld) {
    day_photo(w, Some(SNOW));
}

#[given("a day photo of a bright snow scene the tool has edited")]
fn day_snow_edited(w: &mut ScenarioWorld) {
    day_photo(w, Some(SNOW));
    start_tool(w);
}

#[given(expr = "a day photo tagged {string} and {string}")]
fn day_tagged_two(w: &mut ScenarioWorld, a: String, b: String) {
    day_photo(w, None);
    w.with("tag", |d| d.tag(&a));
    w.with("tag", |d| d.tag(&b));
}

#[when(expr = "I tag it {string} and start the tool on it")]
fn tag_and_start(w: &mut ScenarioWorld, tag: String) {
    w.with("tag", |d| d.tag(&tag));
    start_tool(w);
}

#[when(expr = "I replace its tag with {string} and start the tool on it")]
fn retag_and_start(w: &mut ScenarioWorld, tag: String) {
    let group = &tag[..tag.rfind('|').expect("a tag like a|b|c") + 1];
    w.with("untag", |d| d.untag(group));
    w.with("tag", |d| d.tag(&tag));
    start_tool(w);
}

fn edit(w: &mut ScenarioWorld) -> crate::sidecar::History {
    let text = w.with("read the sidecar", |d| d.sidecar_text());
    crate::sidecar::parse(&text).unwrap_or_else(|e| panic!("the sidecar: {e:#}"))
}

fn has_step(h: &crate::sidecar::History, op: &str, name: &str) -> bool {
    h.steps
        .iter()
        .any(|s| s.operation == op && s.multi_name == name)
}

#[then(expr = "its edit has the look {string} and local contrast {string}")]
fn edit_has_look(w: &mut ScenarioWorld, look: String, lc: String) {
    let h = edit(w);
    let (want, not) = match look.as_str() {
        "AgX punchy" => (
            vec![("agx", "photoflow AgX punchy")],
            ("sigmoid", "photoflow sigmoid ACES"),
        ),
        "sigmoid ACES with vibrant colors" => (
            vec![
                ("sigmoid", "photoflow sigmoid ACES"),
                ("colorbalancergb", "photoflow vibrant colors"),
            ],
            ("agx", "photoflow AgX punchy"),
        ),
        other => panic!("unknown look {other}"),
    };
    for (op, name) in want {
        assert!(has_step(&h, op, name), "no {name}");
    }
    assert!(!has_step(&h, not.0, not.1), "still {}", not.1);
    assert_eq!(lc, "clarity");
    assert!(has_step(&h, "bilat", "photoflow clarity"), "no clarity");
}

#[then(expr = "its brightest {int} % sit at {float} in the edit's rendering")]
fn brightest_at(w: &mut ScenarioWorld, pct: u32, target: f64) {
    let img = w.with("render the edit", |d| {
        d.render(crate::day_edit::RENDER_SIZE)
    });
    let got = crate::day_edit::stats(&img, 1.0 - f64::from(pct) / 100.0).percentile;
    assert!(
        (got.log2() - target.log2()).abs() <= 0.1,
        "brightest {pct} % at {got:.3}, expected {target}"
    );
}

#[then("its edit contains no sharpening")]
fn no_sharpening(w: &mut ScenarioWorld) {
    let h = edit(w);
    assert!(
        !h.steps.iter().any(|s| s.operation == "sharpen"),
        "sharpening in the edit"
    );
}

#[then("every step darktable applied on import is still in its edit")]
fn import_kept(w: &mut ScenarioWorld) {
    let import =
        crate::sidecar::parse(w.import_sidecar.as_ref().expect("an imported photo")).unwrap();
    let h = edit(w);
    // the display transform, the exposure and sharpening are what the tool replaces
    let replaced = [
        "filmicrgb",
        "sigmoid",
        "agx",
        "basecurve",
        "filmic",
        "sharpen",
        "exposure",
    ];
    for s in import
        .steps
        .iter()
        .filter(|s| !replaced.contains(&s.operation.as_str()))
    {
        assert!(
            h.steps.contains(s),
            "{} ({}) from the import is gone",
            s.operation,
            s.multi_name
        );
    }
}

#[then(expr = "its edit also has diffuse or sharpen {string}")]
fn has_dehaze(w: &mut ScenarioWorld, preset: String) {
    assert_eq!(preset, "dehaze");
    assert!(
        has_step(&edit(w), "diffuse", "photoflow dehaze"),
        "no dehaze"
    );
}

#[then(expr = "its edit has no diffuse or sharpen {string}")]
fn no_dehaze(w: &mut ScenarioWorld, preset: String) {
    assert_eq!(preset, "dehaze");
    assert!(
        !has_step(&edit(w), "diffuse", "photoflow dehaze"),
        "dehaze still in the edit"
    );
}

#[then(expr = "the open step {string} doesn't list the photo")]
fn open_step_absent(w: &mut ScenarioWorld, name: String) {
    let tags = w.with("read the tags", |d| d.tags());
    let tag = format!("{}{name}", crate::process::OPEN_TAG);
    assert!(!tags.contains(&tag), "open step {name:?} is there");
}

#[then("the tool leaves its edit as it was")]
fn edit_unchanged(w: &mut ScenarioWorld) {
    let before = crate::sidecar::parse(w.edit_before.as_ref().expect("processed")).unwrap();
    assert_eq!(
        edit(w).fingerprint(),
        before.fingerprint(),
        "the edit changed"
    );
}

#[given(expr = "the self-test was last accepted with darktable {word}")]
fn accepted_with(w: &mut ScenarioWorld, version: String) {
    w.upgrade.0 = Some(format!("darktable {version}"));
}

#[given(expr = "darktable {word} is installed")]
fn installed(w: &mut ScenarioWorld, version: String) {
    w.upgrade.1 = Some(format!("darktable {version}"));
}

#[then(expr = "the tool edits no photo and reports {string}")]
fn edits_blocked(w: &mut ScenarioWorld, report: String) {
    assert_eq!(w.blocked, Some(report.as_str()));
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

#[given("the tool has processed a folder")]
fn processed_folder(w: &mut ScenarioWorld) {
    w.folder = w.with("import a folder", |d| d.import_new_folder(2));
    let folder = w.folder.clone();
    w.edits_before = w.with("read the edits", |d| d.edits(&folder));
}

#[when("I start the tool on the same folder again")]
fn again(w: &mut ScenarioWorld) {
    let folder = w.folder.clone();
    w.with("start the tool", |d| d.start_tool_on(&folder));
}

#[then("no photo's edit changes")]
fn nothing_changed(w: &mut ScenarioWorld) {
    let folder = w.folder.clone();
    let now = w.with("read the edits", |d| d.edits(&folder));
    assert!(
        w.edits_before.iter().all(|(fp, _)| !fp.is_empty()),
        "the folder wasn't edited: {:?}",
        w.edits_before
    );
    assert_eq!(now, w.edits_before, "edits or tags changed");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_narrows_the_selection() {
        assert_eq!(count(Selection::DarktableCli, Some("aurora")), 2);
        assert_eq!(count(Selection::Darktable, Some("aurora")), 0);
        assert_eq!(count(Selection::Darktable, Some("SECOND RUN")), 1);
        assert!(count(Selection::Darktable, Some("us-09")) > 5);
        assert_eq!(
            count(Selection::Darktable, Some("no scenario is called this")),
            0
        );
        assert!(count(Selection::Darktable, None) >= count(Selection::Darktable, Some("us-08")));
    }
}
