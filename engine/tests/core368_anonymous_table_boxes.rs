//! CORE-368: anonymous table boxes for a table's CHILDREN (css-tables-3
//! §3.4.2, CSS2.1 §17.2.1 rules 2.1-2.3).
//!
//! Content that sits directly inside a `display: table` / row group with no
//! row, or directly inside a `display: table-row` with no cell, shall be
//! wrapped in an anonymous row / cell and rendered. The table loop used to
//! skip such children, so their content vanished. That stayed hidden while
//! the paged pass rewrote every div-based table to `display: block`; CORE-368
//! stopped that rewrite and exposed the loss.

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::frag::{Fragment, FragmentContent};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::{layout, Layout};
use typeanvil::pdf::{render_with_options, DocumentMetadata};

fn inches(v: f64) -> Scalar {
    Scalar(v * 72.0)
}

fn geometry() -> PageGeometry {
    PageGeometry {
        width: inches(5.0),
        height: inches(3.0),
        margin_top: inches(0.5),
        margin_right: inches(0.5),
        margin_bottom: inches(0.5),
        margin_left: inches(0.5),
    }
}

fn parse(html: &str) -> (Dom, Stylesheet) {
    let dom = Dom::parse(html).expect("parse html");
    let mut css = String::new();
    for (id, node) in dom.nodes.iter().enumerate() {
        if let NodeKind::Element(el) = &node.kind {
            if el.tag == "style" {
                css.push_str(&dom.text_content(id));
                css.push('\n');
            }
        }
    }
    (dom, Stylesheet::parse(&css))
}

fn lay(html: &str) -> Layout {
    let (dom, sheet) = parse(html);
    layout(&dom, &sheet, geometry())
}

/// Per-page concatenated text of every run.
fn page_texts(l: &Layout) -> Vec<String> {
    fn rec(f: &Fragment, out: &mut String) {
        if let FragmentContent::Text(run) = &f.content {
            out.push_str(&run.text);
            out.push(' ');
        }
        for c in &f.children {
            rec(c, out);
        }
    }
    l.pages
        .iter()
        .map(|p| {
            let mut s = String::new();
            rec(&p.root, &mut s);
            s
        })
        .collect()
}

fn pages_with(l: &Layout, word: &str) -> Vec<usize> {
    page_texts(l)
        .iter()
        .enumerate()
        .filter(|(_, t)| t.contains(word))
        .map(|(i, _)| i + 1)
        .collect()
}

const BODY: &str = "<style>body { margin: 0 }</style>";

#[test]
fn text_directly_in_display_table_renders() {
    let l = lay(&format!("{BODY}<div style=\"display:table\">TABLETEXT</div>"));
    assert_eq!(pages_with(&l, "TABLETEXT"), vec![1]);
}

#[test]
fn text_directly_in_table_row_renders() {
    let l = lay(&format!(
        "{BODY}<div style=\"display:table\"><div style=\"display:table-row\">ROWTEXT</div></div>"
    ));
    assert_eq!(pages_with(&l, "ROWTEXT"), vec![1]);
}

#[test]
fn cell_directly_in_table_renders() {
    let l = lay(&format!(
        "{BODY}<div style=\"display:table\"><div style=\"display:table-cell\">CELLTEXT</div></div>"
    ));
    assert_eq!(pages_with(&l, "CELLTEXT"), vec![1]);
}

/// WPT monolithic-overflow-009's shape: a tall block plus trailing text
/// directly in a `display: table` fragments like the block it wraps.
#[test]
fn tall_block_in_display_table_fragments_and_keeps_trailing_text() {
    let l = lay(&format!(
        "{BODY}<div style=\"display:table; width:100%\">\
         <div style=\"height:350vh; width:50px; background:hotpink\"></div>END</div>"
    ));
    assert_eq!(l.pages.len(), 4, "350vh fragments across 4 pages: {:?}", page_texts(&l));
    assert_eq!(pages_with(&l, "END"), vec![4]);
}

/// Parent side: a `display: table-row` with no table ancestor gets an
/// anonymous table, its bare text an anonymous cell, and forced breaks
/// inside it still fire (WPT fixedpos-in-footer-forced-break's shape).
#[test]
fn orphan_row_gets_anonymous_table_and_keeps_forced_breaks() {
    let l = lay(&format!(
        "{BODY}<div style=\"display:table-row\">PONE\
         <div style=\"break-before:page\">PTWO</div>\
         <div style=\"break-before:page\">PTHREE</div></div>"
    ));
    assert_eq!(pages_with(&l, "PONE"), vec![1], "{:?}", page_texts(&l));
    assert_eq!(pages_with(&l, "PTWO"), vec![2], "{:?}", page_texts(&l));
    assert_eq!(pages_with(&l, "PTHREE"), vec![3], "{:?}", page_texts(&l));
}

