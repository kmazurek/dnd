//! GUI smoke tests for the macOS backend.
//!
//! The binary is run with `DND_SNAPSHOT`, which builds the real window, renders
//! it to a PNG and exits instead of entering the event loop. This catches
//! crashes and layout regressions in the AppKit code without user input.
//!
//! The tests need a logged-in GUI session (a WindowServer connection). Set
//! `DND_SKIP_GUI_TESTS=1` to skip them, e.g. over SSH.
#![cfg(target_os = "macos")]

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;

fn skip() -> bool {
    std::env::var_os("DND_SKIP_GUI_TESTS").is_some()
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
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("DND_SNAPSHOT", &png)
        .args(args)
        .assert()
        .success();
    png_size(&png)
}

/// The rendered width must be the requested content width at some backing
/// scale (1x or 2x/3x for Retina); the height also includes the title bar.
fn assert_window_size((width, height): (u32, u32), content_width: u32, content_height: u32) {
    let scale = width / content_width;
    assert!(
        (1..=3).contains(&scale) && width == content_width * scale,
        "unexpected width {width} for content width {content_width}"
    );
    assert!(
        height > content_height * scale,
        "height {height} lacks a title bar"
    );
}

#[test]
fn source_window_renders() {
    if skip() {
        return;
    }
    let size = snapshot(&["-W", "300", "-H", "200", "Cargo.toml", "src", "Makefile"]);
    assert_window_size(size, 300, 200);
}

#[test]
fn source_window_variants_render() {
    if skip() {
        return;
    }
    for args in [
        &["-a", "-b", "Cargo.toml", "src"][..],
        &["-A", "Cargo.toml", "src"],
        &["-i", "-s", "64", "Cargo.toml"],
        &["-d", "-n", "-r", "Cargo.toml"],
    ] {
        assert_window_size(snapshot(args), 360, 360);
    }
}

#[test]
fn target_windows_render() {
    if skip() {
        return;
    }
    assert_window_size(snapshot(&["-t", "-W", "250", "-H", "100"]), 250, 100);
    assert_window_size(snapshot(&["-t", "-k", "-a", "Cargo.toml", "src"]), 360, 360);
}

#[test]
fn snapshot_errors_are_reported() {
    if skip() {
        return;
    }
    Command::cargo_bin("dnd")
        .unwrap()
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("DND_SNAPSHOT", "/nonexistent-dir/window.png")
        .arg("Cargo.toml")
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot write snapshot"));
}
