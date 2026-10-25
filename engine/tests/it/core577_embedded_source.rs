// SPDX-License-Identifier: AGPL-3.0-only

//! CORE-577: machine-readable sidecar — `--embed-source` / `--embed-data`.
//! Maps to `docs/specifications/pdf-embedded-source.spec.md` AC-2..AC-7.
//! AC-1 (pypdfium2 round trip) lives in `tests/test_embedded_source.py`;
//! the Rust round trip here inflates the `/EmbeddedFile` stream directly.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Output};

const SRC: &str = "# Sidecar\n\nNon-ASCII — “quotes”, ü, 漢字 and `code`.\n";

fn render(input: &Path, extra: &[&str], out: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_typeanvil"))
        .arg("render")
        .arg(input)
        .args(["--page-width", "5in", "--page-height", "3in"])
        .args(extra)
        .arg("-o")
        .arg(out)
        .output()
        .expect("spawn typeanvil")
}

fn setup() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let md = dir.path().join("doc.md");
    std::fs::write(&md, SRC).unwrap();
    (dir, md)
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Inflate every `/Type /EmbeddedFile` stream in the PDF.
fn embedded_streams(pdf: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(i) = find(&pdf[pos..], b"/EmbeddedFile") {
        let at = pos + i;
        pos = at + 1;
        // Skip the name-tree key `/EmbeddedFiles`.
        if pdf.get(at + 13) == Some(&b's') {
            continue;
        }
        let s = find(&pdf[at..], b"stream").unwrap() + at + 6;
        let s = if pdf[s] == b'\r' { s + 2 } else { s + 1 };
        let e = find(&pdf[s..], b"endstream").unwrap() + s;
        let mut z = flate2::read::ZlibDecoder::new(&pdf[s..e]);
        let mut buf = Vec::new();
        z.read_to_end(&mut buf).expect("inflate embedded file");
        out.push(buf);
    }
    out
}

#[test]
fn embed_source_round_trips_with_source_relationship() {
    let (dir, md) = setup();
    let out = dir.path().join("o.pdf");
    let o = render(&md, &["--embed-source"], &out);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let pdf = std::fs::read(&out).unwrap();
    assert!(pdf.starts_with(b"%PDF-2.0"), "attachments switch to PDF 2.0");
    assert_eq!(embedded_streams(&pdf), vec![SRC.as_bytes().to_vec()]);
    assert!(find(&pdf, b"/AFRelationship/Source").is_some());
    assert!(find(&pdf, b"/AF[").is_some(), "catalog /AF array");
    assert!(find(&pdf, b"(doc.md)").is_some(), "attachment named by base name");
    assert!(find(&pdf, b"/text#2Fmarkdown").is_some(), "Markdown MIME subtype");
}

#[test]
fn embed_source_is_deterministic_and_dateless() {
    let (dir, md) = setup();
    let a = dir.path().join("a.pdf");
    let b = dir.path().join("b.pdf");
    assert!(render(&md, &["--embed-source"], &a).status.success());
    assert!(render(&md, &["--embed-source"], &b).status.success());
    let (a, b) = (std::fs::read(a).unwrap(), std::fs::read(b).unwrap());
    assert_eq!(a, b);
    assert!(find(&a, b"/ModDate").is_none());
    assert!(find(&a, b"/CreationDate").is_none());
}

#[test]
fn embed_data_attaches_json_with_data_relationship() {
    let (dir, md) = setup();
    let json = dir.path().join("invoice.json");
    let data = b"{\"total\": 42.5, \"currency\": \"EUR\"}\n";
    std::fs::write(&json, data).unwrap();
    let out = dir.path().join("o.pdf");
    let o = render(&md, &["--embed-data", json.to_str().unwrap()], &out);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let pdf = std::fs::read(&out).unwrap();
    assert_eq!(embedded_streams(&pdf), vec![data.to_vec()]);
    assert!(find(&pdf, b"/AFRelationship/Data").is_some());
    assert!(find(&pdf, b"/application#2Fjson").is_some());
}

#[test]
fn default_render_has_no_attachments() {
    let (dir, md) = setup();
    let out = dir.path().join("o.pdf");
    assert!(render(&md, &[], &out).status.success());
    let pdf = std::fs::read(&out).unwrap();
    assert!(pdf.starts_with(b"%PDF-1.7"));
    assert!(find(&pdf, b"/EmbeddedFiles").is_none());
}

#[test]
fn ua_still_embeds_the_source() {
    let (dir, md) = setup();
    let out = dir.path().join("o.pdf");
    let o = render(&md, &["--ua", "--title", "T", "--embed-source"], &out);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let pdf = std::fs::read(&out).unwrap();
    assert_eq!(embedded_streams(&pdf), vec![SRC.as_bytes().to_vec()]);
}

#[test]
fn duplicate_attachment_name_fails() {
    let (dir, md) = setup();
    let out = dir.path().join("o.pdf");
    let o = render(&md, &["--embed-source", "--embed-data", md.to_str().unwrap()], &out);
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("duplicate embedded file name"));
}
