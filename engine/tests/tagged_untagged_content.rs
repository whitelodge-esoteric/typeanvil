//! Tagged PDF: no page object outside marked content (CORE-380).
//!
//! PDF/UA-1 (ISO 14289-1 §7.1) requires every piece of page content to be
//! either in the structure tree or marked as an Artifact. Two kinds of text
//! were drawn outside any marked-content sequence: page-margin-box text
//! (running headers, `counter(page)` footers, `string()` copies) and
//! footnote bodies. krilla's UA validator does not detect it, so these tests
//! scan the content streams directly.
//!
//! Contract under test:
//! - no text or paint operator is emitted outside a BDC/BMC … EMC sequence;
//! - margin-box text is an Artifact of kind Header (top row) or Footer
//!   (bottom row), on every page;
//! - footnote body lines are owned by their footnote element (so the body
//!   enters the structure tree with the call's paragraph).

use std::io::Read;

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::frag::{Fragment, FragmentContent};
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

/// Spans several pages: a header and footer per page, footnotes on more
/// than one page, and a `string()` running header from the h1.
const CSS: &str = r#"
h1 { string-set: chap content() }
@page {
  @top-center { content: "RUNHEAD" }
  @top-left { content: string(chap) }
  @bottom-center { content: "Page " counter(page) }
}
.fn { float: footnote }
p { margin: 0 0 60px }
"#;

fn body() -> String {
    let mut s = String::from(r#"<html lang="en"><head><title>T</title></head><body><h1>Chapter</h1>"#);
    for i in 0..5 {
        s.push_str(&format!(r#"<p>Body {i}<span class="fn">Note {i}</span></p>"#));
    }
    s.push_str("</body></html>");
    s
}

fn laid_out() -> (Dom, typeanvil::layout::Layout) {
    let dom = Dom::parse(&body()).unwrap();
    let layout = layout::layout(&dom, &Stylesheet::parse(CSS), geometry());
    (dom, layout)
}

fn tagged_pdf() -> (Vec<u8>, usize) {
    let (dom, layout) = laid_out();
    let pages = layout.pages.len();
    let meta = DocumentMetadata {
        title: Some("T".into()),
        ..DocumentMetadata::default()
    };
    (render_with_options(&layout, Some(&dom), &meta, true, false).unwrap(), pages)
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Inflated page content streams: the ones that draw text.
fn page_streams(pdf: &[u8]) -> Vec<Vec<u8>> {
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
            && find(&buf, b"BT").is_some()
            && (find(&buf, b"TJ").is_some() || find(&buf, b"Tj").is_some())
        {
            out.push(buf);
        }
        at = s + e + b"endstream".len();
    }
    out
}

/// Content-stream tokens with strings, hex strings, dicts and arrays kept
/// whole, so glyph bytes inside `(…)` never read as operators.
fn tokens(s: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    let delim = |c: u8| c.is_ascii_whitespace() || b"()<>[]{}/%".contains(&c);
    while i < s.len() {
        let c = s[i];
        if c.is_ascii_whitespace() {
            i += 1;
        } else if c == b'(' {
            let (mut depth, start) = (0, i);
            while i < s.len() {
                match s[i] {
                    b'\\' => i += 1,
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            i += 1;
                            break;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            out.push(String::from_utf8_lossy(&s[start..i]).into_owned());
        } else if s[i..].starts_with(b"<<") || s[i..].starts_with(b">>") || c == b'[' || c == b']' {
            let n = if c == b'[' || c == b']' { 1 } else { 2 };
            out.push(String::from_utf8_lossy(&s[i..i + n]).into_owned());
            i += n;
        } else if c == b'<' {
            let start = i;
            while i < s.len() && s[i] != b'>' {
                i += 1;
            }
            i += 1;
            out.push(String::from_utf8_lossy(&s[start..i]).into_owned());
        } else {
            let start = i;
            i += 1;
            while i < s.len() && !delim(s[i]) {
                i += 1;
            }
            out.push(String::from_utf8_lossy(&s[start..i]).into_owned());
        }
    }
    out
}

const DRAW_OPS: &[&str] = &["Tj", "TJ", "'", "\"", "f", "F", "f*", "S", "s", "B", "B*", "b", "b*", "Do", "sh"];

/// Operators drawn at marked-content depth 0, per stream.
fn draws_outside_marked_content(stream: &[u8]) -> Vec<String> {
    let mut depth = 0i32;
    let mut out = Vec::new();
    for t in tokens(stream) {
        match t.as_str() {
            "BDC" | "BMC" => depth += 1,
            "EMC" => depth -= 1,
            op if depth == 0 && DRAW_OPS.contains(&op) => out.push(op.to_string()),
            _ => {}
        }
    }
    out
}

#[test]
fn nothing_is_drawn_outside_marked_content() {
    let (pdf, pages) = tagged_pdf();
    assert!(pages >= 2, "fixture must span pages, got {pages}");
    let streams = page_streams(&pdf);
    assert_eq!(streams.len(), pages, "one text-drawing content stream per page");
    for (i, s) in streams.iter().enumerate() {
        let loose = draws_outside_marked_content(s);
        assert!(loose.is_empty(), "page {}: drawn outside marked content: {loose:?}", i + 1);
    }
}

#[test]
fn margin_box_text_is_header_and_footer_artifacts_on_every_page() {
    let (pdf, pages) = tagged_pdf();
    for (i, s) in page_streams(&pdf).iter().enumerate() {
        let text = String::from_utf8_lossy(s);
        // krilla writes the subtype into the Artifact property list.
        assert!(text.contains("/Header"), "page {}: no Header artifact", i + 1);
        assert!(text.contains("/Footer"), "page {}: no Footer artifact", i + 1);
    }
    assert!(pages >= 2);
}

fn walk<'a>(f: &'a Fragment, out: &mut Vec<&'a Fragment>) {
    out.push(f);
    for c in &f.children {
        walk(c, out);
    }
}

#[test]
fn footnote_body_lines_are_owned_by_their_footnote() {
    let (dom, layout) = laid_out();
    let notes: Vec<usize> = dom
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| matches!(&n.kind, NodeKind::Element(e) if e.classes.iter().any(|c| c == "fn")))
        .map(|(id, _)| id)
        .collect();
    assert_eq!(notes.len(), 5);
    let mut found = 0;
    for page in &layout.pages {
        let mut frags = Vec::new();
        walk(&page.root, &mut frags);
        for f in frags {
            if let FragmentContent::Text(run) = &f.content {
                if run.text.contains("Note ") {
                    found += 1;
                    let src = f.source.unwrap_or_else(|| panic!("footnote line {:?} has no source", run.text));
                    assert!(notes.contains(&src), "footnote line {:?} owned by {src}, not a footnote", run.text);
                }
            }
        }
    }
    assert_eq!(found, 5, "every footnote body laid out once");
}
