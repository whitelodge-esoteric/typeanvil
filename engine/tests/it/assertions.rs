//! CORE-573: intent assertions — layout goals with a pass/fail verdict.
//!
//! Spec: `docs/specifications/intent-assertions.spec.md`. Each assertion
//! kind has a passing and a failing fixture; the CLI tests cover the exit
//! status contract and determinism.

use std::path::PathBuf;
use std::process::Command;

use typeanvil::assertions::{evaluate, parse, Outcome};
use typeanvil::css::Stylesheet;
use typeanvil::dom::Dom;
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::layout;
use typeanvil::render_report::analyze_layout;

/// 5in x 3in, 0.5in margins: content area 288 x 144pt.
fn geo() -> PageGeometry {
    PageGeometry {
        width: Scalar(360.0),
        height: Scalar(216.0),
        margin_top: Scalar(36.0),
        margin_right: Scalar(36.0),
        margin_bottom: Scalar(36.0),
        margin_left: Scalar(36.0),
    }
}

const BASE: &str = "html,body{margin:0} p{margin:0;font-size:12pt;line-height:14pt} \
                    h1,h2{margin:0;font-size:12pt;line-height:14pt}";

/// Evaluate one assertion (JSON object text) against a document.
fn check(css: &str, body: &str, assertion: &str) -> Outcome {
    let html = format!("<!DOCTYPE html>\n<body>\n{body}\n</body>");
    let dom = Dom::parse(&html).expect("parse");
    let sheet = Stylesheet::parse(&format!("{BASE} {css}"));
    let l = layout(&dom, &sheet, geo());
    let report = analyze_layout(&dom, &sheet, &l);
    let list = parse(&format!("[{assertion}]")).expect("assertion parses");
    let mut out = evaluate(&dom, &sheet, &l, &report, &list);
    out.pop().expect("one outcome")
}

/// Three pages: two forced breaks.
const THREE_PAGES: &str = "<p>a</p>\n<p style=\"break-before:page\">b</p>\n<p style=\"break-before:page\">c</p>";

