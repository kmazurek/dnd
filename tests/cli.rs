//! End-to-end tests of the command line surface.
//!
//! These only exercise code paths that exit before a window is opened, so they
//! run fine without a display.

use assert_cmd::Command;
use predicates::prelude::*;

fn dnd() -> Command {
    Command::cargo_bin("dnd").unwrap()
}

#[test]
fn help_lists_ripdrag_compatible_flags() {
    let assert = dnd().arg("--help").assert().success();
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    for flag in [
        "--target",
        "--keep",
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
        "--from-stdin",
        "--no-click",
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
fn keep_without_target_is_rejected() {
    dnd()
        .args(["-k"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--target"));
}

#[test]
fn paths_in_plain_target_mode_are_rejected() {
    dnd()
        .args(["-t", "Cargo.toml"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .assert()
        .failure()
        .stderr(predicate::str::contains("--keep"));
}
