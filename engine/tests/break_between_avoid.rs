//! Between-sibling break avoidance (CORE-365) — one test per acceptance
//! criterion in `docs/specifications/fragmentation-core.spec.md`, Addendum
//! "Between-Sibling Break Avoidance".
//!
//! Geometry: 5in × 3in page, 0.5in margins → a 288pt × 144pt content box.
//! Every fixture uses 12pt lines and zero margins, so a 132pt filler leaves
//! room for exactly ONE more line on page 1: the heading lands on the page's
//! last line and its paragraph would start page 2.

use typeanvil::css::{cascade, Stylesheet};
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::frag::{BreakBetween, Fragment, FragmentContent};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::{layout, Layout};

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

fn style_text(dom: &Dom) -> String {
    let mut css = String::new();
    for (id, node) in dom.nodes.iter().enumerate() {
        if let NodeKind::Element(el) = &node.kind {
            if el.tag == "style" {
                css.push_str(&dom.text_content(id));
                css.push('\n');
            }
        }
    }
    css
}

fn lay(html: &str) -> Layout {
    let dom = Dom::parse(html).expect("parse html");
    let sheet = Stylesheet::parse(&style_text(&dom));
    layout(&dom, &sheet, geometry())
}

fn collect_text(frag: &Fragment, out: &mut Vec<String>) {
    if let FragmentContent::Text(run) = &frag.content {
        out.push(run.text.clone());
    }
    for c in &frag.children {
        collect_text(c, out);
    }
}

/// Text of every page, one joined string per page.
fn pages(layout: &Layout) -> Vec<String> {
    layout
        .pages
        .iter()
        .map(|p| {
            let mut out = Vec::new();
            collect_text(&p.root, &mut out);
            out.join(" ")
        })
        .collect()
}

const BASE_CSS: &str = "html, body { margin: 0; } \
    body { font-size: 12pt; line-height: 12pt; } \
    h1, h2, p, div { margin: 0; padding: 0; font-size: 12pt; line-height: 12pt; } \
    .fill { height: 132pt; }";

/// Filler, a one-line heading, a one-line paragraph; `css` adds the rules
/// under test.
fn heading_doc(css: &str) -> String {
    format!(
        "<html><head><style>{BASE_CSS} {css}</style></head><body>\
         <div class=\"fill\">FILL</div>\
         <h2>HEADING</h2>\
         <p>PARA text.</p>\
         </body></html>"
    )
}

/// Page 1 holds only the filler; page 2 holds the heading, then the
/// paragraph.
fn assert_heading_moved(got: &[String], what: &str) {
    assert_eq!(got.len(), 2, "{what}: expected two pages, got {got:?}");
    assert!(got[0].contains("FILL"), "{what}: page 1 keeps the filler: {got:?}");
    assert!(
        !got[0].contains("HEADING"),
        "{what}: the heading must not strand on page 1: {got:?}"
    );
    let h = got[1].find("HEADING");
    let p = got[1].find("PARA");
    assert!(
        matches!((h, p), (Some(h), Some(p)) if h < p),
        "{what}: page 2 holds the heading, then its paragraph: {got:?}"
    );
}

// --- AC1 ------------------------------------------------------------------

#[test]
fn avoid_after_keeps_heading_with_paragraph() {
    // Sanity: without avoid, the heading strands on page 1 (the bug's
    // natural break point), so the fixture exercises the boundary.
    let control = pages(&lay(&heading_doc("")));
    assert_eq!(control.len(), 2, "control: two pages, got {control:?}");
    assert!(
        control[0].contains("HEADING") && !control[0].contains("PARA"),
        "control: heading on page 1, paragraph on page 2: {control:?}"
    );

    let got = pages(&lay(&heading_doc("h2 { break-after: avoid; }")));
    assert_heading_moved(&got, "break-after: avoid");
}

// --- AC2 ------------------------------------------------------------------

#[test]
fn legacy_page_break_after_avoid_matches() {
    let modern = pages(&lay(&heading_doc("h2 { break-after: avoid; }")));
    let legacy = pages(&lay(&heading_doc("h2 { page-break-after: avoid; }")));
    assert_heading_moved(&legacy, "page-break-after: avoid");
    assert_eq!(legacy, modern, "legacy alias renders the same pages");
}

// --- AC3 ------------------------------------------------------------------

#[test]
fn avoid_before_on_following_sibling() {
    let got = pages(&lay(&heading_doc("p { break-before: avoid; }")));
    assert_heading_moved(&got, "p { break-before: avoid }");
}

// --- AC4 ------------------------------------------------------------------

#[test]
fn avoid_chain_moves_every_heading() {
    // 120pt filler: h1 lands at 120, h2 at 132 (the page's last line), and
    // the paragraph would start page 2. Both headings move with it.
    let html = format!(
        "<html><head><style>{BASE_CSS} .fill {{ height: 120pt; }} \
         h1, h2 {{ break-after: avoid; }}</style></head><body>\
         <div class=\"fill\">FILL</div>\
         <h1>TITLE</h1>\
         <h2>HEADING</h2>\
         <p>PARA text.</p>\
         </body></html>"
    );
    let got = pages(&lay(&html));
    assert_eq!(got.len(), 2, "two pages, got {got:?}");
    assert!(
        !got[0].contains("TITLE") && !got[0].contains("HEADING"),
        "neither heading strands on page 1: {got:?}"
    );
    let t = got[1].find("TITLE");
    let h = got[1].find("HEADING");
    let p = got[1].find("PARA");
    assert!(
        matches!((t, h, p), (Some(t), Some(h), Some(p)) if t < h && h < p),
        "page 2 holds title, heading, paragraph in order: {got:?}"
    );
}

