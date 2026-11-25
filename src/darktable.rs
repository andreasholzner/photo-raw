//! Rendering with darktable-cli, isolated from the owner's darktable.
//!
//! darktable-cli runs with a private copy of the owner's darktable configuration (settings,
//! presets/styles database, colour profiles) and an in-memory library, so the owner's library
//! and configuration are never touched. Sources are rendered through symlinks in a work
//! folder, so nothing can be written next to the owner's photos.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Darktable {
    cli: String,
    config_copy: PathBuf,
    links: PathBuf,
}

impl Darktable {
    /// Refreshes the private configuration copy in `work/darktable-config` from `owner_config`.
    pub fn prepare(cli: &str, owner_config: &Path, work: &Path) -> Result<Self> {
        let config_copy = work.join("darktable-config");
        let links = work.join("links");
        if config_copy.exists() {
            std::fs::remove_dir_all(&config_copy)?;
        }
        std::fs::create_dir_all(&config_copy)?;
        std::fs::create_dir_all(&links)?;

        let rc = owner_config.join("darktablerc");
        let text = std::fs::read_to_string(&rc)
            .with_context(|| format!("cannot read {}", rc.display()))?;
        std::fs::write(config_copy.join("darktablerc"), never_write_sidecars(&text))?;
        let db = owner_config.join("data.db");
        if db.is_file() {
            std::fs::copy(&db, config_copy.join("data.db"))?;
        }
        let color = owner_config.join("color");
        if color.is_dir() {
            copy_dir(&color, &config_copy.join("color"))?;
        }
        Ok(Self {
            cli: cli.to_string(),
            config_copy,
            links,
        })
    }

    /// First line of `darktable-cli --version`, e.g. "darktable 5.6.1".
    pub fn version(&self) -> Result<String> {
        let out = Command::new(&self.cli)
            .arg("--version")
            .output()
            .with_context(|| format!("cannot run {}", self.cli))?;
        let text = String::from_utf8_lossy(&out.stdout);
        Ok(text.lines().next().unwrap_or("").trim().to_string())
    }

    /// Renders `source` with the edit in `xmp` to `out`, longest edge `size` pixels. The format
    /// follows `out`'s extension: `.png` for 8-bit display output, `.pfm` for linear floats.
    pub fn render(&self, source: &Path, xmp: &Path, out: &Path, size: u32) -> Result<()> {
        let name = source.file_name().context("source without file name")?;
        let link = self.links.join(name);
        if link.symlink_metadata().is_ok() {
            std::fs::remove_file(&link)?;
        }
        std::os::unix::fs::symlink(source, &link)?;
        if out.exists() {
            std::fs::remove_file(out)?; // darktable-cli would otherwise write "<name>_01.png"
        }
        let size = size.to_string();
        let result = Command::new(&self.cli)
            .arg(&link)
            .arg(xmp)
            .arg(out)
            .args([
                "--width",
                &size,
                "--height",
                &size,
                "--hq",
                "false",
                "--apply-custom-presets",
                "false",
            ])
            .arg("--core")
            .arg("--configdir")
            .arg(&self.config_copy)
            .args(["--library", ":memory:"])
            .output()
            .with_context(|| format!("cannot run {}", self.cli))?;
        if !out.is_file() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            let tail: Vec<&str> = stderr.lines().rev().take(5).collect();
            bail!(
                "darktable-cli produced no output for {}: {}",
                source.display(),
                tail.join(" | ")
            );
        }
        Ok(())
    }
}

fn never_write_sidecars(rc: &str) -> String {
    let mut out: Vec<String> = rc
        .lines()
        .filter(|l| !l.starts_with("write_sidecar_files="))
        .map(str::to_string)
        .collect();
    out.push("write_sidecar_files=never".into());
    out.join("\n") + "\n"
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidecar_writing_is_disabled() {
        let rc = never_write_sidecars("a=1\nwrite_sidecar_files=on import\nb=2\n");
        assert_eq!(rc, "a=1\nb=2\nwrite_sidecar_files=never\n");
    }
}
