//! Validated runtime configuration handed to the platform backends.

use std::io::BufRead;
use std::path::Path;

use anyhow::{Result, bail};
use serde::Deserialize;

use crate::cli::Cli;
use crate::config_file::ConfigFile;
use crate::items::{self, Item};

/// Icon size used when neither the command line nor the config file sets one.
pub const DEFAULT_ICON_SIZE: u16 = 32;
/// Content width and height used when neither the command line nor the
/// config file sets them.
pub const DEFAULT_CONTENT_SIZE: u16 = 360;

/// When the "drag everything at once" row is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
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
    /// Builds the configuration from parsed arguments, falling back to `file`
    /// for anything not given on the command line. `stdin` is only read when
    /// `--from-stdin` is in effect; relative paths are resolved against `cwd`.
    pub fn from_cli(cli: Cli, file: ConfigFile, stdin: impl BufRead, cwd: &Path) -> Result<Self> {
        let mut paths = cli.paths;
        if cli.from_stdin || file.from_stdin {
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
            file.drag_all.unwrap_or(DragAll::Auto)
        };
        let size = |cli: Option<u16>, file: Option<u16>, default| {
            f64::from(cli.or(file).unwrap_or(default))
        };

        Ok(Self {
            items: items::resolve(&paths, cwd, cli.basename || file.basename)?,
            verbose: cli.verbose || file.verbose,
            and_exit: cli.and_exit || file.and_exit,
            icons_only: cli.icons_only || file.icons_only,
            thumbnails: !(cli.disable_thumbnails || file.disable_thumbnails),
            icon_size: size(cli.icon_size, file.icon_size, DEFAULT_ICON_SIZE),
            resizable: cli.resizable || file.resizable,
            width: size(cli.content_width, file.content_width, DEFAULT_CONTENT_SIZE),
            height: size(
                cli.content_height,
                file.content_height,
                DEFAULT_CONTENT_SIZE,
            ),
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
        options_with_file(args, "", stdin, cwd)
    }

    fn options_with_file(args: &[&str], file: &str, stdin: &str, cwd: &Path) -> Result<Options> {
        let cli = Cli::try_parse_from(std::iter::once("dnd").chain(args.iter().copied()))?;
        let file = ConfigFile::parse(file)?;
        Options::from_cli(cli, file, Cursor::new(stdin.to_owned()), cwd)
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

    #[test]
    fn defaults_without_flags_or_config_file() {
        let (_dir, root) = fixture();
        let opts = options(&["a"], "", &root).unwrap();
        assert_eq!(opts.icon_size, 32.0);
        assert_eq!((opts.width, opts.height), (360.0, 360.0));
        assert!(!(opts.verbose || opts.and_exit || opts.icons_only || opts.resizable));
        assert_eq!(opts.items[0].label, root.join("a").to_string_lossy());
    }

    #[test]
    fn config_file_provides_defaults() {
        let (_dir, root) = fixture();
        let file = r#"
            verbose = true
            and-exit = true
            icons-only = true
            disable-thumbnails = true
            icon-size = 48
            basename = true
            resizable = true
            content-width = 200
            content-height = 100
            drag-all = "never"
            from-stdin = true
        "#;
        let opts = options_with_file(&[], file, "a\n", &root).unwrap();
        assert!(opts.verbose && opts.and_exit && opts.icons_only && opts.resizable);
        assert!(!opts.thumbnails);
        assert_eq!(opts.icon_size, 48.0);
        assert_eq!((opts.width, opts.height), (200.0, 100.0));
        assert_eq!(opts.drag_all, DragAll::Never);
        assert_eq!(opts.items[0].label, "a");
    }

    #[test]
    fn flags_override_config_file() {
        let (_dir, root) = fixture();
        let file = "icon-size = 48\ncontent-width = 200\ndrag-all = \"never\"";
        let opts = options_with_file(&["-s", "64", "-W", "300", "-A", "a"], file, "", &root);
        let opts = opts.unwrap();
        assert_eq!(opts.icon_size, 64.0);
        assert_eq!(opts.width, 300.0);
        assert_eq!(opts.drag_all, DragAll::Compact);
        // Keys missing from both fall back to the built-in defaults.
        assert_eq!(opts.height, 360.0);
    }
}
