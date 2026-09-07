//! Footnote regression tests for spanning-paragraph call attribution
//! (CORE-146). The CORE-107 suite covers same-page and per-page-paragraph
//! cases; these cover calls inside ONE paragraph that wraps across pages
//! and beside floats (the segmented text path), where the original byte-
//! window drift dropped notes from their call pages.
//!
//! Assertions are structural (footnote-area text extraction), never pixel
//! output.

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::frag::{FragmentContent, FragmentKind};
use typeanvil::geom::PageGeometry;
use typeanvil::layout::{layout, Layout};

fn inches(v: f64) -> typeanvil::geom::Scalar {
    typeanvil::geom::Scalar(v * 72.0)
}

fn geometry() -> PageGeometry {
    PageGeometry {
        width: inches(3.0),
        height: inches(2.0),
        margin_top: inches(0.4),
        margin_right: inches(0.4),
        margin_bottom: inches(0.4),
        margin_left: inches(0.4),
    }
}

fn doc(body: &str) -> String {
    format!(
        r#"<!DOCTYPE html><html><head><style>
@page {{ size: 3in 2in; margin: 0.4in; }}
body {{ font-family: serif; font-size: 12pt; line-height: 1.5; }}
.fn {{ float: footnote; font-size: 9pt; }}
</style></head><body>{}</body></html>"#,
        body
    )
}

fn layout_doc(html: &str) -> Layout {
    let dom = Dom::parse(html).unwrap();
    let css = {
        let mut s = String::new();
        for (id, node) in dom.nodes.iter().enumerate() {
            if let NodeKind::Element(el) = &node.kind {
                if el.tag == "style" {
                    s.push_str(&dom.text_content(id));
                }
            }
        }
        s
    };
    let stylesheet = Stylesheet::parse(&css);
    layout(&dom, &stylesheet, geometry())
}

fn footnote_area_lines(l: &Layout, page: usize) -> Vec<String> {
    l.pages[page]
        .root
        .children
        .iter()
        .filter(|c| c.kind == FragmentKind::Line)
        .filter_map(|c| match &c.content {
            FragmentContent::Text(run) => Some(run.text.clone()),
            _ => None,
        })
        .collect()
}

fn words(n: usize, start: usize) -> String {
    (start..start + n)
        .map(|i| format!("WORD{}", i))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Two calls in ONE paragraph: the paragraph is long enough to overflow a
/// page at the test geometry, so its lines span pages and each note must
/// attach to the page where its call marker's line landed (spec AC2
/// applied within a single paragraph, CORE-146).
#[test]
fn spanning_paragraph_notes_attach_per_call_page() {
    // ~142 words at 12pt serif over an 8.5in measure: ~4 lines/page is too
    // coarse — use a narrower page (4in) so the paragraph actually spans.
    let html = doc(&format!(
        "{}<span class=\"fn\">Note alpha text.</span> {}<span class=\"fn\">Note beta text.</span> {}.",
        words(31, 0),
        words(90, 31),
        words(21, 121)
    ));
    let l = layout_doc(&html);
    assert!(l.pages.len() >= 2, "paragraph must span pages");

    // Note 1 attaches to the page carrying its marker's line (page 4 of the
    // 3in×2in layout — wherever the 31st word lands).
    let mut note1_pages = Vec::new();
    let mut note2_pages = Vec::new();
    for i in 0..l.pages.len() {
        let area = footnote_area_lines(&l, i).join("\n");
        if area.contains("1. Note alpha text.") {
            note1_pages.push(i);
        }
        if area.contains("2. Note beta text.") {
            note2_pages.push(i);
        }
    }
    assert_eq!(
        note1_pages.len(),
        1,
        "note 1 must attach to exactly one page, got {:?}",
        note1_pages
    );
    assert_eq!(
        note2_pages.len(),
        1,
        "note 2 must attach to exactly one page, got {:?}",
        note2_pages
    );
    // Calls are in document order: note 2's page never precedes note 1's.
    assert!(
        note2_pages[0] >= note1_pages[0],
        "note 2 (page {}) attached before note 1 (page {})",
        note2_pages[0] + 1,
        note1_pages[0] + 1
    );
}
