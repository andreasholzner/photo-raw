//! `photoflow selftest`: render the reference set and compare it with the reference renders.
//!
//! Layout below `<data dir>/selftest/`: `reference/` and `current/` hold one PNG per photo plus
//! `meta.toml`; `work/` holds the private darktable configuration and the source symlinks.

use crate::config::Config;
use crate::darktable::Darktable;
use crate::image_diff::{self, Diff};
use crate::locate::locate;
use crate::reference_set::{self, RefPhoto};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Serialize, Deserialize)]
struct Meta {
    darktable: String,
    rendered_at_unix: u64,
    size: u32,
}

struct Dirs {
    reference: PathBuf,
    current: PathBuf,
    work: PathBuf,
}

impl Dirs {
    fn new(cfg: &Config) -> Result<Self> {
        let base = cfg.data_dir()?.join("selftest");
        Ok(Self {
            reference: base.join("reference"),
            current: base.join("current"),
            work: base.join("work"),
        })
    }
}

/// The render of the tool's day or aurora edit of a reference photo (US-9, US-10).
fn tool_png_name(p: &RefPhoto) -> String {
    format!("tool-{}", png_name(p))
}

fn png_name(p: &RefPhoto) -> String {
    format!("{}-{}.png", p.year, p.file)
}

/// Renders the reference set and stores it as the reference.
pub fn init(cfg: &Config, force: bool) -> Result<bool> {
    let dirs = Dirs::new(cfg)?;
    if dirs.reference.exists() && !force {
        bail!(
            "reference renders already exist in {}\nUse `photoflow selftest run` to compare, or `init --force` to replace them.",
            dirs.reference.display()
        );
    }
    if !render_set(cfg, &dirs)? {
        bail!("some photos could not be rendered; no reference was stored");
    }
    promote(&dirs)?;
    println!("Reference renders stored in {}", dirs.reference.display());
    Ok(true)
}

/// The whole self-test: the renders of the reference set, then the darktable scenarios.
/// Returns whether all passed.
pub fn run(cfg: &Config) -> Result<bool> {
    let renders_ok = run_renders(cfg)?;
    println!();
    let scenarios_ok = scenarios(cfg, None)?;
    Ok(renders_ok && scenarios_ok)
}

/// Renders the reference set and compares it with the reference.
fn run_renders(cfg: &Config) -> Result<bool> {
    let dirs = Dirs::new(cfg)?;
    if !dirs.reference.exists() {
        bail!("no reference renders yet; run `photoflow selftest init` first");
    }
    let rendered_all = render_set(cfg, &dirs)?;
    let reference_meta = read_meta(&dirs.reference)?;
    let current_meta = read_meta(&dirs.current)?;

    let set = reference_set::load()?;
    // the owner's edits, then the tool's day edits where there is one to compare
    let mut checks: Vec<(String, String)> = set
        .iter()
        .map(|p| (png_name(p), p.reason.clone()))
        .collect();
    for p in &set {
        let name = tool_png_name(p);
        if dirs.reference.join(&name).is_file() || dirs.current.join(&name).is_file() {
            checks.push((name, "the tool's edit (US-9, US-10, US-21)".into()));
        }
    }
    let width = checks.iter().map(|(n, _)| n.len()).max().unwrap_or(10);
    println!();
    println!(
        "{:<width$}  {:>6}  {:>4}  {:>4}  result",
        "photo", "mean", "p99", "max"
    );
    let mut failed = 0;
    for (name, reason) in &checks {
        let (diff, verdict) = compare(cfg, &dirs.reference.join(name), &dirs.current.join(name));
        let numbers = match diff {
            Some(d) => format!("{:>6.2}  {:>4}  {:>4}", d.mean, d.p99, d.max),
            None => format!("{:>6}  {:>4}  {:>4}", "-", "-", "-"),
        };
        if verdict == "ok" {
            println!("{name:<width$}  {numbers}  ok");
        } else {
            failed += 1;
            println!("{name:<width$}  {numbers}  {verdict} ({reason})");
        }
    }
    println!();
    if reference_meta.darktable != current_meta.darktable {
        println!(
            "darktable changed: reference {} → now {}",
            reference_meta.darktable, current_meta.darktable
        );
    }
    if failed == 0 && rendered_all {
        println!(
            "All {} renders match the reference ({}).",
            checks.len(),
            current_meta.darktable
        );
        Ok(true)
    } else {
        println!(
            "{failed} of {} renders differ from the reference.",
            checks.len()
        );
        println!(
            "Compare the PNGs in {} and {}.",
            dirs.reference.display(),
            dirs.current.display()
        );
        println!(
            "If the changes are intended, make them the new reference: `photoflow selftest accept`."
        );
        Ok(false)
    }
}