/// Guard: a stray cell in ordinary flow and a row nested inside a cell's
/// block flow both render (they already did before the parent-side fixup;
/// the fixup must not lose them).
#[test]
fn orphan_cell_and_row_inside_cell_render() {
    let l = lay(&format!(
        "{BODY}<div><span style=\"display:table-cell\">ORPHANCELL</span></div>\
         <table><tr><td><div style=\"display:table-row\">NESTEDROW</div></td></tr></table>"
    ));
    assert_eq!(pages_with(&l, "ORPHANCELL"), vec![1], "{:?}", page_texts(&l));
    assert_eq!(pages_with(&l, "NESTEDROW"), vec![1], "{:?}", page_texts(&l));
}

/// Content between proper rows is wrapped in place: document order holds.
#[test]
fn stray_content_between_rows_keeps_document_order() {
    let l = lay(&format!(
        "{BODY}<div style=\"display:table\">\
         <div style=\"display:table-row\"><div style=\"display:table-cell\">FIRST</div></div>\
         MIDDLE\
         <div style=\"display:table-row\"><div style=\"display:table-cell\">LAST</div></div></div>"
    ));
    let t = &page_texts(&l)[0];
    let (f, m, z) = (t.find("FIRST"), t.find("MIDDLE"), t.find("LAST"));
    assert!(f < m && m < z, "order FIRST < MIDDLE < LAST in {t:?}");
}

/// Guard: white space between proper table boxes generates nothing — the
/// layout is identical to the same table without the white space.
#[test]
fn whitespace_between_rows_adds_no_row() {
    let tight = lay(&format!("{BODY}<table><tr><td>A</td></tr><tr><td>B</td></tr></table><p>AFTER</p>"));
    let spaced = lay(&format!(
        "{BODY}<table>\n  <tr><td>A</td></tr>\n  <tr><td>B</td></tr>\n</table><p>AFTER</p>"
    ));
    assert_eq!(page_texts(&tight), page_texts(&spaced));
    assert_eq!(geometry_of(&tight), geometry_of(&spaced));
}

/// Every fragment's page-absolute box (node ids differ between the two
/// documents — white-space text nodes take arena slots — so compare boxes).
fn geometry_of(l: &Layout) -> Vec<(i64, i64, i64, i64)> {
    fn rec(f: &Fragment, px: f64, py: f64, out: &mut Vec<(i64, i64, i64, i64)>) {
        let ax = px + f.offset.x.get();
        let ay = py + f.offset.y.get();
        let q = |v: f64| (v * 100.0).round() as i64;
        out.push((q(ax), q(ay), q(f.size.0.get()), q(f.size.1.get())));
        for c in &f.children {
            rec(c, ax, ay, out);
        }
    }
    let mut out = Vec::new();
    for p in &l.pages {
        rec(&p.root, 0.0, 0.0, &mut out);
    }
    out
}

/// The table's OWN `background-color` paints on every fragment (it was a
/// table-backgrounds Non-Goal; div tables painted it only while they were
/// mis-laid as blocks). The pink cell content paints over it.
#[test]
fn table_background_paints_on_every_fragment() {
    fn yellow_fills(f: &Fragment, out: &mut usize) {
        if let FragmentContent::Background(c) = &f.content {
            if (c.r, c.g, c.b) == (255, 255, 0) {
                *out += 1;
            }
        }
        for ch in &f.children {
            yellow_fills(ch, out);
        }
    }
    for html in [
        "<table style=\"background:yellow; width:100%\"><tr><td>\
         <div style=\"height:350vh; width:50px; background:hotpink\"></div>END</td></tr></table>",
        "<div style=\"display:table; width:100%; background:yellow\">\
         <div style=\"height:350vh; width:50px; background:hotpink\"></div>END</div>",
    ] {
        let l = lay(&format!("{BODY}{html}"));
        assert_eq!(l.pages.len(), 4, "{html}");
        for (i, p) in l.pages.iter().enumerate() {
            let mut n = 0;
            yellow_fills(&p.root, &mut n);
            assert_eq!(n, 1, "page {} paints the table background once: {html}", i + 1);
        }
    }
}

/// The caller's DOM has no anonymous nodes, so tagged output must not look
/// one up: rendering a fixed-up table tagged must succeed.
#[test]
fn tagged_render_survives_anonymous_boxes() {
    let (dom, sheet) = parse(&format!("{BODY}<div style=\"display:table\"><p>TAGGED</p></div>"));
    let l = layout(&dom, &sheet, geometry());
    let pdf = render_with_options(&l, Some(&dom), &DocumentMetadata::default(), true, false);
    assert!(pdf.is_ok(), "tagged render failed: {:?}", pdf.err());
}
