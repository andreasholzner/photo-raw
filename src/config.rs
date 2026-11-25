//! User configuration, read from `$XDG_CONFIG_HOME/photoflow/config.toml`.

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::path::{Path, PathBuf};

pub const EXAMPLE: &str = r#"# ~/.config/photoflow/config.toml
photo_root = "~/Pictures/raw"        # folder that contains the trip folders (required)
# data_dir = "~/.local/share/photoflow"

# [darktable]
# cli = "darktable-cli"
# app = "darktable"            # the darktable application, for the self-test's scenarios
# config_dir = "~/.config/darktable"

# [selftest]
# size = 1000          # longest edge of the renders in pixels
# max_mean_diff = 1.0  # tolerated mean difference, in 8-bit levels
# max_p99_diff = 8.0   # tolerated 99th-percentile difference, in 8-bit levels
# jobs = 3             # parallel darktable-cli processes
"#;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub photo_root: PathBuf,
    #[serde(default)]
    pub data_dir: Option<PathBuf>,
    #[serde(default)]
    pub darktable: DarktableConfig,
    #[serde(default)]
    pub selftest: SelftestConfig,
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DarktableConfig {
    pub cli: String,
    pub app: String,
    pub config_dir: Option<PathBuf>,
}

impl Default for DarktableConfig {
    fn default() -> Self {
        Self {
            cli: "darktable-cli".into(),
            app: "darktable".into(),
            config_dir: None,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SelftestConfig {
    pub size: u32,
    pub max_mean_diff: f64,
    pub max_p99_diff: f64,
    pub jobs: usize,
}

impl Default for SelftestConfig {
    fn default() -> Self {
        Self {
            size: 1000,
            max_mean_diff: 1.0,
            max_p99_diff: 8.0,
            jobs: 3,
        }
    }
}

impl Config {
    pub fn load(path: Option<&Path>) -> Result<Self> {
        let path = match path {
            Some(p) => p.to_path_buf(),
            None => xdg_dir("XDG_CONFIG_HOME", ".config")?.join("photoflow/config.toml"),
        };
        if !path.is_file() {
            bail!(
                "no configuration at {}\nCreate it, for example:\n\n{EXAMPLE}",
                path.display()
            );
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("cannot read the configuration {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("invalid configuration {}", path.display()))
    }

    pub fn parse(text: &str) -> Result<Self> {
        let mut cfg: Config = toml::from_str(text)?;
        cfg.photo_root = expand_home(&cfg.photo_root)?;
        cfg.data_dir = cfg.data_dir.as_deref().map(expand_home).transpose()?;
        cfg.darktable.config_dir = cfg
            .darktable
            .config_dir
            .as_deref()
            .map(expand_home)
            .transpose()?;
        if cfg.selftest.jobs == 0 {
            bail!("selftest.jobs must be at least 1");
        }
        Ok(cfg)
    }

    pub fn data_dir(&self) -> Result<PathBuf> {
        match &self.data_dir {
            Some(d) => Ok(d.clone()),
            None => Ok(xdg_dir("XDG_DATA_HOME", ".local/share")?.join("photoflow")),
        }
    }

    pub fn darktable_config_dir(&self) -> Result<PathBuf> {
        match &self.darktable.config_dir {
            Some(d) => Ok(d.clone()),
            None => Ok(xdg_dir("XDG_CONFIG_HOME", ".config")?.join("darktable")),
        }
    }
}

fn home() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .context("HOME is not set")
}

fn xdg_dir(var: &str, fallback: &str) -> Result<PathBuf> {
    match std::env::var_os(var) {
        Some(v) if !v.is_empty() => Ok(PathBuf::from(v)),
        _ => Ok(home()?.join(fallback)),
    }
}

fn expand_home(p: &Path) -> Result<PathBuf> {
    match p.strip_prefix("~") {
        Ok(rest) => Ok(home()?.join(rest)),
        Err(_) => Ok(p.to_path_buf()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_config_gets_defaults() {
        let cfg = Config::parse("photo_root = \"/photos\"").unwrap();
        assert_eq!(cfg.photo_root, PathBuf::from("/photos"));
        assert_eq!(cfg.darktable.cli, "darktable-cli");
        assert_eq!(cfg.selftest.size, 1000);
        assert!(cfg.data_dir.is_none());
    }

    #[test]
    fn example_config_is_valid() {
        let cfg = Config::parse(EXAMPLE).unwrap();
        assert!(cfg.photo_root.is_absolute());
    }

    #[test]
    fn unknown_keys_are_rejected() {
        assert!(Config::parse("photo_root = \"/p\"\nphoto_rot = \"/x\"").is_err());
    }

    #[test]
    fn zero_jobs_are_rejected() {
        assert!(Config::parse("photo_root = \"/p\"\n[selftest]\njobs = 0").is_err());
    }
}
