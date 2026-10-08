//! Platform independent handling of the files shown in the window.

use std::collections::HashSet;
use std::io::BufRead;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// A file that can be dragged out of the window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// Absolute, canonical path of the file.
    pub path: PathBuf,
    /// Text shown next to the icon.
    pub label: String,
}

impl Item {
    /// Creates an item for an already absolute path.
    pub fn new(path: PathBuf, basename: bool) -> Self {
        let label = label_for(&path, basename);
        Self { path, label }
    }
}

/// Returns the label shown for `path`: either the full path or only its last
/// component. Paths without a final component (such as `/`) fall back to the
/// full path.
pub fn label_for(path: &Path, basename: bool) -> String {
    if basename && let Some(name) = path.file_name() {
        return name.to_string_lossy().into_owned();
    }
    path.to_string_lossy().into_owned()
}

/// Reads one path per line, skipping blank lines and trailing `\r`.
///
/// Lines are not otherwise trimmed because file names may legitimately start
/// or end with spaces.
pub fn read_paths(reader: impl BufRead) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for line in reader.lines() {
        let line = line.context("failed to read paths from stdin")?;
        let line = line.strip_suffix('\r').unwrap_or(&line);
        if !line.trim().is_empty() {
            paths.push(PathBuf::from(line));
        }
    }
    Ok(paths)
}

/// Resolves `paths` relative to `cwd`, checks that they exist and removes
/// duplicates while preserving order.
///
/// All missing paths are reported at once so the user can fix them in one go.
pub fn resolve(paths: &[PathBuf], cwd: &Path, basename: bool) -> Result<Vec<Item>> {
    let mut seen = HashSet::new();
    let mut items = Vec::with_capacity(paths.len());
    let mut missing = Vec::new();

    for path in paths {
        match cwd.join(path).canonicalize() {
            Ok(abs) => {
                if seen.insert(abs.clone()) {
                    items.push(Item::new(abs, basename));
                }
            }
            Err(err) => missing.push(format!("  {}: {err}", path.display())),
        }
    }

    if !missing.is_empty() {
        bail!("cannot access the following paths:\n{}", missing.join("\n"));
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Cursor;

    #[test]
    fn label_full_path_and_basename() {
        let p = Path::new("/tmp/some dir/file.txt");
        assert_eq!(label_for(p, false), "/tmp/some dir/file.txt");
        assert_eq!(label_for(p, true), "file.txt");
        assert_eq!(label_for(Path::new("/"), true), "/");
    }

    #[test]
    fn read_paths_skips_blank_lines_and_crlf() {
        let input = Cursor::new("a.txt\r\n\n   \n b c.txt \nlast");
        let paths = read_paths(input).unwrap();
        assert_eq!(
            paths,
            [
                PathBuf::from("a.txt"),
                PathBuf::from(" b c.txt "),
                PathBuf::from("last")
            ]
        );
    }

    #[test]
    fn resolve_makes_paths_absolute_and_dedupes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        fs::write(root.join("a.txt"), "a").unwrap();
        fs::create_dir(root.join("sub")).unwrap();

        let items = resolve(
            &[
                "a.txt".into(),
                "sub".into(),
                "./a.txt".into(),
                "sub/../a.txt".into(),
            ],
            &root,
            true,
        )
        .unwrap();

        assert_eq!(
            items,
            [
                Item {
                    path: root.join("a.txt"),
                    label: "a.txt".into()
                },
                Item {
                    path: root.join("sub"),
                    label: "sub".into()
                },
            ]
        );
    }

    #[test]
    fn resolve_keeps_absolute_paths() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().canonicalize().unwrap().join("x");
        fs::write(&file, "").unwrap();
        let items = resolve(std::slice::from_ref(&file), Path::new("/"), false).unwrap();
        assert_eq!(items[0].path, file);
        assert_eq!(items[0].label, file.to_string_lossy());
    }

    #[test]
    fn resolve_reports_every_missing_path() {
        let dir = tempfile::tempdir().unwrap();
        let err = resolve(&["nope1".into(), "nope2".into()], dir.path(), false)
            .unwrap_err()
            .to_string();
        assert!(err.contains("nope1") && err.contains("nope2"), "{err}");
    }
}
