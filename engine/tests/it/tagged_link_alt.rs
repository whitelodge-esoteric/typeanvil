//! Tagged PDF: link annotations carry alt text (CORE-381).
//!
//! PDF/UA-1 §7.18.5 requires alternate text (`/Contents`) on every link
//! annotation; krilla's UA validator rejects a link without it
//! (`MissingAnnotationAltText`). Rule (tagged-pdf.spec.md Behavior 6): the
//! `<a>` element's text with whitespace collapsed, else its `title`, else its
//! `href`. Untagged output writes no `/Contents`.
//!
//! Contract under test:
//! - each link shape renders with `--tagged --ua`;
//! - the annotation's `/Contents` is exactly the rule's text;
//! - untagged output carries no `/Contents`.

use typeanvil::css::Stylesheet;
use typeanvil::dom::Dom;
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout;
use typeanvil::pdf::{render_with_options, DocumentMetadata};

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

fn render(body: &str, css: &str, tagged: bool, ua: bool) -> Result<Vec<u8>, String> {
    let html = format!(
        r#"<html lang="en"><head><title>T</title></head><body><h1>Heading</h1>{body}</body></html>"#
    );
    let dom = Dom::parse(&html).unwrap();
    let laid = layout::layout(&dom, &Stylesheet::parse(css), geometry());
    let meta = DocumentMetadata {
        title: Some("T".into()),
        ..DocumentMetadata::default()
    };
    render_with_options(&laid, Some(&dom), &meta, tagged, ua).map_err(|e| format!("{e:?}"))
}

/// Every `/Contents(…)` literal in the file (annotation dicts are not
/// stream-compressed).
fn contents(pdf: &[u8]) -> Vec<String> {
    let s = String::from_utf8_lossy(pdf);
    s.match_indices("/Contents(")
        .map(|(i, m)| {
            let rest = &s[i + m.len()..];
            rest[..rest.find(')').unwrap()].to_string()
        })
        .collect()
}

fn assert_alt(body: &str, css: &str, expected: &[&str]) {
    let pdf = render(body, css, true, true)
        .unwrap_or_else(|e| panic!("`{body}` must pass --tagged --ua: {e}"));
    assert_eq!(contents(&pdf), expected, "`{body}`: link alt text");
    let plain = render(body, css, false, false).unwrap();
    assert!(contents(&plain).is_empty(), "`{body}`: untagged output must carry no /Contents");
}

#[test]
fn external_link_alt_is_its_text() {
    assert_alt(
        r#"<p>See <a href="https://example.com/x">the example   site</a>.</p>"#,
        "",
        &["the example site"],
    );
}

#[test]
fn internal_link_alt_is_its_text() {
    assert_alt(
        r##"<p>See <a href="#s2">section two</a>.</p><h2 id="s2">Two</h2>"##,
        "",
        &["section two"],
    );
}

/// A link wrapped over several lines yields one annotation per line; each
/// carries the whole link's text.
#[test]
fn wrapped_link_alt_is_the_whole_text_on_each_piece() {
    let body = r#"<p class="n">A <a href="https://example.com">link whose text wraps over two lines</a> here.</p>"#;
    let css = ".n { width: 120px }";
    let pdf = render(body, css, true, true).unwrap_or_else(|e| panic!("must pass --tagged --ua: {e}"));
    let alts = contents(&pdf);
    assert!(alts.len() >= 2, "the link must wrap into several annotations, got {alts:?}");
    assert!(
        alts.iter().all(|a| a == "link whose text wraps over two lines"),
        "every piece carries the whole link text: {alts:?}"
    );
    assert!(contents(&render(body, css, false, false).unwrap()).is_empty());
}

#[test]
fn whitespace_only_link_alt_falls_back_to_title() {
    assert_alt(
        r#"<p><a href="https://example.com" title="Example   home"> </a>x</p>"#,
        "",
        &["Example home"],
    );
}

const PNG_1X1: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

/// CORE-391: an image-only link gets an annotation whose alt text is the
/// image's `alt`, and the render passes `--tagged --ua`.
#[test]
fn image_only_link_alt_is_the_image_alt() {
    let body = format!(
        r#"<p><a href="https://example.com"><img src="{PNG_1X1}" alt="Example logo" style="width:20px;height:20px"></a></p>"#
    );
    assert_alt(&body, "", &["Example logo"]);
}
