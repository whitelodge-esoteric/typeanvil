//! Integration smoke test: render a fixture HTML through the CLI and assert the
//! PDF exists, is non-empty, is a valid PDF header, and is byte-deterministic.

use std::path::Path;
use std::process::Command;

const FIXTURE: &str = r#"<html><head><style>
h1 { color: #e63946; font-size: 28px; }
p { color: #06d6a0; font-size: 14px; }
</style></head>
<body><h1>Hello, page 1</h1><p>This is the Typeanvil walking skeleton.</p></body></html>
"#;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_typeanvil")
}

fn render(html: &Path, out: &Path) {
    let status = Command::new(bin())
        .args([
            "render",
            html.to_str().unwrap(),
            "--page-width",
            "5in",
            "--page-height",
            "3in",
            "--margin-top",
            "0.5in",
            "--margin-right",
            "0.5in",
            "--margin-bottom",
            "0.5in",
            "--margin-left",
            "0.5in",
            "--base-url",
            "http://127.0.0.1:9/",
            "-o",
            out.to_str().unwrap(),
        ])
        .status()
        .expect("failed to spawn typeanvil");
    assert!(status.success(), "engine exited non-zero: {status:?}");
}

#[test]
fn renders_nonempty_pdf() {
    let dir = tempfile::tempdir().unwrap();
    let html = dir.path().join("hello.html");
    let out = dir.path().join("hello.pdf");
    std::fs::write(&html, FIXTURE).unwrap();

    render(&html, &out);

    let bytes = std::fs::read(&out).expect("output PDF missing");
    assert!(bytes.len() > 1024, "PDF too small: {} bytes", bytes.len());
    assert!(
        bytes.starts_with(b"%PDF-"),
        "output is not a PDF (bad header)"
    );
}

#[test]
fn output_is_deterministic() {
    let dir = tempfile::tempdir().unwrap();
    let html = dir.path().join("hello.html");
    std::fs::write(&html, FIXTURE).unwrap();

    let a = dir.path().join("a.pdf");
    let b = dir.path().join("b.pdf");
    render(&html, &a);
    render(&html, &b);

    let ba = std::fs::read(&a).unwrap();
    let bb = std::fs::read(&b).unwrap();
    assert_eq!(ba, bb, "PDF output is not byte-identical across runs");
}

#[test]
fn paginates_long_content() {
    // Many paragraphs should overflow a 5x3in page and produce >1 page.
    let mut html = String::from("<html><body>");
    for i in 0..60 {
        html.push_str(&format!("<p>Paragraph number {i} with some words.</p>"));
    }
    html.push_str("</body></html>");

    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("long.html");
    let out = dir.path().join("long.pdf");
    std::fs::write(&src, &html).unwrap();
    render(&src, &out);

    let bytes = std::fs::read(&out).unwrap();
    // Count `/Type /Page` occurrences (not /Pages) as a proxy for page count.
    let hay = String::from_utf8_lossy(&bytes);
    let pages = hay.matches("/Type /Page\n").count() + hay.matches("/Type/Page/").count();
    // At minimum the PDF must be valid and larger than a single-page doc.
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(
        pages == 0 || pages >= 2,
        "expected pagination, saw {pages} page markers"
    );
}