/// Runs the executable scenarios that depend on darktable: the `@darktable-cli` ones with
/// darktable-cli on photos of the reference set, then the `@darktable` ones against the darktable
/// application (US-20); lists the ones that need a manual check. Returns whether all passed.
pub fn scenarios(cfg: &Config, only: Option<&str>) -> Result<bool> {
    use crate::scenarios::{Selection, count};
    let base = cfg.data_dir()?.join("selftest").join("scenarios");
    let features = crate::scenarios::write_features(&base.join("features"))?;
    let (n_cli, n_app) = (
        count(Selection::DarktableCli, only),
        count(Selection::Darktable, only),
    );
    if let Some(o) = only {
        println!("--only {o:?}: {n_cli} darktable-cli and {n_app} darktable scenarios");
        if n_cli + n_app == 0 {
            bail!("no scenario matches {o:?}");
        }
    }

    let cli_work = base.join("darktable-cli");
    let cli_ok = if n_cli > 0 {
        println!("Running the darktable-cli scenarios …");
        let dt = Darktable::prepare(&cfg.darktable.cli, &cfg.darktable_config_dir()?, &cli_work)?;
        let (cli, dt_config) = (cfg.darktable.cli.clone(), cfg.darktable_config_dir()?);
        let (hz_work, models) = (base.join("horizon"), cfg.data_dir()?.join("models"));
        let ctx = crate::scenarios::CliContext {
            check: crate::classify::AuroraCheck::new(dt, &cli_work.join("renders"))?,
            photo_root: cfg.photo_root.clone(),
            horizon: std::sync::OnceLock::new(),
            make_horizon: Box::new(move || {
                let dt = Darktable::prepare(&cli, &dt_config, &hz_work.join("darktable-cli"))?;
                crate::horizon::Finder::new(dt, &hz_work.join("renders"), &models)
            }),
        };
        crate::scenarios::run_cli(&features, only, ctx)
    } else {
        true
    };

    let app_ok = if n_app > 0 {
        println!(
            "\nStarting darktable on a virtual screen for the scenarios (private configuration and library in {}) …",
            base.display()
        );
        let app = crate::scenarios::app::App::start(cfg, &base.join("darktable"))?;
        let app = std::sync::Arc::new(std::sync::Mutex::new(app));
        let ok = crate::scenarios::run(
            &features,
            crate::scenarios::Selection::Darktable,
            only,
            Some(app.clone()),
        );
        app.lock().unwrap_or_else(|e| e.into_inner()).stop();
        ok
    } else {
        true
    };
    let manual = crate::scenarios::manual_checks();
    if !manual.is_empty() {
        println!("\nManual checks (can't be run unattended; do them after a darktable upgrade):");
        for m in manual {
            println!("  - {m}");
        }
    }
    println!();
    if cli_ok && app_ok {
        println!("All darktable scenarios passed.");
    } else {
        if !cli_ok {
            println!(
                "Some darktable-cli scenarios failed; their renders: {}",
                cli_work.join("renders").display()
            );
        }
        if !app_ok {
            println!(
                "Some darktable scenarios failed; darktable's log: {}",
                base.join("darktable/darktable.log").display()
            );
        }
    }
    Ok(cli_ok && app_ok)
}

/// The darktable the self-test's reference was last accepted with ("darktable 5.6.1"), if any.
pub fn accepted_darktable(cfg: &Config) -> Result<Option<String>> {
    let dirs = Dirs::new(cfg)?;
    if !dirs.reference.join("meta.toml").is_file() {
        return Ok(None);
    }
    Ok(Some(read_meta(&dirs.reference)?.darktable))
}

