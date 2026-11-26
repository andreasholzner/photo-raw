use clap::{Parser, Subcommand};
use photoflow::{classify, config, darktable, process, selftest};
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
    Scenarios,
    /// Make the renders of the last run the new reference
    Accept,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = config::Config::load(cli.config.as_deref()).and_then(|cfg| match cli.command {
        Command::Selftest(Selftest::Init { force }) => selftest::init(&cfg, force),
        Command::Selftest(Selftest::Run) => selftest::run(&cfg),
        Command::Selftest(Selftest::Scenarios) => selftest::scenarios(&cfg),
        Command::Selftest(Selftest::Accept) => selftest::accept(&cfg),
        Command::Process { handover, plan } => run_process(&cfg, &handover, &plan),
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
    // darktable-cli is prepared only when a night photo needs the aurora check
    let mut check: Option<classify::AuroraCheck> = None;
    process::run(handover, plan, |raw| {
        if check.is_none() {
            let work = cfg.data_dir()?.join("process");
            let dt = darktable::Darktable::prepare(
                &cfg.darktable.cli,
                &cfg.darktable_config_dir()?,
                &work,
            )?;
            check = Some(classify::AuroraCheck::new(dt, &work.join("renders"))?);
        }
        check.as_ref().unwrap().render(raw)
    })?;
    Ok(true)
}
