//! PDF document metadata acceptance tests — one per acceptance criterion in
//! `docs/specifications/pdf-metadata.spec.md` (CORE-105).
//!
//! Byte-presence assertions check the rendered PDF for the Info-dict keys
//! krilla writes (`/Title`, `/Author`, `/Subject`, `/Keywords`). ASCII
//! metadata values serialize as literal parenthesized strings, so the
//! plain-text title also appears verbatim.

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::{layout, Layout};
use typeanvil::metadata::extract_metadata;
use typeanvil::pdf::{render, render_with_metadata, DocumentMetadata};

// --- helpers (mirror tests/fonts.rs) ----------------------------------------

fn inches(v: f64) -> Scalar {
    Scalar(v * 72.0)
}

fn geometry(w_in: f64, h_in: f64, margin_in: f64) -> PageGeometry {
    PageGeometry {
        width: inches(w_in),
        height: inches(h_in),
        margin_top: inches(margin_in),
        margin_right: inches(margin_in),
        margin_bottom: inches(margin_in),
        margin_left: inches(margin_in),
    }
}

fn stylesheet_of(dom: &Dom) -> Stylesheet {
    let mut css = String::new();
    for (id, node) in dom.nodes.iter().enumerate() {
        if let NodeKind::Element(el) = &node.kind {
            if el.tag == "style" {
                css.push_str(&dom.text_content(id));
                css.push('\n');
            }
        }
    }
    Stylesheet::parse(&css)
}

fn lay(html: &str) -> Layout {
    let dom = Dom::parse(html).unwrap();
    let ss = stylesheet_of(&dom);
    layout(&dom, &ss, geometry(8.5, 11.0, 0.8))
}

fn pdf_bytes_with_metadata(html: &str, meta: &DocumentMetadata) -> Vec<u8> {
    let l = lay(html);
    render_with_metadata(&l, meta).unwrap()
}

fn contains(bytes: &[u8], needle: &[u8]) -> bool {
    bytes.windows(needle.len()).any(|w| w == needle)
}

/// A document declaring all four metadata fields.
const FULL_METADATA_HTML: &str = r#"<!DOCTYPE html><html><head><style>
p { font-family: Arial; font-size: 12pt; margin: 0; }
</style>
<title>Quarterly Report</title>
<meta name="author" content="A. Author">
<meta name="description" content="Q3 results summary">
<meta name="keywords" content="finance, quarterly, Q3">
</head><body>
<p>The quick brown fox jumps over the lazy dog.</p>
</body></html>"#;

// --- AC 1: determinism --------------------------------------------------------

/// Rendering the same input twice with full metadata yields byte-identical
/// PDFs — no `creation_date` or other wall-clock-derived bytes enter output.
#[test]
fn metadata_render_is_deterministic() {
    let dom = Dom::parse(FULL_METADATA_HTML).unwrap();
    let meta = extract_metadata(&dom, None, None);
    let b1 = pdf_bytes_with_metadata(FULL_METADATA_HTML, &meta);
    let b2 = pdf_bytes_with_metadata(FULL_METADATA_HTML, &meta);
    assert_eq!(
        b1, b2,
        "identical input with full metadata must yield byte-identical PDF"
    );
}

// --- AC 2: title lands in Document Properties ---------------------------------

#[test]
fn title_lands_in_document_properties() {
    // Title-only document: /Title carries the trimmed <title> text.
    let html = r#"<!DOCTYPE html><html><head><style>
p { font-family: Arial; font-size: 12pt; margin: 0; }
</style><title>Quarterly Report</title></head><body>
<p>The quick brown fox jumps over the lazy dog.</p>
</body></html>"#;
    let dom = Dom::parse(html).unwrap();
    let meta = extract_metadata(&dom, None, None);
    assert_eq!(meta.title.as_deref(), Some("Quarterly Report"));
    let bytes = pdf_bytes_with_metadata(html, &meta);
    assert!(contains(&bytes, b"/Title"), "/Title key missing from PDF");
    assert!(
        contains(&bytes, b"(Quarterly Report)"),
        "encoded title text missing from PDF"
    );

    // With other fields set, their Info-dict keys appear too.
    let dom = Dom::parse(FULL_METADATA_HTML).unwrap();
    let meta = extract_metadata(&dom, None, None);
    let bytes = pdf_bytes_with_metadata(FULL_METADATA_HTML, &meta);
    for key in [b"/Title".as_slice(), b"/Author", b"/Subject", b"/Keywords"] {
        assert!(contains(&bytes, key), "Info dict missing {key:?}");
    }
    assert!(
        contains(&bytes, b"(A. Author)"),
        "encoded author text missing from PDF"
    );
}

