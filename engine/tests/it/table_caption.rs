//! Table captions (CORE-351) — `docs/specifications/tables-fragmentation.spec.md`
//! §Behavior 19 and Acceptance Criterion 13.
//!
//! A `table-caption` child of a table is a block in the table wrapper box,
//! outside the grid: top captions above it, bottom captions below it, in
//! document order, at the table's width, outside the table's own border, and
//! a top caption stays with the first row across a page break.

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::frag::{Fragment, FragmentContent};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::{layout, Layout};

fn inches(v: f64) -> Scalar {
    Scalar(v * 72.0)
}

/// The corpus geometry the CORE-349 triage reproduced on: 5in × 3in page,
/// 0.5in margins (144pt × 288pt content box).
fn corpus_geometry() -> PageGeometry {
    PageGeometry {
        width: inches(5.0),
        height: inches(3.0),
        margin_top: inches(0.5),
        margin_right: inches(0.5),
        margin_bottom: inches(0.5),
        margin_left: inches(0.5),
    }
}

fn lay(html: &str) -> Layout {
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
    let sheet = Stylesheet::parse(&css);
    layout(&dom, &sheet, corpus_geometry())
}

/// Every fragment on a page with its absolute (x, y).
fn page_fragments(l: &Layout, page: usize) -> Vec<(&Fragment, f32, f32)> {
    fn rec<'a>(f: &'a Fragment, px: f32, py: f32, out: &mut Vec<(&'a Fragment, f32, f32)>) {
        let x = px + f.offset.x.to_f32();
        let y = py + f.offset.y.to_f32();
        out.push((f, x, y));
        for c in &f.children {
            rec(c, x, y, out);
        }
    }
    let mut out = Vec::new();
    if let Some(p) = l.pages.get(page) {
        rec(&p.root, 0.0, 0.0, &mut out);
    }
    out
}

/// Absolute (x, y, width) of the first text run containing `needle` on
/// `page`, or None.
fn find_text(l: &Layout, page: usize, needle: &str) -> Option<(f32, f32, f32)> {
    page_fragments(l, page).into_iter().find_map(|(f, x, y)| match &f.content {
        FragmentContent::Text(run) if run.text.contains(needle) => Some((x, y, f.size.0.to_f32())),
        _ => None,
    })
}

/// The page index holding a text run containing `needle`.
fn page_of(l: &Layout, needle: &str) -> Option<usize> {
    (0..l.pages.len()).find(|&p| find_text(l, p, needle).is_some())
}

const BASE_CSS: &str = "body { margin: 0; font-family: Arial, sans-serif; font-size: 9pt; } \
     table { border-collapse: collapse; width: 100%; } \
     th, td { padding: 2pt 4pt; }";

#[test]
fn caption_renders_above_the_first_row() {
    let html = format!(
        "<html><head><style>{BASE_CSS}</style></head><body>\
         <table><caption>CaptionText</caption>\
         <thead><tr><th>HeadA</th><th>HeadB</th></tr></thead>\
         <tbody><tr><td>row1</td><td>1</td></tr></tbody></table>\
         <p>After table.</p></body></html>"
    );
    let l = lay(&html);
    let cap = find_text(&l, 0, "CaptionText").expect("caption text is laid out on page 1");
    let head = find_text(&l, 0, "HeadA").expect("header row present");
    let after = find_text(&l, 0, "After").expect("following paragraph present");
    assert!(cap.1 < head.1, "caption y {} must be above the header row y {}", cap.1, head.1);
    assert!(head.1 < after.1, "table content precedes the following paragraph");
}

#[test]
fn caption_side_bottom_renders_below_the_last_row() {
    let html = format!(
        "<html><head><style>{BASE_CSS} caption {{ caption-side: bottom; }}</style></head><body>\
         <table><caption>CaptionText</caption>\
         <tr><td>row1</td></tr><tr><td>row2</td></tr></table>\
         <p>After table.</p></body></html>"
    );
    let l = lay(&html);
    let cap = find_text(&l, 0, "CaptionText").expect("bottom caption is laid out");
    let last = find_text(&l, 0, "row2").expect("last row present");
    let after = find_text(&l, 0, "After").expect("following paragraph present");
    assert!(cap.1 > last.1, "bottom caption y {} must be below the last row y {}", cap.1, last.1);
    assert!(cap.1 < after.1, "bottom caption precedes the following paragraph");
}

#[test]
fn captions_in_mixed_dom_order_place_tops_then_grid_then_bottoms() {
    // sections-and-captions-mixed-order (WPT css-break/table) in miniature.
    let html = format!(
        "<html><head><style>{BASE_CSS} .b {{ caption-side: bottom; }}</style></head><body>\
         <table>\
         <caption class=\"b\">BottomOne</caption>\
         <caption>TopOne</caption>\
         <tr><td>GridRow</td></tr>\
         <caption class=\"b\">BottomTwo</caption>\
         <caption>TopTwo</caption>\
         </table></body></html>"
    );
    let l = lay(&html);
    let y = |s: &str| find_text(&l, 0, s).unwrap_or_else(|| panic!("{s} laid out")).1;
    let order = [y("TopOne"), y("TopTwo"), y("GridRow"), y("BottomOne"), y("BottomTwo")];
    assert!(
        order.windows(2).all(|w| w[0] < w[1]),
        "expected TopOne < TopTwo < GridRow < BottomOne < BottomTwo, got {order:?}"
    );
}

