// SPDX-License-Identifier: AGPL-3.0-only

//! CORE-578: fetch policy and resource limits for untrusted input.
//!
//! Maps to the Acceptance Criteria of
//! `docs/specifications/untrusted-input-safety.spec.md`.

use std::path::Path;
use std::process::{Command, Output};

fn render(input: &Path, extra: &[&str], out: &Path) -> Output {
    let mut args: Vec<String> = vec![
        "render".into(),
        input.to_string_lossy().into_owned(),
        "--page-width".into(),
        "5in".into(),
        "--page-height".into(),
        "3in".into(),
        "-o".into(),
        out.to_string_lossy().into_owned(),
    ];
    args.extend(extra.iter().map(|s| s.to_string()));
    Command::new(env!("CARGO_BIN_EXE_typeanvil"))
        .args(&args)
        .output()
        .expect("spawn typeanvil")
}

fn page_count(pdf: &[u8]) -> usize {
    let s = String::from_utf8_lossy(pdf);
    s.split("/Type /Page")
        .skip(1)
        .filter(|rest| !rest.starts_with('s'))
        .count()
        .max(
            s.split("/Type/Page")
                .skip(1)
                .filter(|rest| !rest.starts_with('s'))
                .count(),
        )
}

/// A tree: `<tmp>/secret.html` (outside) and `<tmp>/doc/` (the root).
fn tree(body: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("secret.html"), "<p>TOPSECRET</p>").unwrap();
    std::fs::write(dir.path().join("secret.css"), "p { color: red }").unwrap();
    let doc = dir.path().join("doc");
    std::fs::create_dir_all(&doc).unwrap();
    let input = doc.join("in.html");
    std::fs::write(&input, format!("<html><body>{body}</body></html>")).unwrap();
    (dir, input)
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// AC1: `--fetch local` refuses a `..` escape and reports it.
#[test]
fn path_escape_is_blocked_and_reported() {
    let (dir, input) = tree(r#"<p>hi</p><iframe src="../secret.html"></iframe><link rel="stylesheet" href="../secret.css">"#);
    let out = dir.path().join("o.pdf");
    let o = render(&input, &["--fetch", "local", "--diagnostics", "json"], &out);
    assert!(o.status.success(), "{}", stderr(&o));
    let err = stderr(&o);
    assert!(err.contains("[fetch-blocked]"), "stderr: {err}");
    assert!(err.contains("../secret.html"), "stderr: {err}");
    assert!(err.contains("../secret.css"), "stderr: {err}");
    let json = String::from_utf8_lossy(&o.stdout);
    assert!(json.contains("\"fetch-blocked\""), "diagnostics: {json}");
}

/// AC1: a symlink inside the root that points outside is refused.
#[cfg(unix)]
#[test]
fn symlink_escape_is_blocked() {
    let (dir, input) = tree(r#"<iframe src="link.html"></iframe>"#);
    std::os::unix::fs::symlink(dir.path().join("secret.html"), input.parent().unwrap().join("link.html"))
        .unwrap();
    let out = dir.path().join("o.pdf");
    let o = render(&input, &["--fetch", "local"], &out);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stderr(&o).contains("[fetch-blocked]"), "{}", stderr(&o));
}

/// AC2: a remote URL under `--fetch local` / `none` is reported blocked;
/// an allow-listed host is not.
#[test]
fn remote_url_is_blocked_and_reported() {
    let (dir, input) = tree(r#"<img src="https://evil.example/x.png"><img src="file:///etc/passwd">"#);
    let out = dir.path().join("o.pdf");
    for mode in ["local", "none"] {
        let o = render(&input, &["--fetch", mode], &out);
        assert!(o.status.success(), "{}", stderr(&o));
        let err = stderr(&o);
        assert!(err.contains("https://evil.example/x.png") && err.contains("[fetch-blocked]"), "{mode}: {err}");
        assert!(err.contains("file:///etc/passwd"), "{mode}: {err}");
    }
    let o = render(&input, &["--fetch", "allow-list", "--fetch-hosts", "evil.example"], &out);
    let err = stderr(&o);
    assert!(!err.contains("https://evil.example/x.png"), "allow-listed host reported: {err}");
}

/// AC2: `--fetch none` blocks even a file inside the root.
#[test]
fn fetch_none_blocks_local_reads() {
    let (dir, input) = tree(r#"<iframe src="inner.html"></iframe>"#);
    std::fs::write(input.parent().unwrap().join("inner.html"), "<p>x</p>").unwrap();
    let out = dir.path().join("o.pdf");
    let o = render(&input, &["--fetch", "local"], &out);
    assert!(!stderr(&o).contains("fetch-blocked"), "{}", stderr(&o));
    let o = render(&input, &["--fetch", "none"], &out);
    assert!(stderr(&o).contains("[fetch-blocked]"), "{}", stderr(&o));
}

/// AC3: `--max-pages` stops with LIMIT_PAGE_COUNT, exit 4, no PDF.
#[test]
fn page_limit_yields_error_code() {
    let body: String = (0..5).map(|i| format!(r#"<div style="break-after:page">{i}</div>"#)).collect();
    let (dir, input) = tree(&body);
    let out = dir.path().join("o.pdf");
    let o = render(&input, &["--max-pages", "5"], &out);
    assert!(o.status.success(), "5 pages fit in 5: {}", stderr(&o));
    assert_eq!(page_count(&std::fs::read(&out).unwrap()), 5);
    std::fs::remove_file(&out).unwrap();
    let o = render(&input, &["--max-pages", "2"], &out);
    assert_eq!(o.status.code(), Some(4), "{}", stderr(&o));
    assert!(stderr(&o).contains("[LIMIT_PAGE_COUNT]"), "{}", stderr(&o));
    assert!(!out.exists(), "a stopped render writes no PDF");
}

/// AC4: `--timeout` stops with LIMIT_TIMEOUT, exit 4, no PDF. A tiny
/// budget on a many-page document trips at the first page check.
#[test]
fn timeout_yields_error_code() {
    let body: String = (0..400).map(|i| format!(r#"<div style="break-after:page">{i}</div>"#)).collect();
    let (dir, input) = tree(&body);
    let out = dir.path().join("o.pdf");
    let o = render(&input, &["--timeout", "0.000001"], &out);
    assert_eq!(o.status.code(), Some(4), "{}", stderr(&o));
    assert!(stderr(&o).contains("[LIMIT_TIMEOUT]"), "{}", stderr(&o));
    assert!(!out.exists(), "a timeout never writes a PDF");
}

/// AC5: `--max-input-bytes` stops with LIMIT_INPUT_SIZE.
#[test]
fn input_size_limit_yields_error_code() {
    let (dir, input) = tree("<p>hello world</p>");
    let out = dir.path().join("o.pdf");
    let o = render(&input, &["--max-input-bytes", "10"], &out);
    assert_eq!(o.status.code(), Some(4), "{}", stderr(&o));
    assert!(stderr(&o).contains("[LIMIT_INPUT_SIZE]"), "{}", stderr(&o));
}

/// AC5: `--max-image-pixels` stops with LIMIT_IMAGE_SIZE before decode.
#[test]
fn image_size_limit_yields_error_code() {
    // A 1x1 PNG passes a 1-pixel budget; a 10x10 SVG (100 px) trips a
    // 99-pixel budget before rasterization.
    let png_1x1 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
    let (dir, input) = tree(&format!(r#"<img src="data:image/png;base64,{png_1x1}">"#));
    let out = dir.path().join("o.pdf");
    let o = render(&input, &["--max-image-pixels", "1"], &out);
    assert!(o.status.success(), "{}", stderr(&o));
    let (dir, input) = tree(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10"/></svg>"#,
    );
    let out = dir.path().join("o.pdf");
    let o = render(&input, &["--max-image-pixels", "100"], &out);
    assert!(o.status.success(), "{}", stderr(&o));
    let o = render(&input, &["--max-image-pixels", "99"], &out);
    assert_eq!(o.status.code(), Some(4), "{}", stderr(&o));
    assert!(stderr(&o).contains("[LIMIT_IMAGE_SIZE]"), "{}", stderr(&o));
}

/// AC6: without a safety flag the render is byte-identical to a render
/// with `--fetch unrestricted`, and a generous limit changes nothing.
#[test]
fn no_flag_is_unchanged() {
    let (dir, input) = tree(r#"<p>hi</p><iframe src="../secret.html"></iframe>"#);
    let a = dir.path().join("a.pdf");
    let b = dir.path().join("b.pdf");
    let c = dir.path().join("c.pdf");
    assert!(render(&input, &[], &a).status.success());
    assert!(render(&input, &["--fetch", "unrestricted"], &b).status.success());
    let o = render(&input, &["--max-pages", "1000", "--timeout", "600"], &c);
    assert!(o.status.success());
    assert!(o.stderr.is_empty(), "{}", stderr(&o));
    let a = std::fs::read(a).unwrap();
    assert_eq!(a, std::fs::read(b).unwrap());
    assert_eq!(a, std::fs::read(c).unwrap());
}

/// AC7: bad flag values are usage errors (exit 2).
#[test]
fn bad_values_are_usage_errors() {
    let (dir, input) = tree("<p>x</p>");
    let out = dir.path().join("o.pdf");
    for args in [
        &["--fetch", "bogus"][..],
        &["--fetch", "allow-list"][..],
        &["--fetch-hosts", "a.com"][..],
        &["--max-pages", "0"][..],
        &["--timeout", "-1"][..],
    ] {
        let o = render(&input, args, &out);
        assert_eq!(o.status.code(), Some(2), "{args:?}: {}", stderr(&o));
    }
}
