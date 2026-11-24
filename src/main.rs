mod config;
mod darktable;
#[allow(dead_code)] // used by the processing command (US-7)
mod exif;
mod image_diff;
mod locate;
mod reference_set;
#[allow(dead_code)] // used by the processing command (US-7)
mod scene;
mod selftest;
#[allow(dead_code)] // used by the processing command (US-8)
mod sidecar;

use clap::{Parser, Subcommand};
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
}

#[derive(Subcommand)]
enum Selftest {
    /// Render the reference set and store the renders as the reference
    Init {
        /// Replace existing reference renders
        #[arg(long)]
        force: bool,
    },
    /// Render the reference set and compare it with the reference
    Run,
    /// Make the renders of the last run the new reference
    Accept,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = config::Config::load(cli.config.as_deref()).and_then(|cfg| match cli.command {
        Command::Selftest(Selftest::Init { force }) => selftest::init(&cfg, force),
        Command::Selftest(Selftest::Run) => selftest::run(&cfg),
        Command::Selftest(Selftest::Accept) => selftest::accept(&cfg),
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