// --- AC 3: absence stays absence ----------------------------------------------

/// A document declaring no metadata and no CLI overrides produces output with
/// NO Info-dict metadata keys — current behavior, byte-for-byte.
#[test]
fn no_metadata_when_document_declares_none() {
    let html = r#"<!DOCTYPE html><html><head><style>
p { font-family: Arial; font-size: 12pt; margin: 0; }
</style></head><body>
<p>The quick brown fox jumps over the lazy dog.</p>
</body></html>"#;
    let dom = Dom::parse(html).unwrap();
    let meta = extract_metadata(&dom, None, None);
    assert_eq!(
        meta,
        DocumentMetadata::default(),
        "a metadata-free document must extract to the empty default"
    );
    let bytes = pdf_bytes_with_metadata(html, &meta);
    for key in [b"/Title".as_slice(), b"/Author", b"/Subject", b"/Keywords"] {
        assert!(
            !contains(&bytes, key),
            "unexpected Info-dict key {key:?} in metadata-free render"
        );
    }
    // The plain `render` path (no metadata at all) matches.
    let l = lay(html);
    let bytes = render(&l).unwrap();
    for key in [b"/Title".as_slice(), b"/Author", b"/Subject", b"/Keywords"] {
        assert!(
            !contains(&bytes, key),
            "unexpected Info-dict key {key:?} in plain render"
        );
    }
}

// --- AC 4: CLI overrides replace document values --------------------------------

#[test]
fn cli_overrides_replace_document_values() {
    let html = r#"<!DOCTYPE html><html><head><style>
p { font-family: Arial; font-size: 12pt; margin: 0; }
</style>
<title>Doc Title</title>
<meta name="author" content="Doc Author">
<meta name="description" content="Doc Subject">
<meta name="keywords" content="a, b, , c,,">
</head><body>
<p>The quick brown fox jumps over the lazy dog.</p>
</body></html>"#;
    let dom = Dom::parse(html).unwrap();

    // No overrides: document values pass through; empty keyword entries drop.
    let meta = extract_metadata(&dom, None, None);
    assert_eq!(meta.title.as_deref(), Some("Doc Title"));
    assert_eq!(meta.authors, vec!["Doc Author".to_string()]);
    assert_eq!(meta.subject.as_deref(), Some("Doc Subject"));
    assert_eq!(
        meta.keywords,
        vec!["a".to_string(), "b".to_string(), "c".to_string()]
    );

    // Overrides win for title and author; other fields stay document-derived.
    let meta = extract_metadata(
        &dom,
        Some("Cli Title".to_string()),
        Some("Cli Author".to_string()),
    );
    assert_eq!(meta.title.as_deref(), Some("Cli Title"));
    assert_eq!(meta.authors, vec!["Cli Author".to_string()]);
    assert_eq!(meta.subject.as_deref(), Some("Doc Subject"));
    assert_eq!(meta.keywords, vec!["a".to_string(), "b".to_string(), "c".to_string()]);

    // Empty overrides fall back to the document values.
    let meta = extract_metadata(&dom, Some(String::new()), Some(String::new()));
    assert_eq!(meta.title.as_deref(), Some("Doc Title"));
    assert_eq!(meta.authors, vec!["Doc Author".to_string()]);
}

// --- Spec edge cases ------------------------------------------------------------

/// Empty <title> is treated as absent; the first <title> wins; meta names
/// match exactly (lowercase), and overrides win over an empty document title.
#[test]
fn extract_metadata_edge_cases() {
    // Empty title → None; whitespace-trimmed title lands.
    let html = r#"<!DOCTYPE html><html><head><title>  </title>
<title>Second</title></head><body><p>x</p></body></html>"#;
    let dom = Dom::parse(html).unwrap();
    let meta = extract_metadata(&dom, None, None);
    assert_eq!(meta.title, None, "empty <title> must extract to None");

    let html = r#"<!DOCTYPE html><html><head><title> First </title>
<title>Second</title></head><body><p>x</p></body></html>"#;
    let dom = Dom::parse(html).unwrap();
    let meta = extract_metadata(&dom, None, None);
    assert_eq!(
        meta.title.as_deref(),
        Some("First"),
        "first <title> wins and is trimmed"
    );

    // Case-variant meta names do not match (exact lowercase match).
    let html = r#"<!DOCTYPE html><html><head>
<meta name="Author" content="No Match"></head><body><p>x</p></body></html>"#;
    let dom = Dom::parse(html).unwrap();
    let meta = extract_metadata(&dom, None, None);
    assert!(meta.authors.is_empty(), "case-variant author must not match");
}