/// Makes the last run's renders the new reference.
pub fn accept(cfg: &Config) -> Result<bool> {
    let dirs = Dirs::new(cfg)?;
    let set = reference_set::load()?;
    let missing: Vec<String> = set
        .iter()
        .map(png_name)
        .filter(|n| !dirs.current.join(n).is_file())
        .collect();
    if !dirs.current.join("meta.toml").is_file() || !missing.is_empty() {
        bail!(
            "the last run is incomplete (missing: {}); run `photoflow selftest run` first",
            missing.join(", ")
        );
    }
    promote(&dirs)?;
    println!(
        "The last run is now the reference ({}).",
        dirs.reference.display()
    );
    Ok(true)
}

fn compare(cfg: &Config, reference: &Path, current: &Path) -> (Option<Diff>, &'static str) {
    if !current.is_file() {
        return (None, "NOT RENDERED");
    }
    if !reference.is_file() {
        return (None, "NO REFERENCE");
    }
    let (Ok(a), Ok(b)) = (
        image_diff::load_png(reference),
        image_diff::load_png(current),
    ) else {
        return (None, "UNREADABLE");
    };
    match image_diff::diff(&a, &b) {
        None => (None, "SIZE CHANGED"),
        Some(d)
            if d.mean > cfg.selftest.max_mean_diff
                || f64::from(d.p99) > cfg.selftest.max_p99_diff =>
        {
            (Some(d), "DIFFERENT")
        }
        Some(d) => (Some(d), "ok"),
    }
}

