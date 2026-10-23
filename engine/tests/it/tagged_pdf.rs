//! Tagged PDF / structure tree (CORE-111).
//!
//! Contract under test:
//! - untagged path byte-stability vs `render_with_metadata`
//! - tagged renders succeed and are deterministic
//! - role mapping is pure and correct
//! - UA-1 validator rejects an incomplete document and accepts a complete one
//!
//! Identifier note: krilla's `Identifier` has no public constructor, so the
//! tree builder is exercised end-to-end through `render_with_options` (which
//! collects real identifiers from tagged surfaces) rather than by synthesizing
//! draws here.

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, Element};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout;
use typeanvil::pdf::{render_with_metadata, render_with_options, DocumentMetadata};
use typeanvil::tags::role_for;

fn geometry() -> PageGeometry {
    PageGeometry {
        width: Scalar(5.0 * 72.0),
        height: Scalar(3.0 * 72.0),
        margin_top: Scalar(36.0),
        margin_right: Scalar(36.0),
        margin_bottom: Scalar(36.0),
        margin_left: Scalar(36.0),
    }
}

const DOC: &str = r#"<html lang="en"><head><title>T</title></head><body>
<h1>Heading</h1>
<p>First paragraph of body text long enough to shape into at least one line.</p>
<ul><li>One</li><li>Two</li></ul>
<table><thead><tr><th scope="col">A</th></tr></thead>
<tbody><tr><td>1</td></tr></tbody></table>
</body></html>"#;

fn laid_out(html: &str) -> (Dom, typeanvil::layout::Layout) {
    let dom = Dom::parse(html).unwrap();
    let stylesheet = Stylesheet::parse("");
    let layout = layout::layout(&dom, &stylesheet, geometry());
    (dom, layout)
}

/// AC-1: the default (`tagged: false`) path must produce bytes identical to
/// the pre-existing `render_with_metadata` entry point.
#[test]
fn untagged_path_is_byte_stable() {
    let (_dom, layout) = laid_out(DOC);
    let meta = DocumentMetadata::default();
    let old = render_with_metadata(&layout, &meta).unwrap();
    let new = render_with_options(&layout, None, &meta, false, false).unwrap();
    assert_eq!(old, new);
}

/// AC-5: tagged rendering is deterministic (identical input → identical bytes).
#[test]
fn tagged_render_is_deterministic() {
    let (dom, layout) = laid_out(DOC);
    let meta = DocumentMetadata {
        title: Some("T".into()),
        ..DocumentMetadata::default()
    };
    let a = render_with_options(&layout, Some(&dom), &meta, true, false).unwrap();
    let b = render_with_options(&layout, Some(&dom), &meta, true, false).unwrap();
    assert_eq!(a, b);
    // Tagged output differs from untagged (structure objects exist).
    let plain = render_with_metadata(&layout, &meta).unwrap();
    assert_ne!(a, plain);
}

/// Role mapping sanity: pure function of tag (+ attrs for scope).
#[test]
fn role_mapping_covers_semantics() {
    fn el(tag: &str, attrs: &[(&str, &str)]) -> Element {
        Element {
            tag: tag.to_string(),
            id: None,
            classes: Vec::new(),
            attrs: attrs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }
    use typeanvil::tags::TagRole::*;
    assert_eq!(role_for(&el("h1", &[])), Heading(1));
    assert_eq!(role_for(&el("p", &[])), Paragraph);
    assert_eq!(role_for(&el("ul", &[])), List);
    assert_eq!(role_for(&el("li", &[])), ListItem);
    assert_eq!(role_for(&el("table", &[])), Table);
    // Attribute-less <th> defaults to Column scope; explicit scopes map.
    assert_eq!(
        role_for(&el("th", &[])),
        TableHeader(krilla::tagging::TableHeaderScope::Column)
    );
    assert_eq!(
        role_for(&el("th", &[("scope", "row")])),
        TableHeader(krilla::tagging::TableHeaderScope::Row)
    );
    assert!(matches!(role_for(&el("img", &[])), Figure(_)));
    assert_eq!(role_for(&el("style", &[])), Skipped);
    assert_eq!(role_for(&el("div", &[])), Div);
    assert_eq!(role_for(&el("section", &[])), Section); // CORE-374
}

/// AC-6: the UA validator rejects a document missing required properties
/// (no title/lang) and accepts the same class of document completed.
#[test]
fn ua_validator_gates_conformance() {
    // No <title>, no lang: PDF/UA-1 requires the display doc title + language.
    let bad = "<html><body><p>No metadata here.</p></body></html>";
    let (dom, layout) = laid_out(bad);
    let meta = DocumentMetadata::default();
    let err = render_with_options(&layout, Some(&dom), &meta, true, true);
    assert!(err.is_err(), "incomplete document must fail UA validation");

    // Completed: title (both <title> and metadata) + lang + real content.
    // If this ever fails, the panic prints the recorded violations for triage.
    let good = r#"<html lang="en"><head><title>Complete</title></head>
<body><h1>Start</h1><p>Accessible body text.</p></body></html>"#;
    let (dom2, layout2) = laid_out(good);
    let meta2 = DocumentMetadata {
        title: Some("Complete".into()),
        ..DocumentMetadata::default()
    };
    if let Err(e) = render_with_options(&layout2, Some(&dom2), &meta2, true, true) {
        panic!("complete document should pass UA validation: {e:?}");
    }
}

/// CORE-396: a draw that paints nothing must not open a marked-content
/// sequence. Scans every content stream for an empty `BDC` .. `EMC` body.
#[test]
fn no_empty_marked_content_sequences() {
    use std::io::Read;
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../corpus/");
    for (cat, name) in [
        ("tickets", "weasyprint-ticket"),
        ("financial", "weasyprint-invoice"),
        ("reports", "weasyprint-report"),
    ] {
        let html = std::fs::read_to_string(format!("{dir}{cat}/{name}.html")).unwrap();
        let mut css = String::new();
        let mut rest = html.as_str();
        while let Some(a) = rest.find("<style") {
            let body = &rest[a..];
            let s = body.find('>').unwrap() + 1;
            let e = body.find("</style>").unwrap();
            css.push_str(&body[s..e]);
            rest = &body[e + 8..];
        }
        let dom = Dom::parse(&html).unwrap();
        let layout = layout::layout(&dom, &Stylesheet::parse(&css), geometry());
        let pdf = render_with_options(&layout, Some(&dom), &DocumentMetadata::default(), true, false)
            .unwrap();
        let mut pos = 0;
        let mut streams = 0;
        while let Some(i) = pdf[pos..].windows(6).position(|w| w == b"stream") {
            let start = pos + i + 6;
            let start = start + if pdf[start] == b'\r' { 2 } else { 1 };
            let Some(end) = pdf[start..].windows(9).position(|w| w == b"endstream") else {
                break;
            };
            let mut out = Vec::new();
            if flate2::read::ZlibDecoder::new(&pdf[start..start + end])
                .read_to_end(&mut out)
                .is_ok()
                && out.windows(3).any(|w| w == b"BDC")
            {
                streams += 1;
                let text = String::from_utf8_lossy(&out);
                assert!(
                    !text.contains("BDC\nEMC") && !text.contains("BDC EMC"),
                    "{name}: empty BDC..EMC sequence"
                );
            }
            pos = start + end + 9;
        }
        assert!(streams > 0, "{name}: no tagged content stream found");
    }
}