#[test]
fn max_pages() {
    let ok = check("", THREE_PAGES, r#"{"kind":"max-pages","value":3}"#);
    assert!(ok.passed, "{ok:?}");
    let bad = check("", THREE_PAGES, r#"{"kind":"max-pages","value":2}"#);
    assert!(!bad.passed);
    assert_eq!(bad.detail, "3 pages (max 2)");
}

#[test]
fn exact_pages() {
    assert!(check("", THREE_PAGES, r#"{"kind":"exact-pages","value":3}"#).passed);
    let bad = check("", "<p>a</p>", r#"{"kind":"exact-pages","value":2}"#);
    assert!(!bad.passed);
    assert_eq!(bad.detail, "1 pages (expected 2)");
}

#[test]
fn no_overflow() {
    assert!(check("", "<p>fits</p>", r#"{"kind":"no-overflow"}"#).passed);
    let bad = check(
        ".wide{width:500pt;height:20pt}",
        "<p>intro</p>\n<div id=\"w\" class=\"wide\">x</div>",
        r#"{"kind":"no-overflow"}"#,
    );
    assert!(!bad.passed, "{bad:?}");
    assert_eq!(bad.evidence.len(), 1);
    let e = &bad.evidence[0];
    assert_eq!((e.page, e.line, e.column), (1, 4, 1));
    assert_eq!(e.element, "div#w.wide");
    assert_eq!(e.rect.expect("rect")[2], 500.0);
}

#[test]
fn no_empty_pages() {
    assert!(check("", "<p>a</p>", r#"{"kind":"no-empty-pages"}"#).passed);
    let bad = check(
        ".b{break-before:page} .t{height:144pt}",
        "<div class=\"t\"></div>\n<p class=\"b\">after</p>",
        r#"{"kind":"no-empty-pages"}"#,
    );
    assert!(!bad.passed, "{bad:?}");
    assert_eq!(bad.evidence[0].page, 1);
}

#[test]
fn keep_together() {
    // A 100pt card after an 80pt spacer: with break-inside:avoid it moves
    // whole to page 2; without, it splits across pages 1 and 2.
    let body = "<div style=\"height:80pt\"></div>\n<div class=\"card\"><p>1</p><p>2</p><p>3</p><p>4</p><p>5</p><p>6</p><p>7</p></div>";
    let ok = check(".card{break-inside:avoid}", body, r#"{"kind":"keep-together","selector":".card"}"#);
    assert!(ok.passed, "{ok:?}");
    let bad = check("", body, r#"{"kind":"keep-together","selector":".card"}"#);
    assert!(!bad.passed, "{bad:?}");
    let pages: Vec<usize> = bad.evidence.iter().map(|e| e.page).collect();
    assert_eq!(pages, vec![1, 2]);
    assert!(bad.evidence.iter().all(|e| e.element == "div.card" && e.line == 4));
}

#[test]
fn on_page() {
    let ok = check("", THREE_PAGES, r#"{"kind":"on-page","selector":"p:last-child","page":3}"#);
    assert!(ok.passed, "{ok:?}");
    let bad = check("", THREE_PAGES, r#"{"kind":"on-page","selector":"p","page":1}"#);
    assert!(!bad.passed);
    assert_eq!(bad.detail, "2 of 3 matched elements do not start on page 1");
    let pages: Vec<usize> = bad.evidence.iter().map(|e| e.page).collect();
    assert_eq!(pages, vec![2, 3]);
    // An element with no box fails with page 0 and no rect.
    let hidden = check("", "<p>a</p><p id=\"h\" style=\"display:none\">b</p>", r##"{"kind":"on-page","selector":"#h","page":1}"##);
    assert!(!hidden.passed);
    assert_eq!((hidden.evidence[0].page, hidden.evidence[0].rect), (0, None));
}

#[test]
fn outline_covers() {
    let body = "<h1>Title</h1>\n<h2 class=\"quiet\">Sub</h2>\n<p>x</p>";
    let ok = check("", body, r#"{"kind":"outline-covers","selector":"h1, h2"}"#);
    assert!(ok.passed, "{ok:?}");
    let bad = check(".quiet{bookmark-level:none}", body, r#"{"kind":"outline-covers","selector":"h1,h2"}"#);
    assert!(!bad.passed, "{bad:?}");
    assert_eq!(bad.evidence.len(), 1);
    assert_eq!(bad.evidence[0].element, "h2.quiet");
    assert_eq!(bad.evidence[0].line, 4);
}

#[test]
fn fonts_resolved() {
    assert!(check("", "<p>a</p>", r#"{"kind":"fonts-resolved"}"#).passed);
    let bad = check(
        ".x{font-family:\"Nonexistent Family CORE573\"}",
        "<p class=\"x\">a</p>",
        r#"{"kind":"fonts-resolved"}"#,
    );
    assert!(!bad.passed, "{bad:?}");
    assert_eq!(bad.evidence[0].element, "p.x");
}

#[test]
fn selector_edge_cases() {
    let invalid = check("", "<p>a</p>", r#"{"kind":"keep-together","selector":"p[[["}"#);
    assert!(!invalid.passed);
    assert!(invalid.detail.starts_with("invalid selector"), "{}", invalid.detail);
    let none = check("", "<p>a</p>", r#"{"kind":"keep-together","selector":"table"}"#);
    assert!(!none.passed);
    assert!(none.detail.contains("matches no element"), "{}", none.detail);
}

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("typeanvil-assert-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Run `render` on THREE_PAGES with extra args; returns (status, stdout,
/// stderr, pdf exists).
fn run(name: &str, assertions: Option<&str>, extra: &[&str]) -> (Option<i32>, String, String, bool) {
    let d = scratch(name);
    let html = d.join("in.html");
    std::fs::write(&html, format!("<!DOCTYPE html>\n<style>{BASE}</style>\n<body>\n{THREE_PAGES}\n</body>")).unwrap();
    let pdf = d.join("out.pdf");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_typeanvil"));
    cmd.arg("render").arg(&html).args(["--page-width", "5in", "--page-height", "3in"]);
    cmd.args(["--margin-top", "0.5in", "--margin-right", "0.5in", "--margin-bottom", "0.5in", "--margin-left", "0.5in"]);
    if let Some(a) = assertions {
        let f = d.join("a.json");
        std::fs::write(&f, a).unwrap();
        cmd.arg("--assert").arg(&f);
    }
    cmd.args(extra).arg("-o").arg(&pdf);
    let out = cmd.output().expect("run typeanvil");
    let exists = pdf.exists();
    let _ = std::fs::remove_dir_all(&d);
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        exists,
    )
}

const PASSING: &str = r#"[{"kind":"max-pages","value":3,"id":"fit"}]"#;
const FAILING: &str = r#"[{"kind":"max-pages","value":3},{"kind":"exact-pages","value":2,"id":"two"}]"#;

#[test]
fn cli_exit_codes() {
    // Failing assertions without the opt-in flag: exit 0, verdict reported.
    let (code, out, _, pdf) = run("soft", Some(FAILING), &[]);
    assert_eq!(code, Some(0));
    assert!(pdf);
    assert!(out.contains("\"assertions\": {\"passed\": 1, \"failed\": 1, \"results\": ["), "{out}");
    // With --fail-on-assert: exit 3, PDF still written, stderr names count.
    let (code, out, err, pdf) = run("hard", Some(FAILING), &["--fail-on-assert"]);
    assert_eq!(code, Some(3), "{err}");
    assert!(pdf);
    assert!(err.contains("typeanvil: 1 assertion(s) failed"), "{err}");
    assert!(out.contains("{\"index\": 1, \"id\": \"two\", \"kind\": \"exact-pages\", \"passed\": false, \"detail\": \"3 pages (expected 2)\", \"evidence\": []}"), "{out}");
    // Passing assertions with --fail-on-assert: exit 0.
    let (code, _, _, _) = run("pass", Some(PASSING), &["--fail-on-assert"]);
    assert_eq!(code, Some(0));
    // --fail-on-assert alone is a usage error.
    let (code, _, err, pdf) = run("alone", None, &["--fail-on-assert"]);
    assert_eq!(code, Some(2), "{err}");
    assert!(!pdf);
    // Text mode prints one line per assertion on stderr.
    let (code, _, err, _) = run("text", Some(FAILING), &["--diagnostics", "text"]);
    assert_eq!(code, Some(0));
    assert!(err.contains("assert[0] PASS max-pages: 3 pages (max 3)"), "{err}");
    assert!(err.contains("assert[1] FAIL exact-pages: 3 pages (expected 2)"), "{err}");
}

#[test]
fn cli_rejects_bad_file() {
    for bad in ["[{\"kind\":\"fit-it\"}]", "[{\"kind\":\"on-page\",\"page\":1}]", "[{", "{}"] {
        let (code, _, err, pdf) = run("bad", Some(bad), &[]);
        assert_eq!(code, Some(2), "{bad}: {err}");
        assert!(!pdf, "{bad}: no PDF on a usage error");
        assert!(err.contains("--assert"), "{err}");
    }
}

#[test]
fn cli_output_is_deterministic() {
    let a = run("det-a", Some(FAILING), &[]).1;
    let b = run("det-b", Some(FAILING), &[]).1;
    assert_eq!(a, b);
    // `assertions` sits between `counts` and `summary`.
    let c = a.find("\"counts\"").unwrap();
    let s = a.find("\"assertions\"").unwrap();
    let m = a.find("\"summary\"").unwrap();
    assert!(c < s && s < m, "{a}");
    // Without --assert the document has no assertions key.
    let plain = run("det-plain", None, &["--diagnostics", "json"]).1;
    assert!(plain.starts_with("{\n  \"schema\": 2,"), "{plain}");
    assert!(!plain.contains("\"assertions\""), "{plain}");
}
