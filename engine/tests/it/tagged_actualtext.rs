//! Tagged PDF: ActualText for hyphenated lines (CORE-376, spec Behavior 10).
//!
//! Contract under test:
//! - a line that ends in the engine-inserted hyphen is wrapped in a `Span`
//!   marked-content sequence whose `/ActualText` is the line without it;
//! - a source hyphen stays in the text and gets no `ActualText`;
//! - untagged output is unchanged and carries no `/ActualText`;
//! - tagged output is deterministic and `--ua` stays clean.

use std::io::Read;

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

const CSS: &str = "p { hyphens: auto; width: 90pt; font-size: 12pt }";
const HTML: &str = r#"<html lang="en"><head><title>T</title></head><body>
<p>start supercalifragilisticexpialidocious fine flow office end</p>
</body></html>"#;

fn laid_out() -> (Dom, layout::Layout) {
    let dom = Dom::parse(HTML).unwrap();
    let layout = layout::layout(&dom, &Stylesheet::parse(CSS), geometry());
    (dom, layout)
}

fn meta() -> DocumentMetadata {
    DocumentMetadata {
        title: Some("T".into()),
        ..DocumentMetadata::default()
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// All inflated streams (page content streams carry the BDC property dicts).
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
        if flate2::read::ZlibDecoder::new(&pdf[s..s + e])
            .read_to_end(&mut buf)
            .is_ok()
        {
            out.push(buf);
        }
        at = s + e + b"endstream".len();
    }
    out
}

/// Every `/ActualText` value in the page streams, decoded to a String.
/// Values are literal `(...)` strings (PDFDocEncoding) or `<hex>` UTF-16BE.
fn actual_texts(pdf: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    for s in streams(pdf) {
        let mut at = 0;
        while let Some(i) = find(&s[at..], b"/ActualText") {
            let mut p = at + i + b"/ActualText".len();
            while s[p].is_ascii_whitespace() {
                p += 1;
            }
            match s[p] {
                b'(' => {
                    let end = s[p..].iter().position(|&c| c == b')').unwrap();
                    out.push(String::from_utf8_lossy(&s[p + 1..p + end]).into_owned());
                }
                b'<' => {
                    let end = s[p..].iter().position(|&c| c == b'>').unwrap();
                    let hex = String::from_utf8_lossy(&s[p + 1..p + end]).into_owned();
                    let bytes: Vec<u8> = (0..hex.len() / 2)
                        .map(|k| u8::from_str_radix(&hex[2 * k..2 * k + 2], 16).unwrap())
                        .collect();
                    let units: Vec<u16> = bytes
                        .chunks(2)
                        .map(|c| u16::from_be_bytes([c[0], c[1]]))
                        .collect();
                    let text = String::from_utf16_lossy(&units);
                    out.push(text.trim_start_matches('\u{feff}').to_string());
                }
                _ => panic!("unexpected ActualText value"),
            }
            at = p;
        }
    }
    out
}

/// The hyphenated fragments carry ActualText without the synthetic hyphen,
/// and no ActualText value ends in a hyphen the engine added.
#[test]
fn hyphenated_lines_carry_actualtext() {
    let (dom, layout) = laid_out();
    let pdf = render_with_options(&layout, Some(&dom), &meta(), true, false).unwrap();
    let texts = actual_texts(&pdf);
    assert!(
        !texts.is_empty(),
        "hyphenated fixture must produce at least one ActualText"
    );
    for t in &texts {
        assert!(!t.ends_with('-'), "ActualText must drop the synthetic hyphen: {t:?}");
        assert!(!t.is_empty());
    }
    // Joined with the following line, the fragment must spell source text.
    let joined: String = texts.concat();
    assert!(
        "supercalifragilisticexpialidocious".contains(texts.last().unwrap().split(' ').last().unwrap())
            || joined.contains("super"),
        "fragments should come from the long word: {texts:?}"
    );
}

/// Untagged output has no ActualText (and is unchanged by this feature).
#[test]
fn untagged_has_no_actualtext() {
    let (_dom, layout) = laid_out();
    let pdf = render_with_options(&layout, None, &meta(), false, false).unwrap();
    assert!(actual_texts(&pdf).is_empty());
}

/// Determinism and `--ua` cleanliness with ActualText present.
#[test]
fn actualtext_render_is_deterministic_and_ua_clean() {
    let (dom, layout) = laid_out();
    let a = render_with_options(&layout, Some(&dom), &meta(), true, false).unwrap();
    let b = render_with_options(&layout, Some(&dom), &meta(), true, false).unwrap();
    assert_eq!(a, b);
    let ua = render_with_options(&layout, Some(&dom), &meta(), true, true);
    assert!(ua.is_ok(), "--ua must stay clean: {:?}", ua.err());
}

/// A hyphen that exists in the source is not synthetic: no ActualText.
#[test]
fn source_hyphen_gets_no_actualtext() {
    let html = r#"<html lang="en"><head><title>T</title></head><body>
<p>well-known well-known well-known well-known</p></body></html>"#;
    let dom = Dom::parse(html).unwrap();
    let layout = layout::layout(
        &dom,
        &Stylesheet::parse("p { hyphens: manual; width: 60pt; font-size: 12pt }"),
        geometry(),
    );
    let pdf = render_with_options(&layout, Some(&dom), &meta(), true, false).unwrap();
    assert!(actual_texts(&pdf).is_empty(), "{:?}", actual_texts(&pdf));
}
