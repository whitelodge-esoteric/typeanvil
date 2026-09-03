// SPDX-License-Identifier: AGPL-3.0-only

//! CORE-134: CLI surface — `--version` and `--help`.
//!
//! Maps 1:1 to the Behavior statements in
//! `docs/specifications/cli-surface.spec.md`.

use std::process::{Command, Output};

fn run_typeanvil(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_typeanvil"))
        .args(args)
        .output()
        .expect("spawn typeanvil binary")
}

/// AC1 — Behavior 1: `--version` prints `typeanvil <CARGO_PKG_VERSION>` and
/// exits 0. The expectation is built independently from the CLI: the test
/// re-reads `CARGO_PKG_VERSION` from this crate's metadata (same crate the
/// binary compiles from), not from the binary's own output.
#[test]
fn version_flag_prints_crate_version_and_exits_zero() {
    let out = run_typeanvil(&["--version"]);
    assert!(out.status.success(), "exit code must be 0");
    let expected = format!(
        "typeanvil {}\n",
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout), expected);
    assert!(out.stderr.is_empty(), "stderr must be empty");
}

/// AC1 — Behavior 1: `-V` behaves identically to `--version`.
#[test]
fn version_short_flag_matches_long_form() {
    let long = run_typeanvil(&["--version"]);
    let short = run_typeanvil(&["-V"]);
    assert!(short.status.success());
    assert_eq!(short.stdout, long.stdout);
}

/// AC2 — Behavior 2: `--help` prints usage covering subcommands and flags,
/// exits 0.
#[test]
fn help_prints_usage_and_exits_zero() {
    let out = run_typeanvil(&["--help"]);
    assert!(out.status.success(), "exit code must be 0");
    let text = String::from_utf8_lossy(&out.stdout);
    // Subcommand + the flags wrappers and the harness rely on.
    for needle in [
        "render",
        "--page-width",
        "--page-height",
        "-o",
        "--base-url",
        "--diagnostics",
        "--version",
        "--help",
    ] {
        assert!(text.contains(needle), "help output must mention `{needle}`");
    }
}

/// AC2 — Behavior 2: `-h` prints the same usage text as `--help`.
#[test]
fn help_short_flag_matches_long_form() {
    let long = run_typeanvil(&["--help"]);
    let short = run_typeanvil(&["-h"]);
    assert!(short.status.success());
    assert_eq!(short.stdout, long.stdout);
}

/// AC3 — Behavior 3: `--version` is honored only as the FIRST argument; in
/// any other position it is rejected (non-zero exit, stderr message) so
/// wrapper scripts can trust the exit code.
#[test]
fn version_in_non_first_position_is_rejected() {
    let out = run_typeanvil(&["render", "doc.html", "--version"]);
    assert!(!out.status.success(), "exit code must be non-zero");
    assert!(
        !out.stderr.is_empty(),
        "stderr must carry a message"
    );
}

/// AC3 — Behavior 3: `--help` in a non-first position is rejected too.
#[test]
fn help_in_non_first_position_is_rejected() {
    let out = run_typeanvil(&["render", "--help"]);
    assert!(!out.status.success(), "exit code must be non-zero");
    assert!(!out.stderr.is_empty());
}

/// AC4 — Behavior 4: no arguments prints short usage to stderr and exits
/// non-zero (pre-existing behavior, kept stable).
#[test]
fn no_arguments_prints_usage_to_stderr_and_fails() {
    let out = run_typeanvil(&[]);
    assert!(!out.status.success(), "exit code must be non-zero");
    assert!(
        !out.stderr.is_empty(),
        "stderr must carry the short usage"
    );
}
