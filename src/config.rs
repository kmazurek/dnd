//! Validated runtime configuration handed to the platform backends.

use std::io::BufRead;
use std::path::Path;

use anyhow::{Result, bail};

use crate::cli::Cli;
use crate::items::{self, Item};

/// When the "drag everything at once" row is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragAll {
    /// Above the individual rows whenever there is more than one file.
    Auto,
    /// Above the individual rows, even for a single file (`--all`).
    Always,
    /// Instead of the individual rows (`--all-compact`).
    Compact,
    /// Never (`--no-all`).
    Never,
}

/// Which rows to show for a given number of files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rows {
    pub all: bool,
    pub individual: bool,
}

impl DragAll {
    /// Decides the rows for `count` files.
    pub fn rows(self, count: usize) -> Rows {
        match self {
            DragAll::Auto => Rows {
                all: count > 1,
                individual: true,
            },
            DragAll::Always => Rows {
                all: true,
                individual: true,
            },
            DragAll::Compact => Rows {
                all: true,
                individual: false,
            },
            DragAll::Never => Rows {
                all: false,
                individual: true,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    pub items: Vec<Item>,
    pub verbose: bool,
    pub and_exit: bool,
    pub icons_only: bool,
    pub thumbnails: bool,
    pub icon_size: f64,
    pub resizable: bool,
    pub width: f64,
    pub height: f64,
    pub drag_all: DragAll,
}

impl Options {
    /// Builds the configuration from parsed arguments. `stdin` is only read
    /// when `--from-stdin` was given; relative paths are resolved against
    /// `cwd`.
    pub fn from_cli(cli: Cli, stdin: impl BufRead, cwd: &Path) -> Result<Self> {
        let mut paths = cli.paths;
        if cli.from_stdin {
            paths.extend(items::read_paths(stdin)?);
        }

        if paths.is_empty() {
            bail!("no files to drag: pass some paths or use --from-stdin");
        }

        let drag_all = if cli.all_compact {
            DragAll::Compact
        } else if cli.all {
            DragAll::Always
        } else if cli.no_all {
            DragAll::Never
        } else {
            DragAll::Auto
        };

        Ok(Self {
            items: items::resolve(&paths, cwd, cli.basename)?,
            verbose: cli.verbose,
            and_exit: cli.and_exit,
            icons_only: cli.icons_only,
            thumbnails: !cli.disable_thumbnails,
            icon_size: f64::from(cli.icon_size),
            resizable: cli.resizable,
            width: f64::from(cli.content_width),
            height: f64::from(cli.content_height),
            drag_all,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use std::fs;
    use std::io::Cursor;

    fn options(args: &[&str], stdin: &str, cwd: &Path) -> Result<Options> {
        let cli = Cli::try_parse_from(std::iter::once("dnd").chain(args.iter().copied()))?;
        Options::from_cli(cli, Cursor::new(stdin.to_owned()), cwd)
    }

    fn fixture() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        for name in ["a", "b", "c"] {
            fs::write(root.join(name), name).unwrap();
        }
        (dir, root)
    }

    #[test]
    fn paths_are_required() {
        let (_dir, root) = fixture();
        let err = options(&[], "", &root).unwrap_err().to_string();
        assert!(err.contains("no files to drag"), "{err}");
    }

    #[test]
    fn stdin_is_only_read_when_requested() {
        let (_dir, root) = fixture();
        let opts = options(&["a"], "b\nc\n", &root).unwrap();
        assert_eq!(opts.items.len(), 1);

        let opts = options(&["-I", "a"], "b\nc\n", &root).unwrap();
        let labels: Vec<_> = opts.items.iter().map(|i| i.path.clone()).collect();
        assert_eq!(labels, [root.join("a"), root.join("b"), root.join("c")]);
    }

    #[test]
    fn drag_all_variants() {
        let (_dir, root) = fixture();
        let drag_all = |args: &[&str]| options(args, "", &root).unwrap().drag_all;
        assert_eq!(drag_all(&["a"]), DragAll::Auto);
        assert_eq!(drag_all(&["-a", "a"]), DragAll::Always);
        assert_eq!(drag_all(&["-A", "a"]), DragAll::Compact);
        assert_eq!(drag_all(&["--no-all", "a"]), DragAll::Never);
    }

    #[test]
    fn drag_all_rows_by_count() {
        let rows = |all, individual| Rows { all, individual };
        // The default only adds the "all" row once it drags more than one row would.
        assert_eq!(DragAll::Auto.rows(1), rows(false, true));
        assert_eq!(DragAll::Auto.rows(2), rows(true, true));
        assert_eq!(DragAll::Always.rows(1), rows(true, true));
        assert_eq!(DragAll::Compact.rows(1), rows(true, false));
        assert_eq!(DragAll::Compact.rows(5), rows(true, false));
        assert_eq!(DragAll::Never.rows(5), rows(false, true));
    }

    #[test]
    fn thumbnails_flag_is_inverted() {
        let (_dir, root) = fixture();
        assert!(options(&["a"], "", &root).unwrap().thumbnails);
        assert!(!options(&["-d", "a"], "", &root).unwrap().thumbnails);
    }
}
