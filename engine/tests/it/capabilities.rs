// SPDX-License-Identifier: AGPL-3.0-only

//! CORE-572: capability manifest (`typeanvil capabilities --json`).
//!
//! Maps to `docs/specifications/capability-manifest.spec.md` acceptance
//! criteria: CLI ships and `--help` lists it (AC1), no drift between the
//! diagnostics scanner and the manifest (AC2), determinism (AC3), and
//! diagnostics messages reference manifest entries (AC4).

use std::process::{Command, Output};

use typeanvil::capabilities::{lookup, manifest, to_json, Status};
use typeanvil::diagnostics::analyze;

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_typeanvil"))
        .args(args)
        .output()
        .expect("spawn typeanvil binary")
}

/// AC1: the subcommand prints the manifest and exits 0.
#[test]
fn cli_prints_manifest() {
    let out = run(&["capabilities", "--json"]);
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(stdout, to_json());
    assert!(stdout.starts_with("{\n  \"schema\": 1,"));
    assert!(stdout.contains("\"id\": \"property:color\""));
}

/// AC1: a missing `--json` is a usage error (exit 2).
#[test]
fn cli_without_json_flag_is_usage_error() {
    let out = run(&["capabilities"]);
    assert_eq!(out.status.code(), Some(2));
}

/// AC1: `--help` lists the subcommand.
#[test]
fn help_lists_capabilities() {
    let out = run(&["--help"]);
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("typeanvil capabilities --json"), "{s}");
    assert!(s.contains("  capabilities  "), "{s}");
}

/// AC2 (no drift): every property, at-rule and display value the scanner
/// flags resolves to `unsupported` / `partial` in the manifest, and every
/// manifest `supported` property is silent in the scanner.
#[test]
fn scanner_and_manifest_agree() {
    // Each manifest property / at-rule / display entry, fed to the scanner.
    for c in manifest() {
        let css = match c.kind {
            "property" => format!("p {{ {}: 1px; }}", c.name),
            "at-rule" => format!("@{} x {{ }}", c.name),
            "display" => format!("p {{ display: {}; }}", c.name),
            "page-descriptor" => format!("@page {{ {}: 1in; }}", c.name),
            _ => continue,
        };
        let flagged = !analyze(&css).is_empty();
        match c.status {
            Status::Supported => assert!(!flagged, "{} supported but scanner flags it", c.id()),
            Status::Partial | Status::Unsupported => {
                assert!(flagged, "{} is {:?} but scanner is silent", c.id(), c.status)
            }
        }
    }
    // A sweep of common properties: scanner flags ⇔ manifest says not supported.
    let sweep = [
        "color", "opacity", "transform", "text-indent", "letter-spacing", "border-radius",
        "box-shadow", "margin", "padding", "background", "font-variant", "grid-area",
        "text-decoration", "word-spacing", "width", "max-width",
    ];
    for p in sweep {
        let flagged = analyze(&format!("p {{ {p}: 1px; }}"))
            .iter()
            .any(|d| d.code == "unsupported-property");
        let status = lookup(&format!("property:{p}")).unwrap().status;
        assert_eq!(flagged, status != Status::Supported, "drift on `{p}`: {status:?}");
    }
    for r in ["import", "supports", "keyframes", "layer", "media", "page", "font-face"] {
        let flagged = !analyze(&format!("@{r} x {{ }}")).is_empty();
        let status = lookup(&format!("at-rule:{r}")).unwrap().status;
        assert_eq!(flagged, status != Status::Supported, "drift on `@{r}`");
    }
}

/// Regression (found by the drift test): the scanner binary-searches its
/// property table, so `flex*` was flagged while `float` sat before it.
#[test]
fn flex_family_is_not_flagged() {
    for p in ["flex", "flex-basis", "flex-direction", "flex-grow", "flex-shrink", "flex-wrap", "float"] {
        assert!(analyze(&format!("p {{ {p}: 1; }}")).is_empty(), "`{p}` flagged");
    }
}

/// AC3: deterministic, sorted, unique ids.
#[test]
fn manifest_is_deterministic_and_sorted() {
    assert_eq!(to_json(), to_json());
    assert_eq!(run(&["capabilities", "--json"]).stdout, run(&["capabilities", "--json"]).stdout);
    let m = manifest();
    for w in m.windows(2) {
        assert!((w[0].kind, &w[0].name) < (w[1].kind, &w[1].name), "{} !< {}", w[0].id(), w[1].id());
    }
    for c in &m {
        if c.status == Status::Supported {
            assert!(c.alternative.is_none(), "{} supported with an alternative", c.id());
        }
    }
}

/// AC4: a warning for an entry with an alternative names that entry.
#[test]
fn diagnostics_reference_manifest_alternative() {
    let ev = analyze("p { text-indent: 2em; }");
    assert_eq!(ev.len(), 1);
    let alt = lookup("property:text-indent").unwrap().alternative.unwrap();
    assert!(ev[0].message.contains("capability property:text-indent"), "{}", ev[0].message);
    assert!(ev[0].message.contains(alt), "{}", ev[0].message);
    let ev = analyze("@import x { }");
    assert!(ev[0].message.contains("capability at-rule:import"), "{}", ev[0].message);
}
