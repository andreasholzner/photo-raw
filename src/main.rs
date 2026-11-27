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
