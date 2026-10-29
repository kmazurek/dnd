//! Command line interface.
//!
//! The flags intentionally mirror [ripdrag](https://github.com/nik012003/ripdrag)
//! (itself modelled after `dragon`) so that existing muscle memory and scripts
//! keep working.

use std::ops::{RangeFrom, RangeInclusive};
use std::path::PathBuf;

use clap::Parser;

/// Accepted values for `--icon-size`, also enforced for the config file.
pub const ICON_SIZES: RangeInclusive<i64> = 16..=512;
/// Accepted values for `--content-width`.
pub const CONTENT_WIDTHS: RangeFrom<i64> = 100..;
/// Accepted values for `--content-height`.
pub const CONTENT_HEIGHTS: RangeFrom<i64> = 60..;

/// Drag files from the terminal into other applications.
///
/// A small window lists the given files; drag them into any application.
///
/// Defaults for every option can be set in `$XDG_CONFIG_HOME/dnd/config.toml`
/// (usually `~/.config/dnd/config.toml`); flags given on the command line take
/// precedence.
#[derive(Debug, Clone, Parser)]
#[command(version, about, long_about)]
pub struct Cli {
    /// Files or directories to drag
    #[arg(value_name = "PATH")]
    pub paths: Vec<PathBuf>,

    /// Print extra diagnostics to stderr
    #[arg(short, long)]
    pub verbose: bool,

    /// Exit after the first successful drag
    #[arg(short = 'x', long)]
    pub and_exit: bool,

    /// Show icons only, without file names
    #[arg(short, long)]
    pub icons_only: bool,

    /// Do not render image thumbnails, use generic file icons instead
    #[arg(short, long)]
    pub disable_thumbnails: bool,

    /// Size of icons and thumbnails, in points [default: 32]
    #[arg(short = 's', long, value_parser = clap::value_parser!(u16).range(ICON_SIZES))]
    pub icon_size: Option<u16>,

    /// Show only the file name instead of the full path
    #[arg(short, long)]
    pub basename: bool,

    /// Make the window resizable
    #[arg(short, long)]
    pub resizable: bool,

    /// Width of the window content, in points [default: 360]
    #[arg(short = 'W', long, value_parser = clap::value_parser!(u16).range(CONTENT_WIDTHS))]
    pub content_width: Option<u16>,

    /// Height of the window content, in points [default: 360]
    #[arg(short = 'H', long, value_parser = clap::value_parser!(u16).range(CONTENT_HEIGHTS))]
    pub content_height: Option<u16>,

    /// Show the "drag all" row even for a single file. With more than one
    /// file it is shown by default
    #[arg(short = 'a', long, conflicts_with_all = ["all_compact", "no_all"])]
    pub all: bool,

    /// Show only a single row that drags all files at once
    #[arg(short = 'A', long, conflicts_with = "no_all")]
    pub all_compact: bool,

    /// Never show the "drag all" row
    #[arg(long)]
    pub no_all: bool,

    /// Read additional paths from stdin, one per line
    #[arg(short = 'I', long)]
    pub from_stdin: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("dnd").chain(args.iter().copied()))
    }

    #[test]
    fn command_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn defaults() {
        let cli = parse(&["a.txt", "b.txt"]).unwrap();
        assert_eq!(cli.paths, [PathBuf::from("a.txt"), PathBuf::from("b.txt")]);
        assert!(!cli.and_exit);
        // Left unset so that the config file can provide them.
        assert_eq!(cli.icon_size, None);
        assert_eq!((cli.content_width, cli.content_height), (None, None));
    }

    #[test]
    fn short_flags_match_ripdrag() {
        let cli = parse(&[
            "-v", "-x", "-i", "-d", "-b", "-r", "-a", "-I", "-s", "64", "-W", "200", "-H", "100",
            "f",
        ])
        .unwrap();
        assert!(cli.verbose && cli.and_exit && cli.icons_only && cli.disable_thumbnails);
        assert!(cli.basename && cli.resizable && cli.all && cli.from_stdin);
        assert_eq!(cli.icon_size, Some(64));
        assert_eq!(
            (cli.content_width, cli.content_height),
            (Some(200), Some(100))
        );
    }

    #[test]
    fn drag_all_flags_conflict() {
        assert!(parse(&["-a", "-A", "f"]).is_err());
        assert!(parse(&["-a", "--no-all", "f"]).is_err());
        assert!(parse(&["-A", "--no-all", "f"]).is_err());
        assert!(parse(&["--no-all", "f"]).unwrap().no_all);
    }

    #[test]
    fn icon_size_is_bounded() {
        assert!(parse(&["-s", "8", "f"]).is_err());
        assert!(parse(&["-s", "1024", "f"]).is_err());
        assert!(parse(&["-s", "nope", "f"]).is_err());
    }
}
