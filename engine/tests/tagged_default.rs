// SPDX-License-Identifier: AGPL-3.0-only

//! CORE-378: tagged PDF is the default; `--no-tagged` opts out.
//!
//! Maps to Behavior 1-2 and the CLI contract in
//! `docs/specifications/tagged-pdf.spec.md`.

use std::path::PathBuf;
use std::process::Command;

const DOC: &str = r#"<html lang="en"><head><title>T</title></head><body>
<h1>Heading</h1><p>Body text for the default tagging check.</p></body></html>"#;

fn render(tag: &str, extra: &[&str]) -> Vec<u8> {
    let dir: PathBuf = std::env::temp_dir().join(format!("core378-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("in.html");
    let output = dir.join("out.pdf");
    std::fs::write(&input, DOC).unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_typeanvil"))
        .arg("render")
        .arg(&input)
        .args(["--page-width", "5in", "--page-height", "3in"])
        .args(["--margin-top", "0.5in", "--margin-right", "0.5in"])
        .args(["--margin-bottom", "0.5in", "--margin-left", "0.5in"])
        .args(extra)
        .arg("-o")
        .arg(&output)
        .status()
        .expect("spawn typeanvil");
    assert!(status.success(), "render {extra:?} must exit 0");
    let bytes = std::fs::read(&output).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    bytes
}

fn has_struct_tree(pdf: &[u8]) -> bool {
    pdf.windows(14).any(|w| w == b"/StructTreeRoo")
}

#[test]
fn default_render_is_tagged() {
    assert!(has_struct_tree(&render("default", &[])));
}

#[test]
fn no_tagged_render_is_untagged() {
    assert!(!has_struct_tree(&render("off", &["--no-tagged"])));
}

#[test]
fn tagged_flag_is_a_noop_alias_of_the_default() {
    assert_eq!(render("a", &[]), render("b", &["--tagged"]));
}

#[test]
fn ua_overrides_no_tagged_in_either_order() {
    let a = render("ua1", &["--ua", "--no-tagged"]);
    let b = render("ua2", &["--no-tagged", "--ua"]);
    assert!(has_struct_tree(&a));
    assert_eq!(a, b);
}
