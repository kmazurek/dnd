//! GUI smoke tests for the Linux backend.
//!
//! The binary is run with `DND_SNAPSHOT`, which builds the real window, renders
//! its content to a PNG once it has been drawn and exits. This catches crashes
//! and layout regressions in the GTK code without user input. The window shows
//! up briefly while each test runs.
//!
//! The tests need a graphical session (Wayland or X11) and are skipped without
//! one. Set `DND_SKIP_GUI_TESTS=1` to skip them anyway.
#![cfg(target_os = "linux")]

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;

fn skip() -> bool {
    std::env::var_os("DND_SKIP_GUI_TESTS").is_some()
        || ["WAYLAND_DISPLAY", "DISPLAY"]
            .iter()
            .all(|var| std::env::var_os(var).is_none())
}

/// Width and height of a PNG, read from its IHDR chunk.
fn png_size(path: &Path) -> (u32, u32) {
    let data = fs::read(path).unwrap();
    assert!(data.len() > 24, "PNG too short");
    assert_eq!(&data[..8], b"\x89PNG\r\n\x1a\n", "not a PNG");
    assert_eq!(&data[12..16], b"IHDR");
    let be = |range: std::ops::Range<usize>| u32::from_be_bytes(data[range].try_into().unwrap());
    (be(16..20), be(20..24))
}

/// Renders the window for `args` and returns the PNG's size in pixels.
fn snapshot(args: &[&str]) -> (u32, u32) {
    let dir = tempfile::tempdir().unwrap();
    let png = dir.path().join("window.png");
    Command::cargo_bin("dnd")
        .unwrap()
        .env("XDG_CONFIG_HOME", "/nonexistent-dnd-config-home")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("DND_SNAPSHOT", &png)
        .args(args)
        .assert()
        .success();
    png_size(&png)
}

/// The snapshot covers only the window content, at one pixel per logical
/// pixel, so it must match the requested content size exactly. A tiling
/// window manager that resizes the window will fail this.
#[test]
fn source_window_renders() {
    if skip() {
        return;
    }
    let size = snapshot(&["-W", "300", "-H", "200", "Cargo.toml", "src", "Makefile"]);
    assert_eq!(size, (300, 200));
}

#[test]
fn source_window_variants_render() {
    if skip() {
        return;
    }
    for args in [
        &["-a", "-b", "Cargo.toml", "src"][..],
        &["-A", "Cargo.toml", "src"],
        &["--no-all", "Cargo.toml", "src"],
        &["-i", "-s", "64", "Cargo.toml"],
        &["-d", "-r", "Cargo.toml"],
    ] {
        assert_eq!(snapshot(args), (360, 360), "for {args:?}");
    }
}

#[test]
fn snapshot_errors_are_reported() {
    if skip() {
        return;
    }
    Command::cargo_bin("dnd")
        .unwrap()
        .env("XDG_CONFIG_HOME", "/nonexistent-dnd-config-home")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("DND_SNAPSHOT", "/nonexistent-dir/window.png")
        .arg("Cargo.toml")
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot write snapshot"));
}

#[test]
fn missing_display_is_reported() {
    if std::env::var_os("DND_SKIP_GUI_TESTS").is_some() {
        return;
    }
    Command::cargo_bin("dnd")
        .unwrap()
        .env("XDG_CONFIG_HOME", "/nonexistent-dnd-config-home")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("DISPLAY")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .arg("Cargo.toml")
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot open a display"));
}
