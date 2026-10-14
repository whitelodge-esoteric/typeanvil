//! Tagged rendering of page chrome (CORE-379).
//!
//! Page-level paint that has no DOM owner (the `@page` fill, the propagated
//! canvas fill, the `@page` outline and border rings, the root border ring)
//! is tagged as a Background artifact. PDF 1.7 (ISO 32000-1 table 330)
//! requires a BBox on Background artifacts, and krilla panics without one, so
//! the whole process aborted on the first page.
//!
//! Contract under test:
//! - every page-chrome kind renders with `--tagged` and `--tagged --ua`;
//! - each Background artifact carries a BBox that lies on the page, including
//!   on a rotated page (krilla writes the BBox in page space and ignores the
//!   current transform).
//!
//! This lives in its own test binary: the pre-fix failure is a non-unwinding
//! abort, which would take the other tagged tests down with it.

use std::io::Read;

use typeanvil::css::Stylesheet;
use typeanvil::dom::Dom;
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout;
use typeanvil::pdf::{render_with_options, DocumentMetadata};

const W: f32 = 5.0 * 72.0;
const H: f32 = 3.0 * 72.0;

fn geometry() -> PageGeometry {
    PageGeometry {
        width: Scalar(W as f64),
        height: Scalar(H as f64),
        margin_top: Scalar(36.0),
        margin_right: Scalar(36.0),
        margin_bottom: Scalar(36.0),
        margin_left: Scalar(36.0),
    }
}

const BODY: &str = r#"<html lang="en"><head><title>T</title></head>
<body><h1>Heading</h1><p>Body text.</p></body></html>"#;

fn render(css: &str, ua: bool) -> Vec<u8> {
    let dom = Dom::parse(BODY).unwrap();
    let laid = layout::layout(&dom, &Stylesheet::parse(css), geometry());
    let meta = DocumentMetadata {
        title: Some("T".into()),
        ..DocumentMetadata::default()
    };
    render_with_options(&laid, Some(&dom), &meta, true, ua)
        .unwrap_or_else(|e| panic!("tagged render of `{css}` failed: {e:?}"))
}

/// Inflate every Flate stream that inflates cleanly and concatenate them.
fn inflated_streams(pdf: &[u8]) -> String {
    let mut out = String::new();
    let mut at = 0;
    while let Some(i) = find(&pdf[at..], b"stream") {
        let mut s = at + i + b"stream".len();
        if pdf[s..].starts_with(b"\r\n") {
            s += 2;
        } else if pdf[s..].starts_with(b"\n") {
            s += 1;
        }
        let Some(e) = find(&pdf[s..], b"endstream") else { break };
        let mut buf = Vec::new();
        if flate2::read::ZlibDecoder::new(&pdf[s..s + e])
            .read_to_end(&mut buf)
            .is_ok()
        {
            out.push_str(&String::from_utf8_lossy(&buf));
            out.push('\n');
        }
        at = s + e + b"endstream".len();
    }
    out
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// The BBox arrays of every Background artifact in the content streams.
fn background_bboxes(pdf: &[u8]) -> Vec<[f32; 4]> {
    let text = inflated_streams(pdf);
    let mut boxes = Vec::new();
    // Each artifact opens as `/Artifact<<…>>BDC`; key order inside the
    // property list is krilla's (BBox is written before Type).
    for (i, _) in text.match_indices("/Artifact") {
        let rest = &text[i..];
        let end = rest.find("BDC").unwrap_or(rest.len());
        let props = &rest[..end];
        if !props.contains("/Background") {
            continue;
        }
        let b = props
            .find("/BBox")
            .unwrap_or_else(|| panic!("Background artifact without /BBox: {props}"));
        let open = props[b..].find('[').unwrap() + b + 1;
        let close = props[open..].find(']').unwrap() + open;
        let v: Vec<f32> = props[open..close]
            .split_whitespace()
            .map(|n| n.parse().unwrap())
            .collect();
        boxes.push([v[0], v[1], v[2], v[3]]);
    }
    boxes
}

fn assert_on_page(css: &str, page_w: f32, page_h: f32) {
    for ua in [false, true] {
        let pdf = render(css, ua);
        let boxes = background_bboxes(&pdf);
        assert!(!boxes.is_empty(), "`{css}`: no Background artifact emitted");
        for [x0, y0, x1, y1] in boxes {
            let eps = 0.01;
            assert!(
                x0 >= -eps && y0 >= -eps && x1 <= page_w + eps && y1 <= page_h + eps
                    && x0 < x1 && y0 < y1,
                "`{css}`: BBox [{x0} {y0} {x1} {y1}] is not on the {page_w}x{page_h} page"
            );
        }
    }
}

#[test]
fn page_background_renders_tagged() {
    assert_on_page("@page { background: #eef1f5 }", W, H);
}

#[test]
fn canvas_background_renders_tagged() {
    assert_on_page("body { background: #eef1f5 }", W, H);
}

#[test]
fn page_outline_renders_tagged() {
    assert_on_page("@page { outline: 4px solid red; outline-offset: 4px }", W, H);
}

#[test]
fn page_border_renders_tagged() {
    assert_on_page("@page { border: 4px solid red }", W, H);
}

#[test]
fn root_border_renders_tagged() {
    assert_on_page("html { border: 4px solid green }", W, H);
}

/// A rotated page swaps the media box; the BBox must be mapped through the
/// same rotation as the fill, or it describes an area off the page.
#[test]
fn rotated_page_background_bbox_is_on_page() {
    assert_on_page(
        "@page { background: #eef1f5; page-orientation: rotate-left }",
        H,
        W,
    );
}
