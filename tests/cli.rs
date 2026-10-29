//! End-to-end tests of the command line surface.
//!
//! These only exercise code paths that exit before a window is opened, so they
//! run fine without a display.

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;

/// Runs the binary without a config file, so the user's own settings cannot
/// affect the outcome.
fn dnd() -> Command {
    with_config_home(Path::new("/nonexistent-dnd-config-home"))
}

/// Runs the binary with `$XDG_CONFIG_HOME` set to `dir`.
fn with_config_home(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("dnd").unwrap();
    cmd.env("XDG_CONFIG_HOME", dir);
    cmd
}

/// Creates a config home holding `config.toml` with `contents`.
fn config_home(contents: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("dnd")).unwrap();
    fs::write(dir.path().join("dnd/config.toml"), contents).unwrap();
    dir
}

#[test]
fn help_lists_ripdrag_compatible_flags() {
    let assert = dnd().arg("--help").assert().success();
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    for flag in [
        "--and-exit",
        "--icons-only",
        "--disable-thumbnails",
        "--icon-size",
        "--basename",
        "--resizable",
        "--content-width",
        "--content-height",
        "--all",
        "--all-compact",
        "--no-all",
        "--from-stdin",
        "--verbose",
    ] {
        assert!(out.contains(flag), "missing {flag} in help:\n{out}");
    }
}

#[test]
fn version() {
    dnd()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn no_arguments_is_an_error() {
    dnd()
        .assert()
        .failure()
        .stderr(predicate::str::contains("no files to drag"));
}

#[test]
fn missing_files_are_reported() {
    let dir = tempfile::tempdir().unwrap();
    dnd()
        .current_dir(dir.path())
        .args(["missing-one", "missing-two"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("missing-one").and(predicate::str::contains("missing-two")),
        );
}

#[test]
fn missing_files_from_stdin_are_reported() {
    let dir = tempfile::tempdir().unwrap();
    dnd()
        .current_dir(dir.path())
        .arg("-I")
        .write_stdin("from-stdin.txt\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("from-stdin.txt"));
}

#[test]
fn config_file_options_apply() {
    // `from-stdin` from the file makes the binary read (and report) stdin.
    let home = config_home("from-stdin = true\n");
    let dir = tempfile::tempdir().unwrap();
    with_config_home(home.path())
        .current_dir(dir.path())
        .write_stdin("from-config.txt\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("from-config.txt"));
}

#[test]
fn invalid_config_file_is_reported() {
    let home = config_home("icon-size = 8\n");
    with_config_home(home.path())
        .arg("Cargo.toml")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .assert()
        .failure()
        .stderr(predicate::str::contains("config.toml").and(predicate::str::contains("icon-size")));
}

#[test]
fn unknown_config_keys_are_reported() {
    let home = config_home("target = true\n");
    with_config_home(home.path())
        .arg("Cargo.toml")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .assert()
        .failure()
        .stderr(predicate::str::contains("target"));
}
