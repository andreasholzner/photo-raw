use clap::{Parser, Subcommand};
use photoflow::{config, process, selftest};
use std::path::PathBuf;
use std::process::ExitCode;

/// Automation around darktable for the repetitive parts of raw editing.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Configuration file (default: ~/.config/photoflow/config.toml)
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check that darktable still renders the reference photos as before
    #[command(subcommand)]
    Selftest(Selftest),
    /// Process the photos darktable hands over (called by photoflow's darktable script)
    Process {
        /// Hand-over file written by the darktable script
        handover: PathBuf,
        /// Plan file for the darktable script to carry out
        plan: PathBuf,
    },
    /// Try the aurora edit on raw photos, from darktable's defaults (development aid, US-10)
    #[command(hide = true)]
    AuroraEdit {
        /// Blue-violet sky instead of neutral blue
        #[arg(long)]
        violet: bool,
        /// Directory for the edits (<photo>.xmp) and their renders (<photo>.jpg)
        #[arg(long)]
        out: PathBuf,
        /// History to start from instead of darktable's defaults (an import's sidecar)
        #[arg(long)]
        base: Option<PathBuf>,
        #[arg(required = true)]
        photos: Vec<PathBuf>,
    },
    /// Find where the land meets the sky in raw photos and print it (US-23)
    Horizon {
        /// Write a preview per photo into this directory (sky tinted blue, skyline yellow)
        #[arg(long)]
        preview: Option<PathBuf>,
        /// Raw photos
        #[arg(required = true)]
        photos: Vec<PathBuf>,
    },
}

#[derive(Subcommand)]
enum Selftest {
    /// Render the reference set and store the renders as the reference
    Init {
        /// Replace existing reference renders
        #[arg(long)]
        force: bool,
    },
    /// Render the reference set and compare it with the reference, then run the darktable scenarios
    Run,
    /// Run only the scenarios that check darktable's behaviour (starts darktable for a few minutes)
    Scenarios {
        /// Only scenarios whose name, rule, feature or feature file contains this text
        #[arg(long)]
        only: Option<String>,
    },
    /// Make the renders of the last run the new reference
    Accept,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = config::Config::load(cli.config.as_deref()).and_then(|cfg| match cli.command {
        Command::Selftest(Selftest::Init { force }) => selftest::init(&cfg, force),
        Command::Selftest(Selftest::Run) => selftest::run(&cfg),
        Command::Selftest(Selftest::Scenarios { only }) => {
            selftest::scenarios(&cfg, only.as_deref())
        }
        Command::Selftest(Selftest::Accept) => selftest::accept(&cfg),
        Command::Process { handover, plan } => run_process(&cfg, &handover, &plan),
        Command::Horizon { preview, photos } => run_horizon(&cfg, preview.as_deref(), &photos),
        Command::AuroraEdit {
            violet,
            out,
            base,
            photos,
        } => run_aurora_edit(&cfg, violet, &out, base.as_deref(), &photos),
    });
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run_process(
    cfg: &config::Config,
    handover: &std::path::Path,
    plan: &std::path::Path,
) -> anyhow::Result<bool> {
    let mut tools = process::RealTools::new(cfg)?;
    process::run(handover, plan, &mut tools)?;
    Ok(true)
}

/// Prints each photo's skyline: its middle and extent as fractions of the frame height from the
/// top, or that it shows no land.
fn run_horizon(
    cfg: &config::Config,
    preview: Option<&std::path::Path>,
    photos: &[PathBuf],
) -> anyhow::Result<bool> {
    let data = cfg.data_dir()?;
    let work = data.join("horizon");
    let dt = photoflow::darktable::Darktable::prepare(
        &cfg.darktable.cli,
        &cfg.darktable_config_dir()?,
        &work.join("darktable-cli"),
    )?;
    let finder = photoflow::horizon::Finder::new(dt, &work.join("renders"), &data.join("models"))?;
    if let Some(dir) = preview {
        std::fs::create_dir_all(dir)?;
    }
    let mut ok = true;
    for raw in photos {
        match finder.find(raw) {
            Ok((line, img)) => {
                let what = match (line.has_land(), line.middle(), line.extent()) {
                    (true, Some(m), Some((hi, lo))) => {
                        format!("land starts at {m:.2} (from {hi:.2} to {lo:.2})")
                    }
                    (false, Some(_), _) => "no land".to_string(),
                    _ => "no sky".to_string(),
                };
                println!("{}: {what}", raw.display());
                if let Some(dir) = preview {
                    let name = raw.file_name().unwrap_or_default().to_string_lossy();
                    photoflow::horizon::preview(&img, &line, &dir.join(format!("{name}.png")))?;
                }
            }
            Err(e) => {
                eprintln!("{}: {e:#}", raw.display());
                ok = false;
            }
        }
    }
    Ok(ok)
}

fn run_aurora_edit(
    cfg: &config::Config,
    violet: bool,
    out: &std::path::Path,
    base: Option<&std::path::Path>,
    photos: &[PathBuf],
) -> anyhow::Result<bool> {
    use photoflow::aurora_edit::{self, SkyColour};
    let base = match base {
        Some(b) => std::fs::read_to_string(b)?,
        None => photoflow::day_edit::EMPTY_HISTORY.to_string(),
    };
    let data = cfg.data_dir()?;
    let work = data.join("aurora-edit");
    let prep = |d: &str| {
        photoflow::darktable::Darktable::prepare(
            &cfg.darktable.cli,
            &cfg.darktable_config_dir()?,
            &work.join(d),
        )
    };
    let finder = photoflow::horizon::Finder::new(
        prep("hz")?,
        &work.join("hz-renders"),
        &data.join("models"),
    )?;
    let check = photoflow::classify::AuroraCheck::new(prep("check")?, &work.join("check-renders"))?;
    let dt = prep("solve")?;
    std::fs::create_dir_all(out)?;
    let colour = if violet {
        SkyColour::BlueViolet
    } else {
        SkyColour::NeutralBlue
    };
    for raw in photos {
        let name = raw
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let (line, _) = finder.find(raw)?;
        let ratio = aurora_edit::scene_ratio(&check.render(raw)?);
        let target = aurora_edit::sky_target(ratio);
        let x = work.join(format!("{name}.solve.xmp"));
        let o = work.join(format!("{name}.solve.pfm"));
        let mut n = 0;
        let mut render = |xmp: &str| {
            n += 1;
            std::fs::write(&x, xmp)?;
            dt.render(raw, &x, &o, aurora_edit::RENDER_SIZE)?;
            photoflow::aurora::load_pfm(&o)
        };
        let solved = aurora_edit::solve(&base, colour, target, Some(&line), &mut render)?;
        let stem = raw.file_stem().unwrap_or_default().to_string_lossy();
        let xmp = out.join(format!("{stem}.xmp"));
        std::fs::write(&xmp, &solved.xmp)?;
        dt.render(raw, &xmp, &out.join(format!("{stem}.jpg")), 1600)?;
        println!(
            "{}: ratio {ratio:.2} target {target} ev {:.2} wb {:.2}/{:.2} land {:?} reached {} ({n} renders)",
            raw.display(),
            solved.params.ev,
            solved.params.wb.0,
            solved.params.wb.1,
            solved.params.land.as_ref().map(|l| l.1),
            solved.reached
        );
    }
    Ok(true)
}
