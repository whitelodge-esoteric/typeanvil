//! Tagged PDF: a document with no headings still gets an outline (CORE-382).
//!
//! krilla's PDF/UA-1 validator requires a document outline
//! (`MissingDocumentOutline`). A letter or poster with no headings produced
//! none, so it could never pass `--ua`. Rule (tagged-pdf.spec.md Behavior
//! 7a, user decision 2026-10-13): a tagged render whose outline would be
//! empty gets one entry, the document title, pointing at page 1.
//!
//! Contract under test:
//! - a titled, heading-less, multi-page document passes `--tagged --ua` with
//!   exactly one outline entry: the title, whose destination is page 1;
//! - all headings suppressed by `bookmark-level: none` → same fallback;
//! - a document with headings keeps its heading outline (no title entry);
//! - untagged output of a heading-less document still has no `/Outlines`.

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

const TITLE: &str = "Cover Letter";

fn render(body: &str, css: &str, tagged: bool, ua: bool) -> Result<Vec<u8>, String> {
    let html = format!(r#"<html lang="en"><head><title>{TITLE}</title></head><body>{body}</body></html>"#);
    let dom = Dom::parse(&html).unwrap();
    let laid = layout::layout(&dom, &Stylesheet::parse(css), geometry());
    let meta = DocumentMetadata {
        title: Some(TITLE.into()),
        ..DocumentMetadata::default()
    };
    render_with_options(&laid, Some(&dom), &meta, tagged, ua).map_err(|e| format!("{e:?}"))
}

/// Outline item titles, in file order (outline item dicts are not
/// stream-compressed; the Info dict's `/Title` is not followed by `/Dest`).
fn outline_titles(pdf: &[u8]) -> Vec<String> {
    let s = String::from_utf8_lossy(pdf);
    let mut out = Vec::new();
    for (i, m) in s.match_indices("/Title(") {
        let rest = &s[i + m.len()..];
        let close = rest.find(')').unwrap();
        if rest[close + 1..].starts_with("/Dest") {
            out.push(rest[..close].to_string());
        }
    }
    out
}

/// The object number of the page an outline item's destination targets,
/// and the object number of the document's first page.
fn dest_page_and_first_page(pdf: &[u8], title: &str) -> (String, String) {
    let s = String::from_utf8_lossy(pdf);
    let item = format!("/Title({title})/Dest ");
    let at = s.find(&item).expect("outline item") + item.len();
    let dest_obj = s[at..].split_whitespace().next().unwrap().to_string();
    let dest_at = s.find(&format!("\n{dest_obj} 0 obj")).expect("dest object");
    let dest = &s[dest_at..];
    let open = dest.find('[').unwrap() + 1;
    let page = dest[open..].split_whitespace().next().unwrap().to_string();
    let kids = s.find("/Kids[").expect("page tree") + "/Kids[".len();
    let first = s[kids..].split_whitespace().next().unwrap().to_string();
    (page, first)
}

/// Long enough to need several pages.
fn letter() -> String {
    (0..8).map(|i| format!("<p>Paragraph {i} of the letter body.</p>")).collect()
}
const TALL: &str = "p { margin: 0 0 40px }";

#[test]
fn headingless_document_gets_a_title_entry_on_page_one() {
    let pdf = render(&letter(), TALL, true, true)
        .unwrap_or_else(|e| panic!("a titled heading-less document must pass --tagged --ua: {e}"));
    assert_eq!(outline_titles(&pdf), [TITLE]);
    let (page, first) = dest_page_and_first_page(&pdf, TITLE);
    assert_eq!(page, first, "the title entry must point at page 1");
}

#[test]
fn all_headings_suppressed_gets_the_title_entry() {
    let body = format!("<h1>Hidden</h1>{}", letter());
    let pdf = render(&body, &format!("{TALL} h1 {{ bookmark-level: none }}"), true, true)
        .unwrap_or_else(|e| panic!("must pass --tagged --ua: {e}"));
    assert_eq!(outline_titles(&pdf), [TITLE]);
}

#[test]
fn headings_keep_their_outline() {
    let body = format!("<h1>One</h1>{}<h2>Two</h2>", letter());
    let pdf = render(&body, TALL, true, true).unwrap();
    let mut titles = outline_titles(&pdf);
    titles.sort();
    assert_eq!(titles, ["One", "Two"], "no extra title entry when headings exist");
}

#[test]
fn untagged_headingless_document_has_no_outline() {
    let pdf = render(&letter(), TALL, false, false).unwrap();
    assert!(!String::from_utf8_lossy(&pdf).contains("/Outlines"));
}