/// Renders every photo of the set into `current/`. Returns whether all were rendered.
fn render_set(cfg: &Config, dirs: &Dirs) -> Result<bool> {
    let set = reference_set::load()?;
    let mut jobs = Vec::new();
    let mut problems = Vec::new();
    for p in &set {
        match locate(&cfg.photo_root, p.year, &p.file) {
            Ok(source) => {
                let xmp = PathBuf::from(format!("{}.xmp", source.display()));
                if xmp.is_file() {
                    jobs.push((p.clone(), source, xmp));
                } else {
                    problems.push(format!("{} ({}): no sidecar next to it", p.file, p.year));
                }
            }
            Err(e) => problems.push(e.to_string()),
        }
    }
    if !problems.is_empty() {
        bail!(
            "cannot find all photos of the reference set:\n  {}",
            problems.join("\n  ")
        );
    }

    let dt = Darktable::prepare(&cfg.darktable.cli, &cfg.darktable_config_dir()?, &dirs.work)?;
    let version = dt.version()?;
    if dirs.current.exists() {
        std::fs::remove_dir_all(&dirs.current)?;
    }
    std::fs::create_dir_all(&dirs.current)?;
    println!("Rendering {} photos with {version} …", jobs.len());

    let sources: Vec<(RefPhoto, PathBuf)> = jobs
        .iter()
        .map(|(p, s, _)| (p.clone(), s.clone()))
        .collect();
    let queue = Mutex::new(jobs.into_iter());
    let done = AtomicUsize::new(0);
    let failures = Mutex::new(Vec::new());
    let total = set.len();
    std::thread::scope(|s| {
        for _ in 0..cfg.selftest.jobs {
            s.spawn(|| {
                loop {
                    let Some((p, source, xmp)) = queue.lock().unwrap().next() else {
                        break;
                    };
                    let out = dirs.current.join(png_name(&p));
                    let result = dt.render(&source, &xmp, &out, cfg.selftest.size);
                    let n = done.fetch_add(1, Ordering::SeqCst) + 1;
                    match result {
                        Ok(()) => println!("  [{n:>2}/{total}] {}", p.file),
                        Err(e) => {
                            println!("  [{n:>2}/{total}] {} FAILED", p.file);
                            failures.lock().unwrap().push(format!("{e:#}"));
                        }
                    }
                }
            });
        }
    });
    // US-9: the tool's day edit of every day photo, on darktable's defaults (the photos' import
    // histories are gone), with the exposure solved as in a real run
    let day: Vec<(RefPhoto, PathBuf)> = sources
        .iter()
        .filter(|(_, s)| {
            crate::exif::read(s)
                .map(|e| crate::scene::classify(&e) == crate::scene::Scene::Day)
                .unwrap_or(false)
        })
        .cloned()
        .collect();
    println!("Rendering the tool's day edit of {} photos …", day.len());
    let queue = Mutex::new(day.into_iter());
    let done = AtomicUsize::new(0);
    let total = queue.lock().unwrap().len();
    std::thread::scope(|s| {
        for _ in 0..cfg.selftest.jobs {
            s.spawn(|| {
                loop {
                    let Some((p, source)) = queue.lock().unwrap().next() else {
                        break;
                    };
                    let out = dirs.current.join(tool_png_name(&p));
                    let result =
                        tool_day_edit(&dt, &source, &dirs.work.join("tool"), &png_name(&p))
                            .and_then(|xmp| dt.render(&source, &xmp, &out, cfg.selftest.size));
                    let n = done.fetch_add(1, Ordering::SeqCst) + 1;
                    match result {
                        Ok(()) => println!("  [{n:>2}/{total}] {} (tool)", p.file),
                        Err(e) => {
                            println!("  [{n:>2}/{total}] {} (tool) FAILED", p.file);
                            failures.lock().unwrap().push(format!("{e:#}"));
                        }
                    }
                }
            });
        }
    });
    // US-10: the tool's aurora edit of every aurora photo, likewise on darktable's defaults
    let aurora = aurora_photos(cfg, dirs, &sources);
    if !aurora.is_empty() {
        println!(
            "Rendering the tool's aurora edit of {} photos …",
            aurora.len()
        );
        let finder = crate::horizon::Finder::new(
            Darktable::prepare(
                &cfg.darktable.cli,
                &cfg.darktable_config_dir()?,
                &dirs.work.join("horizon"),
            )?,
            &dirs.work.join("horizon-renders"),
            &cfg.data_dir()?.join("models"),
        )?;
        let queue = Mutex::new(aurora.into_iter());
        let done = AtomicUsize::new(0);
        let total = queue.lock().unwrap().len();
        std::thread::scope(|s| {
            for _ in 0..cfg.selftest.jobs {
                s.spawn(|| {
                    loop {
                        let Some((p, source, ratio)) = queue.lock().unwrap().next() else {
                            break;
                        };
                        let out = dirs.current.join(tool_png_name(&p));
                        let result = tool_aurora_edit(
                            &dt,
                            &finder,
                            &source,
                            ratio,
                            &dirs.work.join("tool"),
                            &png_name(&p),
                        )
                        .and_then(|xmp| dt.render(&source, &xmp, &out, cfg.selftest.size));
                        let n = done.fetch_add(1, Ordering::SeqCst) + 1;
                        match result {
                            Ok(()) => println!("  [{n:>2}/{total}] {} (tool)", p.file),
                            Err(e) => {
                                println!("  [{n:>2}/{total}] {} (tool) FAILED", p.file);
                                failures.lock().unwrap().push(format!("{e:#}"));
                            }
                        }
                    }
                });
            }
        });
    }
    let failures = failures.into_inner().unwrap();
    for f in &failures {
        eprintln!("  {f}");
    }
    let meta = Meta {
        darktable: version,
        rendered_at_unix: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs(),
        size: cfg.selftest.size,
    };
    std::fs::write(dirs.current.join("meta.toml"), toml::to_string(&meta)?)?;
    Ok(failures.is_empty())
}

/// The tool's day edit of `source` on darktable's defaults; returns the edit's sidecar.
fn tool_day_edit(dt: &Darktable, source: &Path, work: &Path, name: &str) -> Result<PathBuf> {
    use crate::day_edit::{self, Look, Wanted};
    std::fs::create_dir_all(work)?;
    let xmp = work.join(format!("{name}.xmp"));
    let probe = work.join(format!("{name}.pfm"));
    let wanted = Wanted {
        look: Look::Agx,
        dehaze: false,
    };
    let solved = day_edit::solve(day_edit::EMPTY_HISTORY, wanted, |text| {
        std::fs::write(&xmp, text)?;
        dt.render(source, &xmp, &probe, day_edit::RENDER_SIZE)?;
        crate::aurora::load_pfm(&probe)
    })?;
    std::fs::write(&xmp, &solved.xmp)?;
    Ok(xmp)
}

