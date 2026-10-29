//! The optional TOML config file, which provides defaults for every flag.
//!
//! Keys are named after the long flags. The "drag all" flags are combined
//! into a single `drag-all` key:
//!
//! ```toml
//! verbose = false
//! and-exit = true
//! icons-only = false
//! disable-thumbnails = false
//! icon-size = 48
//! basename = true
//! resizable = false
//! content-width = 360
//! content-height = 360
//! drag-all = "auto" # or "always", "compact", "never"
//! from-stdin = false
//! ```

use std::ffi::OsString;
use std::fmt::Debug;
use std::fs;
use std::io;
use std::ops::RangeBounds;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::cli::{CONTENT_HEIGHTS, CONTENT_WIDTHS, ICON_SIZES};
use crate::config::DragAll;

/// Settings read from the config file. Missing keys keep the built-in
/// defaults.
#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct ConfigFile {
    pub verbose: bool,
    pub and_exit: bool,
    pub icons_only: bool,
    pub disable_thumbnails: bool,
    pub icon_size: Option<u16>,
    pub basename: bool,
    pub resizable: bool,
    pub content_width: Option<u16>,
    pub content_height: Option<u16>,
    pub drag_all: Option<DragAll>,
    pub from_stdin: bool,
}

impl ConfigFile {
    /// Loads the file at `path`. A missing file is not an error and yields
    /// the defaults.
    pub fn load(path: &Path) -> Result<Self> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(err) => {
                return Err(err).with_context(|| format!("cannot read {}", path.display()));
            }
        };
        Self::parse(&text).with_context(|| format!("invalid config file {}", path.display()))
    }

    /// Parses and validates the contents of a config file.
    pub fn parse(text: &str) -> Result<Self> {
        let config: Self = toml::from_str(text)?;
        check_range("icon-size", config.icon_size, ICON_SIZES)?;
        check_range("content-width", config.content_width, CONTENT_WIDTHS)?;
        check_range("content-height", config.content_height, CONTENT_HEIGHTS)?;
        Ok(config)
    }
}

/// Applies the same bounds as the corresponding command line flag.
fn check_range(key: &str, value: Option<u16>, range: impl RangeBounds<i64> + Debug) -> Result<()> {
    if let Some(value) = value
        && !range.contains(&i64::from(value))
    {
        bail!("{key} = {value} is out of range (expected {range:?})");
    }
    Ok(())
}

/// Location of the config file: `$XDG_CONFIG_HOME/dnd/config.toml`, falling
/// back to `$HOME/.config/dnd/config.toml`. Returns `None` if neither
/// variable is usable.
pub fn path() -> Option<PathBuf> {
    path_from(
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    )
}

fn path_from(xdg_config_home: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    // The XDG spec says relative paths are invalid and must be ignored.
    let base = xdg_config_home
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .or_else(|| {
            home.filter(|home| !home.is_empty())
                .map(|home| PathBuf::from(home).join(".config"))
        })?;
    Some(base.join("dnd").join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_file_keeps_defaults() {
        assert_eq!(ConfigFile::parse("").unwrap(), ConfigFile::default());
    }

    #[test]
    fn every_key_is_read() {
        let config = ConfigFile::parse(
            r#"
            verbose = true
            and-exit = true
            icons-only = true
            disable-thumbnails = true
            icon-size = 48
            basename = true
            resizable = true
            content-width = 200
            content-height = 100
            drag-all = "compact"
            from-stdin = true
            "#,
        )
        .unwrap();
        assert_eq!(
            config,
            ConfigFile {
                verbose: true,
                and_exit: true,
                icons_only: true,
                disable_thumbnails: true,
                icon_size: Some(48),
                basename: true,
                resizable: true,
                content_width: Some(200),
                content_height: Some(100),
                drag_all: Some(DragAll::Compact),
                from_stdin: true,
            }
        );
    }

    #[test]
    fn drag_all_values() {
        let drag_all = |value| {
            ConfigFile::parse(&format!("drag-all = \"{value}\""))
                .unwrap()
                .drag_all
        };
        assert_eq!(drag_all("auto"), Some(DragAll::Auto));
        assert_eq!(drag_all("always"), Some(DragAll::Always));
        assert_eq!(drag_all("compact"), Some(DragAll::Compact));
        assert_eq!(drag_all("never"), Some(DragAll::Never));
        assert!(ConfigFile::parse("drag-all = \"sometimes\"").is_err());
    }

    #[test]
    fn unknown_keys_and_wrong_types_are_rejected() {
        assert!(ConfigFile::parse("icon_size = 48").is_err());
        assert!(ConfigFile::parse("verbose = \"yes\"").is_err());
        assert!(ConfigFile::parse("icon-size = -1").is_err());
    }

    #[test]
    fn values_are_bounded_like_flags() {
        for text in [
            "icon-size = 8",
            "icon-size = 1024",
            "content-width = 99",
            "content-height = 59",
        ] {
            let err = ConfigFile::parse(text).unwrap_err().to_string();
            assert!(err.contains("out of range"), "{text}: {err}");
        }
        assert!(ConfigFile::parse("icon-size = 512\ncontent-height = 60").is_ok());
    }

    #[test]
    fn missing_file_yields_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let config = ConfigFile::load(&dir.path().join("config.toml")).unwrap();
        assert_eq!(config, ConfigFile::default());
    }

    #[test]
    fn errors_name_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "nope = 1").unwrap();
        let err = format!("{:#}", ConfigFile::load(&path).unwrap_err());
        assert!(err.contains(&*path.to_string_lossy()), "{err}");
        assert!(err.contains("nope"), "{err}");
    }

    #[test]
    fn path_prefers_xdg_config_home() {
        let path = |xdg: Option<&str>, home: Option<&str>| {
            path_from(xdg.map(Into::into), home.map(Into::into))
        };
        assert_eq!(
            path(Some("/xdg"), Some("/home/me")),
            Some("/xdg/dnd/config.toml".into())
        );
        assert_eq!(
            path(None, Some("/home/me")),
            Some("/home/me/.config/dnd/config.toml".into())
        );
        // Relative and empty values are ignored.
        assert_eq!(
            path(Some("relative"), Some("/home/me")),
            Some("/home/me/.config/dnd/config.toml".into())
        );
        assert_eq!(
            path(Some(""), Some("/home/me")),
            Some("/home/me/.config/dnd/config.toml".into())
        );
        assert_eq!(path(None, Some("")), None);
        assert_eq!(path(None, None), None);
    }
}
