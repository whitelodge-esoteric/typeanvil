// SPDX-License-Identifier: AGPL-3.0-only

//! CORE-576: data template mode (`render --data`).
//!
//! Maps to the Acceptance Criteria of
//! `docs/specifications/data-templates.spec.md`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_typeanvil"))
}

fn render(input: &Path, data: &Path, out: &Path, extra: &[&str]) -> Output {
    let mut args: Vec<String> = vec![
        "render".into(),
        input.to_string_lossy().into_owned(),
        "--data".into(),
        data.to_string_lossy().into_owned(),
        "--page-width".into(),
        "8.5in".into(),
        "--page-height".into(),
        "11in".into(),
        "-o".into(),
        out.to_string_lossy().into_owned(),
    ];
    args.extend(extra.iter().map(|s| s.to_string()));
    bin().args(&args).output().expect("spawn typeanvil")
}

fn templates_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../templates")
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// AC1: valid data renders byte-identically across runs.
#[test]
fn valid_data_renders_byte_identically() {
    let dir = tempfile::tempdir().unwrap();
    let t = templates_dir().join("invoice");
    let a = dir.path().join("a.pdf");
    let b = dir.path().join("b.pdf");
    for out in [&a, &b] {
        let o = render(&t.join("invoice.tmpl.html"), &t.join("invoice.example.json"), out, &[]);
        assert!(o.status.success(), "render failed: {}", stderr(&o));
    }
    let (a, b) = (std::fs::read(a).unwrap(), std::fs::read(b).unwrap());
    assert!(a.len() > 1000, "PDF is non-trivial");
    assert_eq!(a, b, "identical template + data must give identical bytes");
}

/// AC2: invalid data yields schema diagnostics with pointers, exit 5, no PDF.
#[test]
fn invalid_data_yields_schema_diagnostics_no_pdf() {
    let dir = tempfile::tempdir().unwrap();
    let t = templates_dir().join("invoice");
    let data = dir.path().join("bad.json");
    std::fs::write(
        &data,
        r#"{"number": "X1", "issue_date": "2026-10-25", "currency": "EUR",
            "seller": {"name": "S"}, "buyer": {"name": "B"},
            "items": [{"description": "a", "quantity": "two", "unit_price": 1}],
            "tax_rate": 7, "colour": "red"}"#,
    )
    .unwrap();
    let out = dir.path().join("o.pdf");
    let o = render(&t.join("invoice.tmpl.html"), &data, &out, &["--diagnostics", "json"]);
    assert_eq!(o.status.code(), Some(5), "stderr: {}", stderr(&o));
    assert!(!out.exists(), "no PDF on schema errors");

    let j: serde_json::Value = serde_json::from_slice(&o.stdout).expect("stdout is JSON");
    let errs = j["template_errors"].as_array().unwrap();
    let ptrs: Vec<&str> = errs.iter().map(|e| e["pointer"].as_str().unwrap()).collect();
    assert!(errs.iter().all(|e| e["code"] == "TEMPLATE_DATA_SCHEMA"));
    for want in ["/due_date", "/items/0/quantity", "/tax_rate", "/colour"] {
        assert!(ptrs.contains(&want), "missing {want} in {ptrs:?}");
    }
    let err = stderr(&o);
    assert!(err.contains("[TEMPLATE_DATA_SCHEMA] /items/0/quantity: expected type"), "{err}");
}

/// AC3: values are HTML-escaped by default.
#[test]
fn values_are_html_escaped() {
    let html = typeanvil::template::expand(
        "<p>{{ name }}</p>",
        &serde_json::json!({"name": "<script>alert(1)</script>"}),
    )
    .unwrap();
    assert_eq!(html, "<p>&lt;script&gt;alert(1)&lt;&#x2f;script&gt;</p>");
}

/// AC4: include / import / extends cannot reach the filesystem.
#[test]
fn include_is_sandboxed() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("secret.html"), "TOPSECRET").unwrap();
    let tmpl = dir.path().join("t.tmpl.html");
    std::fs::write(&tmpl, r#"<p>{% include "secret.html" %}</p>"#).unwrap();
    let data = dir.path().join("d.json");
    std::fs::write(&data, "{}").unwrap();
    let out = dir.path().join("o.pdf");
    let o = render(&tmpl, &data, &out, &[]);
    assert_eq!(o.status.code(), Some(5), "stderr: {}", stderr(&o));
    assert!(stderr(&o).contains("[TEMPLATE_ERROR]"), "{}", stderr(&o));
    assert!(!out.exists());
}

/// AC5: malformed JSON data.
#[test]
fn invalid_json_data() {
    let dir = tempfile::tempdir().unwrap();
    let tmpl = dir.path().join("t.html");
    std::fs::write(&tmpl, "<p>{{ x }}</p>").unwrap();
    let data = dir.path().join("d.json");
    std::fs::write(&data, "{\"x\": ").unwrap();
    let out = dir.path().join("o.pdf");
    let o = render(&tmpl, &data, &out, &[]);
    assert_eq!(o.status.code(), Some(5));
    assert!(stderr(&o).contains("[TEMPLATE_DATA_INVALID_JSON]"), "{}", stderr(&o));
    assert!(!out.exists());
}

/// AC6: the template publishes its schema; capabilities lists the format.
#[test]
fn template_schema_is_published() {
    let t = templates_dir().join("report").join("report.tmpl.html");
    let o = bin().args(["template-schema", t.to_str().unwrap()]).output().unwrap();
    assert!(o.status.success(), "{}", stderr(&o));
    let got: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    let want: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(templates_dir().join("report/report.schema.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(got, want);

    let caps = typeanvil::capabilities::to_json();
    assert!(caps.contains("\"template\""), "capabilities lists the template input format");
}

/// AC7: both shipped examples validate and render.
#[test]
fn shipped_examples_render() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["invoice", "report"] {
        let t = templates_dir().join(name);
        let schema: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(t.join(format!("{name}.schema.json"))).unwrap(),
        )
        .unwrap();
        let data: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(t.join(format!("{name}.example.json"))).unwrap(),
        )
        .unwrap();
        assert!(typeanvil::template::validate_data(&schema, &data).is_empty(), "{name} example valid");
        let out = dir.path().join(format!("{name}.pdf"));
        let o = render(
            &t.join(format!("{name}.tmpl.html")),
            &t.join(format!("{name}.example.json")),
            &out,
            &[],
        );
        assert!(o.status.success(), "{name}: {}", stderr(&o));
        assert!(std::fs::metadata(&out).unwrap().len() > 1000);
    }
}
