//! Validated runtime configuration handed to the platform backends.

use std::io::BufRead;
use std::path::Path;

use anyhow::{Result, bail};

use crate::cli::Cli;
use crate::items::{self, Item};

/// What the window does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Files are dragged out of the window.
    Source,
    /// Files are dropped into the window and printed to stdout.
    Target {
        /// Keep dropped files around as draggable items.
        keep: bool,
    },
}

/// How the "drag everything at once" item is presented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragAll {
    /// Only individual items.
    Off,
    /// An extra item above the individual ones.
    Extra,
    /// A single item replacing the individual ones.
    Compact,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    pub mode: Mode,
    pub items: Vec<Item>,
    pub verbose: bool,
    pub and_exit: bool,
    pub icons_only: bool,
    pub thumbnails: bool,
    pub icon_size: f64,
    pub basename: bool,
    pub resizable: bool,
    pub width: f64,
    pub height: f64,
    pub drag_all: DragAll,
    pub click_to_open: bool,
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

        let mode = if cli.target {
            Mode::Target { keep: cli.keep }
        } else {
            Mode::Source
        };
        match mode {
            Mode::Source if paths.is_empty() => {
                bail!("no files to drag: pass some paths, use --from-stdin or --target")
            }
            Mode::Target { keep: false } if !paths.is_empty() => {
                bail!("paths can only be shown in target mode together with --keep")
            }
            _ => {}
        }

        let drag_all = match (cli.all, cli.all_compact) {
            (_, true) => DragAll::Compact,
            (true, false) => DragAll::Extra,
            (false, false) => DragAll::Off,
        };

        Ok(Self {
            mode,
            items: items::resolve(&paths, cwd, cli.basename)?,
            verbose: cli.verbose,
            and_exit: cli.and_exit,
            icons_only: cli.icons_only,
            thumbnails: !cli.disable_thumbnails,
            icon_size: f64::from(cli.icon_size),
            basename: cli.basename,
            resizable: cli.resizable,
            width: f64::from(cli.content_width),
            height: f64::from(cli.content_height),
            drag_all,
            click_to_open: !cli.no_click,
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
    fn source_mode_requires_paths() {
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
    fn target_mode() {
        let (_dir, root) = fixture();
        let opts = options(&["-t"], "", &root).unwrap();
        assert_eq!(opts.mode, Mode::Target { keep: false });
        assert!(opts.items.is_empty());

        assert!(options(&["-t", "a"], "", &root).is_err());

        let opts = options(&["-t", "-k", "a"], "", &root).unwrap();
        assert_eq!(opts.mode, Mode::Target { keep: true });
        assert_eq!(opts.items.len(), 1);
    }

    #[test]
    fn drag_all_variants() {
        let (_dir, root) = fixture();
        assert_eq!(options(&["a"], "", &root).unwrap().drag_all, DragAll::Off);
        assert_eq!(
            options(&["-a", "a"], "", &root).unwrap().drag_all,
            DragAll::Extra
        );
        assert_eq!(
            options(&["-A", "a"], "", &root).unwrap().drag_all,
            DragAll::Compact
        );
    }

    #[test]
    fn inverted_flags() {
        let (_dir, root) = fixture();
        let opts = options(&["a"], "", &root).unwrap();
        assert!(opts.thumbnails && opts.click_to_open);
        let opts = options(&["-d", "-n", "a"], "", &root).unwrap();
        assert!(!opts.thumbnails && !opts.click_to_open);
    }
}
