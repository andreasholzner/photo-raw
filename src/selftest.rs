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
    let scenarios_ok = scenarios(cfg)?;
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
    let width = set.iter().map(|p| png_name(p).len()).max().unwrap_or(10);
    println!();
    println!(
        "{:<width$}  {:>6}  {:>4}  {:>4}  result",
        "photo", "mean", "p99", "max"
    );
    let mut failed = 0;
    for p in &set {
        let name = png_name(p);
        let (diff, verdict) = compare(cfg, &dirs.reference.join(&name), &dirs.current.join(&name));
        let numbers = match diff {
            Some(d) => format!("{:>6.2}  {:>4}  {:>4}", d.mean, d.p99, d.max),
            None => format!("{:>6}  {:>4}  {:>4}", "-", "-", "-"),
        };
        if verdict == "ok" {
            println!("{name:<width$}  {numbers}  ok");
        } else {
            failed += 1;
            println!("{name:<width$}  {numbers}  {verdict} ({})", p.reason);
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
            "All {} photos match the reference ({}).",
            set.len(),
            current_meta.darktable
        );
        Ok(true)
    } else {
        println!(
            "{failed} of {} photos differ from the reference.",
            set.len()
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

/// Runs the executable scenarios that depend on darktable against the darktable application
/// (US-20) and lists the ones that need a manual check. Returns whether all passed.
pub fn scenarios(cfg: &Config) -> Result<bool> {
    let base = cfg.data_dir()?.join("selftest").join("scenarios");
    println!(
        "Starting darktable for the scenarios (private configuration and library in {}) …",
        base.display()
    );
    let app = crate::scenarios::app::App::start(cfg, &base.join("darktable"))?;
    let features = crate::scenarios::write_features(&base.join("features"))?;
    let app = std::sync::Arc::new(std::sync::Mutex::new(app));
    let ok = crate::scenarios::run(
        &features,
        crate::scenarios::Selection::Darktable,
        Some(app.clone()),
    );
    app.lock().unwrap_or_else(|e| e.into_inner()).stop();
    let manual = crate::scenarios::manual_checks();
    if !manual.is_empty() {
        println!("\nManual checks (can't be run unattended; do them after a darktable upgrade):");
        for m in manual {
            println!("  - {m}");
        }
    }
    println!();
    if ok {
        println!("All darktable scenarios passed.");
    } else {
        println!(
            "Some darktable scenarios failed; darktable's log: {}",
            base.join("darktable/darktable.log").display()
        );
    }
    Ok(ok)
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
