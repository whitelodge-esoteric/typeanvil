//! CORE-390: tagged read-back for running elements.
//!
//! Decision (spec "Running elements"): every margin-box copy is a Header
//! Artifact, and the out-of-flow source element enters no structure element.
//! Observable result: the tagged `/P` MCID count with the running element
//! equals the count for the same document with the element removed, and one
//! Header artifact appears per page.

use std::io::Read;

use typeanvil::css::Stylesheet;
use typeanvil::dom::Dom;
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout;
use typeanvil::pdf::{render_with_options, DocumentMetadata};

fn geometry() -> PageGeometry {
    PageGeometry {
        width: Scalar(360.0),
        height: Scalar(216.0),
        margin_top: Scalar(36.0),
        margin_right: Scalar(36.0),
        margin_bottom: Scalar(36.0),
        margin_left: Scalar(36.0),
    }
}

const CSS: &str = ".r{position:running(r)} @page{@top-right{content:element(r)}} p{margin:0 0 80px}";

/// Returns (pdf bytes, page count).
fn render(body: &str) -> (Vec<u8>, usize) {
    let html = format!("<html lang=\"en\"><head><title>T</title></head><body>{body}</body></html>");
    let dom = Dom::parse(&html).unwrap();
    let laid = layout::layout(&dom, &Stylesheet::parse(CSS), geometry());
    let meta = DocumentMetadata {
        title: Some("T".into()),
        ..DocumentMetadata::default()
    };
    let pages = laid.pages.len();
    let pdf = render_with_options(&laid, Some(&dom), &meta, true, false).expect("tagged render");
    (pdf, pages)
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

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

const PARAS: &str = "<h1>Heading</h1><p>a</p><p>b</p><p>c</p><p>d</p>";

#[test]
fn running_copy_is_a_header_artifact_and_source_is_untagged() {
    let (with, pages) = render(&format!("<div class=\"r\">RUNELEM</div>{PARAS}"));
    let (without, _) = render(PARAS);
    assert!(pages >= 2, "fixture must paginate: {pages}");

    let tw = inflated_streams(&with);
    let to = inflated_streams(&without);

    let headers = tw.matches("/Subtype/Header").count();
    assert_eq!(headers, pages, "one Header artifact per page (got {headers} for {pages} pages)");
    assert_eq!(to.matches("/Subtype/Header").count(), 0);

    // Tagged (non-artifact) marked content is identical with and without the
    // running element: the source adds nothing to the structure tree.
    let tagged = |t: &str| t.matches("BDC").count() - t.matches("/Artifact").count();
    assert_eq!(tagged(&tw), tagged(&to), "running source leaked into tagged content");
}