#[test]
fn caption_sits_outside_the_table_border_and_within_its_width() {
    // A 2in table with a 2pt border and a caption long enough to wrap: the
    // border box starts below the caption, and every caption line stays
    // inside the table's 2in width (caption width = table width).
    let html = "<html><head><style>body { margin: 0; font-size: 9pt; } \
         table { width: 2in; border: 2pt solid black; border-collapse: collapse; } \
         caption { text-align: left; }</style></head><body>\
         <table><caption>Wrapword wrapword wrapword wrapword wrapword wrapword wrapword</caption>\
         <tr><td>cell</td></tr></table></body></html>";
    let l = lay(html);
    let frags = page_fragments(&l, 0);
    let cap_lines: Vec<(f32, f32, f32)> = frags
        .iter()
        .filter_map(|(f, x, y)| match &f.content {
            FragmentContent::Text(run) if run.text.contains("rapword") => Some((*x, *y, f.size.0.to_f32())),
            _ => None,
        })
        .collect();
    assert!(cap_lines.len() >= 2, "a long caption wraps at the 2in table width, got {} line(s)", cap_lines.len());
    let left = 36.0_f32; // 0.5in page margin
    for (x, _, w) in &cap_lines {
        assert!(*x >= left - 0.5 && x + w <= left + 144.0 + 0.5, "caption line [{x}, {}] escapes the 2in table", x + w);
    }
    let cap_bottom = cap_lines.iter().map(|(_, y, _)| *y).fold(f32::MIN, f32::max);
    let border_top = frags
        .iter()
        .filter(|(f, _, _)| matches!(f.content, FragmentContent::Border(_)))
        .filter(|(f, _, _)| (f.size.0.to_f32() - 144.0).abs() < 1.0)
        .map(|(_, _, y)| *y)
        .fold(f32::MAX, f32::min);
    assert!(border_top < f32::MAX, "the table's own 2in border box is painted");
    assert!(border_top > cap_bottom, "table border top {border_top} must be below the caption's last line {cap_bottom}");
}

#[test]
fn top_caption_moves_to_the_next_page_with_the_first_row() {
    // table-caption-and-cells (WPT css-break/table) in paged form: the 125pt
    // filler leaves 19pt — room for the ~11pt caption line but not the
    // three-line (~37pt) row. The caption must not be stranded at the page
    // bottom; it moves with the row.
    let html = format!(
        "<html><head><style>{BASE_CSS}</style></head><body>\
         <div style=\"height: 125pt\"></div>\
         <table><caption>KeepCaption</caption><tr><td>KeepRow<br>two<br>three</td></tr></table>\
         </body></html>"
    );
    let l = lay(&html);
    let cap_page = page_of(&l, "KeepCaption").expect("caption laid out");
    let row_page = page_of(&l, "KeepRow").expect("row laid out");
    assert_eq!(cap_page, row_page, "caption (page {cap_page}) must share the first row's page ({row_page})");
    assert_eq!(row_page, 1, "the table moves to page 2");
}

#[test]
fn tall_top_caption_fragments_and_the_grid_follows() {
    // A caption taller than the page continues on the next page; the grid
    // follows the caption's last line and nothing is dropped.
    let words: String = (0..400).map(|i| format!("cw{i} ")).collect();
    let html = format!(
        "<html><head><style>{BASE_CSS}</style></head><body>\
         <table><caption>{words}</caption><tr><td>AfterCaptionRow</td></tr></table>\
         </body></html>"
    );
    let l = lay(&html);
    let first = page_of(&l, "cw0 ").or_else(|| page_of(&l, "cw0")).expect("caption start laid out");
    let last = page_of(&l, "cw399").expect("caption end laid out");
    let row = page_of(&l, "AfterCaptionRow").expect("row laid out after the long caption");
    assert_eq!(first, 0);
    assert!(last > first, "a caption taller than a page fragments (last line on page {last})");
    assert!(row >= last, "the grid follows the caption (row page {row}, caption end page {last})");
    if row == last {
        assert!(find_text(&l, row, "AfterCaptionRow").unwrap().1 > find_text(&l, last, "cw399").unwrap().1);
    }
}

#[test]
fn caption_centers_over_an_auto_width_table() {
    // An auto-width table narrower than the page: the centered caption
    // centers over the TABLE (the wrapper is as wide as the grid), not over
    // the page content box (page-name-table-001-print shape).
    let html = "<html><head><style>body { margin: 0; font-size: 9pt; } \
         td { text-align: center; padding: 2pt; }</style></head><body>\
         <table><caption>Cap</caption><tr><td>NarrowCellText</td></tr></table></body></html>";
    let l = lay(html);
    let (cx, _, cw) = find_text(&l, 0, "Cap").expect("caption laid out");
    let (tx, _, tw) = find_text(&l, 0, "NarrowCellText").expect("cell laid out");
    let cap_mid = cx + cw / 2.0;
    let cell_mid = tx + tw / 2.0;
    let page_mid = 36.0 + 144.0;
    assert!(cell_mid < page_mid - 20.0, "fixture: the auto table is narrower than the page (cell mid {cell_mid})");
    assert!((cap_mid - cell_mid).abs() < 3.0, "caption mid {cap_mid} must center over the table (cell mid {cell_mid})");
}

#[test]
fn caption_outside_a_table_lays_out_as_a_block() {
    // A bare table-caption with no table parent: ordinary block, no panic.
    let html = "<html><head><style>body { margin: 0; }</style></head><body>\
         <div style=\"display: table-caption\">LoneCaption</div><p>Next</p></body></html>";
    let l = lay(html);
    let cap = find_text(&l, 0, "LoneCaption").expect("lone caption renders as a block");
    let next = find_text(&l, 0, "Next").expect("next paragraph renders");
    assert!(cap.1 < next.1);
}