// --- AC5 ------------------------------------------------------------------

#[test]
fn avoid_without_earlier_break_breaks_anyway() {
    // The heading is the first thing on page 1 and the next sibling (a
    // 12-line `break-inside: avoid` block, 144pt: fits a fresh page, not the
    // 132pt left) defers whole. No earlier break exists, so the heading stays
    // and the box moves.
    let lines: String = (0..12).map(|i| format!("<p>BOX{i}</p>")).collect();
    let html = format!(
        "<html><head><style>{BASE_CSS} h2 {{ break-after: avoid; }} \
         .box {{ break-inside: avoid; }}</style></head><body>\
         <h2>HEADING</h2>\
         <div class=\"box\">{lines}</div>\
         </body></html>"
    );
    let got = pages(&lay(&html));
    assert_eq!(got.len(), 2, "two pages (terminates), got {got:?}");
    assert!(got[0].contains("HEADING"), "heading stays on page 1: {got:?}");
    assert!(
        !got[0].contains("BOX") && got[1].contains("BOX"),
        "the box moves to page 2: {got:?}"
    );
}

// --- AC6 ------------------------------------------------------------------

#[test]
fn forced_break_wins_over_avoid() {
    let html = format!(
        "<html><head><style>{BASE_CSS} h2 {{ break-after: avoid; }} \
         p {{ break-before: page; }}</style></head><body>\
         <h2>HEADING</h2>\
         <p>PARA text.</p>\
         </body></html>"
    );
    let got = pages(&lay(&html));
    assert_eq!(got.len(), 2, "forced break makes two pages, got {got:?}");
    assert!(
        got[0].contains("HEADING") && !got[0].contains("PARA"),
        "heading on page 1: {got:?}"
    );
    assert!(got[1].contains("PARA"), "paragraph on page 2: {got:?}");
}

// --- AC6b -----------------------------------------------------------------

#[test]
fn column_break_value_defeats_avoid() {
    // `break-after: column` is a forced break value (css-break-3 §3.1).
    // v1 does not break for it outside a multicol, but it still wins over
    // the next sibling's `break-before: avoid`: the boundary is not an
    // avoid boundary, so the heading is NOT pulled to page 2 (the natural
    // break stands, as in the control).
    let got = pages(&lay(&heading_doc(
        "h2 { break-after: column; } p { break-before: avoid; }",
    )));
    assert_eq!(got.len(), 2, "two pages, got {got:?}");
    assert!(
        got[0].contains("HEADING") && !got[0].contains("PARA"),
        "a forced value beats avoid: heading stays on page 1: {got:?}"
    );
}

// --- AC8 ------------------------------------------------------------------

#[test]
fn avoid_is_not_forced() {
    for (prop, value) in [
        ("break-after", "avoid"),
        ("break-after", "avoid-page"),
        ("break-after", "avoid-column"),
        ("page-break-after", "avoid"),
        ("break-before", "avoid"),
        ("page-break-before", "avoid"),
    ] {
        let html = format!(
            "<html><head><style>h2 {{ {prop}: {value}; }}</style></head>\
             <body><h2>x</h2></body></html>"
        );
        let dom = Dom::parse(&html).expect("parse html");
        let sheet = Stylesheet::parse(&style_text(&dom));
        let styles = cascade(&dom, &sheet, &geometry());
        let h2 = dom.find_tag("h2").expect("h2 exists");
        let v = if prop.ends_with("before") {
            styles[h2].break_before
        } else {
            styles[h2].break_after
        };
        assert_eq!(v, BreakBetween::Avoid, "{prop}: {value} computes to Avoid");
        assert!(!v.is_forced(), "{prop}: {value} is not a forced break");
    }
}

// --- CORE-369: a table as the next sibling ---------------------------------

fn table_doc(table: &str) -> String {
    format!(
        "<html><head><style>{BASE_CSS} h2 {{ break-after: avoid; }} \
         table {{ border-collapse: collapse; }} td, th, caption {{ padding: 0; font-size: 12pt; line-height: 12pt; }}\
         </style></head><body>\
         <div class=\"fill\">FILL</div><h2>HEADING</h2>{table}</body></html>"
    )
}

fn assert_heading_with_table(table: &str, what: &str) {
    let got = pages(&lay(&table_doc(table)));
    assert_eq!(got.len(), 2, "{what}: two pages, got {got:?}");
    assert!(
        got[0].contains("FILL") && !got[0].contains("HEADING"),
        "{what}: heading must not strand on page 1: {got:?}"
    );
    assert!(
        got[1].contains("HEADING") && got[1].contains("ROW1"),
        "{what}: page 2 holds heading and table: {got:?}"
    );
}

#[test]
fn avoid_after_moves_heading_with_table() {
    assert_heading_with_table(
        "<table><thead><tr><th>HEAD</th></tr></thead><tbody><tr><td>ROW1</td></tr></tbody></table>",
        "table with thead",
    );
}

#[test]
fn avoid_after_moves_heading_with_headerless_table() {
    assert_heading_with_table("<table><tbody><tr><td>ROW1</td></tr></tbody></table>", "headerless table");
}

#[test]
fn avoid_after_moves_heading_with_captioned_table() {
    assert_heading_with_table(
        "<table><caption>CAP</caption><tbody><tr><td>ROW1</td></tr></tbody></table>",
        "captioned table",
    );
}
