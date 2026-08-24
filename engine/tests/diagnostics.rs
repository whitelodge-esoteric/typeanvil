//! CORE-112: structured machine-readable diagnostics.
//!
//! Acceptance criteria from `docs/specifications/diagnostics.spec.md`:
//! exact event list for a known-bad fixture (asserted as an exact JSON
//! string), source-order determinism, comment/string immunity, and CLI
//! contract notes mirrored in `harness/engine.py`.

use typeanvil::diagnostics::{analyze, to_json};

/// The spec's acceptance fixture: one of each event kind plus supported
/// declarations around them. Line/column coordinates are asserted exactly.
const FIXTURE: &str = r#"/* leading comment mentioning text-indent must not fire */
p {
    color: black;
    text-indent: 12pt;
    display: inline-table;
    widows: 2;
}
@ruby {
    color: red;
}
p {
    widows;
    hyphens: auto;
}
@media print {
    p { orphans: 3; letter-spacing: 1pt; }
}
"#;

#[test]
fn fixture_events_exact_list() {
    let events = analyze(FIXTURE);
    let summary: Vec<(String, usize, usize)> = events
        .iter()
        .map(|d| (d.code.clone(), d.line, d.column))
        .collect();
    assert_eq!(
        summary,
        vec![
            ("unsupported-property".to_string(), 4, 5),
            ("display-fallback".to_string(), 5, 5),
            ("unknown-at-rule".to_string(), 8, 1),
            ("malformed-declaration".to_string(), 12, 5),
            // Inside @media, in source order after the top-level events.
            ("unsupported-property".to_string(), 16, 21),
        ],
        "unexpected diagnostic set: {summary:?}"
    );
}

#[test]
fn fixture_events_exact_json() {
    let events = analyze(FIXTURE);
    assert_eq!(
        to_json(&events),
        r#"{
  "schema": 1,
  "diagnostics": [
    {"code": "unsupported-property", "severity": "warning", "message": "property `text-indent` is not supported and was ignored", "line": 4, "column": 5},
    {"code": "display-fallback", "severity": "warning", "message": "inline-table falls back to block", "line": 5, "column": 5},
    {"code": "unknown-at-rule", "severity": "warning", "message": "at-rule `@ruby` is not supported and was ignored", "line": 8, "column": 1},
    {"code": "malformed-declaration", "severity": "warning", "message": "declaration `widows` has no property-value separator", "line": 12, "column": 5},
    {"code": "unsupported-property", "severity": "warning", "message": "property `letter-spacing` is not supported and was ignored", "line": 16, "column": 21}
  ],
  "counts": {"warnings": 5}
}"#
    );
}

#[test]
fn analysis_is_deterministic() {
    let a = analyze(FIXTURE);
    let b = analyze(FIXTURE);
    assert_eq!(a, b);
    assert_eq!(to_json(&a), to_json(&b));
}

#[test]
fn comments_and_strings_are_ignored() {
    let css = r#"/* text-indent: 12pt; inside a comment */
p { content: "text-indent: 12pt"; color: red; }
"#;
    assert!(
        analyze(css).is_empty(),
        "no events for commented/quoted text"
    );
}

#[test]
fn supported_and_custom_properties_stay_silent() {
    let css = r#"p {
    --my-var: 10px;
    -webkit-text-stroke: 1px black;
    break-before: page;
    margin-top: 6pt;
}
@page {
    size: 360pt 216pt;
    margin: 36pt;
    @top-center { content: "Page " counter(page); }
}
@font-face {
    font-family: Custom;
    src: url(custom.ttf);
}
"#;
    assert!(analyze(css).is_empty(), "supported surface stays silent");
}

#[test]
fn unterminated_input_does_not_panic() {
    analyze("p { color: red;");
    analyze("/* never closed");
    analyze("@media print { p { orphans: 2; }");
    analyze("p { color");
}

#[test]
fn empty_and_clean_stylesheets_emit_empty_document() {
    let json = to_json(&analyze(""));
    assert_eq!(
        json,
        "{\n  \"schema\": 1,\n  \"diagnostics\": [],\n  \"counts\": {\"warnings\": 0}\n}"
    );
}
