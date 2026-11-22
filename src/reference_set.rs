//! The reference set for the self-test, built into the binary.

use anyhow::{Result, bail};
use serde::Deserialize;
use std::collections::HashSet;

const SET: &str = include_str!("../data/reference-set.toml");

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefPhoto {
    /// File name of the raw or panorama, without folder.
    pub file: String,
    /// Year at the start of the photo's folder name.
    pub year: u16,
    pub reason: String,
}

#[derive(Deserialize)]
struct SetFile {
    photo: Vec<RefPhoto>,
}

pub fn load() -> Result<Vec<RefPhoto>> {
    parse(SET)
}

fn parse(text: &str) -> Result<Vec<RefPhoto>> {
    let set: SetFile = toml::from_str(text)?;
    let mut seen = HashSet::new();
    for p in &set.photo {
        if p.file.contains('/') {
            bail!("reference set: {} must be a file name, not a path", p.file);
        }
        if !seen.insert((p.year, p.file.clone())) {
            bail!("reference set: {} ({}) is listed twice", p.file, p.year);
        }
    }
    Ok(set.photo)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_set_is_valid() {
        let set = load().unwrap();
        assert_eq!(set.len(), 30);
    }

    #[test]
    fn duplicates_are_rejected() {
        let t = "[[photo]]\nfile='a.ARW'\nyear=2020\nreason=''\n[[photo]]\nfile='a.ARW'\nyear=2020\nreason=''";
        assert!(parse(t).is_err());
    }

    #[test]
    fn paths_are_rejected() {
        assert!(parse("[[photo]]\nfile='x/a.ARW'\nyear=2020\nreason=''").is_err());
    }
}