/// The reference set's aurora photos (US-7's check) with their scene ratio (US-10), and its
/// night photos without aurora and without a bright sky (ratio `None`, US-21).
fn aurora_photos(
    cfg: &Config,
    dirs: &Dirs,
    sources: &[(RefPhoto, PathBuf)],
) -> Vec<(RefPhoto, PathBuf, Option<f64>)> {
    let check = Darktable::prepare(
        &cfg.darktable.cli,
        &cfg.darktable_config_dir().unwrap_or_default(),
        &dirs.work.join("aurora-check"),
    )
    .and_then(|dt| crate::classify::AuroraCheck::new(dt, &dirs.work.join("aurora-renders")));
    let Ok(check) = check else {
        return Vec::new();
    };
    sources
        .iter()
        .filter(|(_, s)| {
            crate::exif::read(s)
                .map(|e| crate::scene::classify(&e) == crate::scene::Scene::Night)
                .unwrap_or(false)
        })
        .filter_map(|(p, s)| {
            let img = check.render(s).ok()?;
            if crate::aurora::has_aurora(&img) {
                Some((
                    p.clone(),
                    s.clone(),
                    Some(crate::aurora_edit::scene_ratio(&img)),
                ))
            } else if crate::night_edit::bright_sky(&img) {
                None
            } else {
                Some((p.clone(), s.clone(), None))
            }
        })
        .collect()
}

/// The tool's aurora edit (neutral blue; `ratio` is the scene ratio) or, without a ratio, its
/// night edit of `source` on darktable's defaults; returns its sidecar.
fn tool_aurora_edit(
    dt: &Darktable,
    finder: &crate::horizon::Finder,
    source: &Path,
    ratio: Option<f64>,
    work: &Path,
    name: &str,
) -> Result<PathBuf> {
    use crate::aurora_edit;
    std::fs::create_dir_all(work)?;
    let xmp = work.join(format!("{name}.xmp"));
    let probe = work.join(format!("{name}.pfm"));
    let (line, _) = finder.find(source)?;
    let Some(ratio) = ratio else {
        let solved =
            crate::night_edit::solve(crate::day_edit::EMPTY_HISTORY, &line, &mut |text, size| {
                std::fs::write(&xmp, text)?;
                dt.render(source, &xmp, &probe, size)?;
                crate::aurora::load_pfm(&probe)
            })?;
        std::fs::write(&xmp, &solved.xmp)?;
        return Ok(xmp);
    };
    let solved = aurora_edit::solve(
        crate::day_edit::EMPTY_HISTORY,
        aurora_edit::SkyColour::NeutralBlue,
        aurora_edit::sky_target(ratio),
        Some(&line),
        &mut |text| {
            std::fs::write(&xmp, text)?;
            dt.render(source, &xmp, &probe, aurora_edit::RENDER_SIZE)?;
            crate::aurora::load_pfm(&probe)
        },
    )?;
    std::fs::write(&xmp, &solved.xmp)?;
    Ok(xmp)
}

fn promote(dirs: &Dirs) -> Result<()> {
    let old = dirs.reference.with_extension("old");
    if old.exists() {
        std::fs::remove_dir_all(&old)?;
    }
    if dirs.reference.exists() {
        std::fs::rename(&dirs.reference, &old)?;
    }
    std::fs::rename(&dirs.current, &dirs.reference)?;
    if old.exists() {
        std::fs::remove_dir_all(&old)?;
    }
    Ok(())
}

fn read_meta(dir: &Path) -> Result<Meta> {
    let p = dir.join("meta.toml");
    let text =
        std::fs::read_to_string(&p).with_context(|| format!("cannot read {}", p.display()))?;
    Ok(toml::from_str(&text)?)
}
