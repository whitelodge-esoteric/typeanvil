//! CORE-539: letter-spaced text keeps its words in the PDF text layer.
//! letter-spacing widens every glyph gap, so a tagged render must carry the
//! source string as `/ActualText`; unspaced text must not.

use std::io::Read;

use typeanvil::css::Stylesheet;
use typeanvil::dom::Dom;
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::layout;
use typeanvil::pdf::{render_with_options, DocumentMetadata};

const HTML: &str = "<html><body><p>LIVE IN CONCERT</p></body></html>";

fn render(css: &str) -> Vec<u8> {
    let dom = Dom::parse(HTML).expect("parse html");
    let g = PageGeometry {
        width: Scalar(612.0),
        height: Scalar(792.0),
        margin_top: Scalar(36.0),
        margin_right: Scalar(36.0),
        margin_bottom: Scalar(36.0),
        margin_left: Scalar(36.0),
    };
    let laid = layout(&dom, &Stylesheet::parse(css), g);
    render_with_options(&laid, Some(&dom), &DocumentMetadata::default(), true, false)
        .expect("render")
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// All inflated streams; page content streams carry the BDC property dicts.
fn streams(pdf: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
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
        if flate2::read::ZlibDecoder::new(&pdf[s..s + e]).read_to_end(&mut buf).is_ok() {
            out.push(buf);
        }
        at = s + e + b"endstream".len();
    }
    out
}

fn has_actual_text(pdf: &[u8], text: &str) -> bool {
    let utf16: Vec<u8> = text.encode_utf16().flat_map(|u| u.to_be_bytes()).collect();
    let hex: String = utf16.iter().map(|b| format!("{b:02X}")).collect();
    streams(pdf).iter().any(|s| {
        let mut at = 0;
        while let Some(i) = find(&s[at..], b"/ActualText") {
            let p = at + i;
            let tail = String::from_utf8_lossy(&s[p..(p + 200).min(s.len())]).to_uppercase();
            if tail.contains(&hex) || tail.contains(&text.to_uppercase()) {
                return true;
            }
            at = p + 1;
        }
        false
    })
}

#[test]
fn letter_spaced_run_carries_source_text_as_actual_text() {
    let pdf = render("p { letter-spacing: 6pt }");
    assert!(
        has_actual_text(&pdf, "LIVE IN CONCERT"),
        "letter-spaced run must carry ActualText with the source string"
    );
}

#[test]
fn plain_run_has_no_actual_text() {
    let pdf = render("");
    assert!(
        !streams(&pdf).iter().any(|s| find(s, b"/ActualText").is_some()),
        "unspaced text must not carry ActualText"
    );
}
