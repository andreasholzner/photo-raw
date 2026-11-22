//! Finding photos below the photo root by file name and folder year.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

/// Finds `file` directly inside a folder of `root` whose name starts with `year`.
/// Fails if there is no match or more than one.
pub fn locate(root: &Path, year: u16, file: &str) -> Result<PathBuf> {
    let prefix = year.to_string();
    let mut found = Vec::new();
    for entry in std::fs::read_dir(root)
        .with_context(|| format!("cannot read photo root {}", root.display()))?
    {
        let entry = entry?;
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with(&prefix) || !entry.file_type()?.is_dir() {
            continue;
        }
        let candidate = entry.path().join(file);
        if candidate.is_file() {
            found.push(candidate);
        }
    }
    match found.len() {
        0 => bail!(
            "{file} ({year}) not found in a {year}… folder below {}",
            root.display()
        ),
        1 => Ok(found.remove(0)),
        n => bail!("{file} ({year}) is ambiguous: found in {n} folders"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tree(name: &str, files: &[&str]) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("photoflow-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for f in files {
            let p = root.join(f);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, b"").unwrap();
        }
        root
    }

    #[test]
    fn finds_by_year() {
        let root = tree(
            "year",
            &["2018-03-25 A/_DSC0001.ARW", "2027-01-01 B/_DSC0001.ARW"],
        );
        assert_eq!(
            locate(&root, 2027, "_DSC0001.ARW").unwrap(),
            root.join("2027-01-01 B/_DSC0001.ARW")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_and_ambiguous_fail() {
        let root = tree("amb", &["2019 A/x.ARW", "2019-05-01 B/x.ARW"]);
        assert!(
            locate(&root, 2019, "x.ARW")
                .unwrap_err()
                .to_string()
                .contains("ambiguous")
        );
        assert!(
            locate(&root, 2020, "x.ARW")
                .unwrap_err()
                .to_string()
                .contains("not found")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ignores_subfolders() {
        let root = tree("sub", &["2019 A/exported/x.ARW"]);
        assert!(locate(&root, 2019, "x.ARW").is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
