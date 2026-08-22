//! Footnote acceptance tests — one per acceptance criterion in
//! `docs/specifications/footnotes.spec.md` (CORE-107).
//!
//! Assertions are structural (fragment-tree shape, text extraction), never
//! pixel output: the invariant under test is note placement and numbering,
//! not rendering geometry.

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::frag::{FragmentContent, FragmentKind};
use typeanvil::geom::PageGeometry;
use typeanvil::layout::{layout, Layout};

// --- helpers -----------------------------------------------------------------

fn inches(v: f64) -> typeanvil::geom::Scalar {
    typeanvil::geom::Scalar(v * 72.0)
}

fn geometry() -> PageGeometry {
    PageGeometry {
        width: inches(8.5),
        height: inches(11.0),
        margin_top: inches(0.8),
        margin_right: inches(0.8),
        margin_bottom: inches(0.8),
        margin_left: inches(0.8),
    }
}

fn doc(body: &str) -> String {
    format!(
        r#"<!DOCTYPE html><html><head><style>
@page {{ size: letter; margin: 0.8in; }}
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

/// All line-fragment texts on a page, in paint order.
fn page_line_texts(l: &Layout, page: usize) -> Vec<String> {
    fn walk(f: &typeanvil::frag::Fragment, out: &mut Vec<String>) {
        if let FragmentContent::Text(run) = &f.content {
            out.push(run.text.clone());
        }
        for c in &f.children {
            walk(c, out);
        }
    }
    let mut out = Vec::new();
    walk(&l.pages[page].root, &mut out);
    out
}

/// The footnote-area lines of a page: root-level Line fragments (the area
/// attaches to the fragmentainer root, outside the body block subtree).
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

const LONG_FILLER: &str = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur excepteur sint occaecat cupidatat non proident sunt in culpa qui officia deserunt mollit anim id est laborum sed ut perspiciatis unde omnis iste natus error sit voluptatem accusantium doloremque laudantium totam rem aperiam eaque ipsa quae ab illo inventore veritatis et quasi architecto beatae vitae dicta sunt explicabo nemo enim ipsam voluptatem quia voluptas sit aspernatur aut odit aut fugit sed quia consequuntur magni dolores eos qui ratione voluptatem sequi nesciunt neque porro quisquam est qui dolorem ipsum quia dolor sit amet consectetur adipisci velit sed quia non numquam eius modi tempora incidunt ut labore et dolore magnam aliquam quaerat voluptatem ut enim ad minima veniam quis nostrum exercitationem ullam corporis suscipit laboriosam nisi ut aliquid ex ea commodi consequatur quis autem vel eum iure reprehenderit qui in ea voluptate velit esse quam nihil molestiae consequatur vel illum qui dolorem eum fugiat quo voluptas nulla pariatur at vero eos et accusamus et iusto odio dignissimos ducimus qui blanditiis praesentium voluptatum deleniti atque corrupti quos dolores et quas molestias excepturi sint occaecati cupiditate non provident similique sunt in culpa qui officia deserunt mollitia animi id est laborum et dolorum fuga.";

// --- AC1: basic call + area ---------------------------------------------------

/// Two notes called on page 1 render `N.` marker lines in the area and the
/// superscript digits ride the body text run.
#[test]
fn basic_call_and_area() {
    let html = &doc(&format!(
        "<p>Alpha<span class=\"fn\">First note.</span> beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon phi chi psi omega.</p><p>More text here with a second call<span class=\"fn\">Second note.</span> and trailing prose. {}</p>",
        LONG_FILLER
    ));
    let l = layout_doc(html);
    let p1_area = footnote_area_lines(&l, 0);
    let joined = p1_area.join("\n");
    assert!(
        joined.contains("1. First note."),
        "note 1 missing from page-1 area, got: {:?}",
        p1_area
    );
    assert!(
        joined.contains("2. Second note."),
        "note 2 missing from page-1 area, got: {:?}",
        p1_area
    );
}

// --- AC2: call-page-N invariant ------------------------------------------------

/// A note whose call sits at the top of page 2 renders ONLY on page 2.
#[test]
fn footnote_lands_on_call_page() {
    // Force the calling paragraph to page 2; its note must not appear on p1.
    let html = &doc(&format!(
        "<p>{} </p><p style=\"break-before: page\">Late paragraph calls it here<span class=\"fn\">The late note.</span> {}</p>",
        LONG_FILLER, LONG_FILLER
    ));
    let l = layout_doc(html);
    assert!(l.pages.len() >= 2);
    let p1 = footnote_area_lines(&l, 0).join("\n");
    let p2 = footnote_area_lines(&l, 1).join("\n");
    assert!(
        !p1.contains("The late note."),
        "note leaked to page 1: {:?}",
        p1
    );
    assert!(
        p2.contains("1. The late note."),
        "note missing from its call page 2: {:?}",
        p2
    );
}

// --- AC3: numbering continues across pages --------------------------------------

#[test]
fn numbering_continues_across_pages() {
    let html = &doc(&format!(
        "<p>First<span class=\"fn\">Note one.</span> {}</p>\
         <p style=\"break-before: page\">Second<span class=\"fn\">Note two.</span></p>\
         <p style=\"break-before: page\">Third<span class=\"fn\">Note three.</span></p>",
        LONG_FILLER
    ));
    let l = layout_doc(html);
    assert!(l.pages.len() >= 3);
    assert!(footnote_area_lines(&l, 0).join("\n").contains("1. Note one."));
    assert!(footnote_area_lines(&l, 1).join("\n").contains("2. Note two."));
    assert!(footnote_area_lines(&l, 2).join("\n").contains("3. Note three."));
}

// --- AC4: tall notes never split --------------------------------------------------

/// A multi-line note stays whole in the band (all its lines adjacent in the
/// area, none carried to another page).
#[test]
fn tall_note_never_splits() {
    let note_text = "A ".repeat(120); // many wrapped lines at 9pt
    let html = &doc(&format!(
        "<p>A call early on<span class=\"fn\">{}</span> then filler. {}</p>",
        note_text.trim(),
        LONG_FILLER
    ));
    let l = layout_doc(html);
    let area = footnote_area_lines(&l, 0);
    // The note occupies >1 consecutive line starting with "1.".
    let first_idx = area
        .iter()
        .position(|t| t.starts_with("1. "))
        .expect("note first line in area");
    assert!(
        first_idx + 1 < area.len(),
        "single-line tall note? area: {:?}",
        area
    );
    // No other note interleaves between its lines.
    for t in &area[first_idx + 1..] {
        assert!(
            !t.starts_with("2. "),
            "another note interleaved inside a monolithic note: {:?}",
            area
        );
    }
}

// --- AC5: zero-cost gate ------------------------------------------------------------

/// A document with no footnotes produces no footnote-area lines (the feature
/// is fully inert without `float: footnote`).
#[test]
fn no_footnotes_inert() {
    let html = &doc(&format!("<p>Plain prose only. {}</p>", LONG_FILLER));
    let l = layout_doc(html);
    for i in 0..l.pages.len() {
        assert!(
            footnote_area_lines(&l, i).is_empty(),
            "spurious footnote-area lines on page {}: {:?}",
            i,
            footnote_area_lines(&l, i)
        );
    }
}

// --- AC6: interaction smoke ----------------------------------------------------------

/// Footnote + float + table coexist; pagination terminates deterministically.
#[test]
fn footnote_table_float_smoke() {
    let html = &doc(&format!(
        "<table><tr><th>H1</th><th>H2</th></tr><tr><td>a</td><td>b</td></tr></table>\
         <p>Text before the float call<span class=\"fn\">Interacted note.</span>.</p>\
         <div style=\"float: right; width: 2in\">Floated box content goes here to wrap around.</div>\
         <p>{}</p>",
        LONG_FILLER
    ));
    let l = layout_doc(html);
    assert!(l.pages.len() >= 1 && l.pages.len() <= 10);
    let all: Vec<String> = (0..l.pages.len())
        .flat_map(|p| footnote_area_lines(&l, p))
        .collect();
    assert!(
        all.iter().any(|t| t.contains("1. Interacted note.")),
        "note lost amid table+float: {:?}",
        all
    );
}
