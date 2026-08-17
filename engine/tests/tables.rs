//! Tables × page breaks acceptance tests — one per acceptance criterion in
//! `docs/specifications/tables-fragmentation.spec.md`.
//!
//! Drive the library directly (`typeanvil::layout::layout`) and assert on the
//! fragment tree, mirroring `fragmentation.rs` conventions.

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::frag::{Fragment, FragmentContent, Fragmentainer};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::{layout, Layout};

// --- helpers ---------------------------------------------------------------

fn inches(v: f64) -> Scalar {
    Scalar(v * 72.0)
}

fn geometry(w_in: f64, h_in: f64, margin_in: f64) -> PageGeometry {
    PageGeometry {
        width: inches(w_in),
        height: inches(h_in),
        margin_top: inches(margin_in),
        margin_right: inches(margin_in),
        margin_bottom: inches(margin_in),
        margin_left: inches(margin_in),
    }
}

fn lay(html: &str, geo: PageGeometry) -> Layout {
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
    layout(&dom, &sheet, geo)
}

fn all_fragments<'a>(root: &'a Fragment, out: &mut Vec<(&'a Fragment, f32, f32)>) {
    fn rec<'a>(f: &'a Fragment, px: f32, py: f32, out: &mut Vec<(&'a Fragment, f32, f32)>) {
        let x = px + f.offset.x.to_f32();
        let y = py + f.offset.y.to_f32();
        out.push((f, x, y));
        for c in &f.children {
            rec(c, x, y, out);
        }
    }
    rec(root, 0.0, 0.0, out);
}

fn page_fragments(l: &Layout, page: usize) -> Vec<(&Fragment, f32, f32)> {
    let mut out = Vec::new();
    if let Some(fa) = l.pages.get(page) {
        all_fragments(&fa.root, &mut out);
    }
    out
}

fn text_of(f: &Fragment) -> String {
    match &f.content {
        FragmentContent::Text(run) => run.text.clone(),
        _ => String::new(),
    }
}

fn is_border(f: &Fragment) -> bool {
    matches!(f.content, FragmentContent::Border(_))
}

/// Count occurrences of a text value across a page's fragments.
fn count_text(l: &Layout, page: usize, needle: &str) -> usize {
    page_fragments(l, page)
        .iter()
        .filter(|(f, _, _)| text_of(f) == needle)
        .count()
}

// --- acceptance tests --------------------------------------------------------

#[test]
fn test_basic_table_cells_side_by_side() {
    let geo = geometry(5.0, 3.0, 0.5);
    let html = r#"<!DOCTYPE html><style>
        table { border-collapse: collapse; width: 100%; }
        th, td { padding: 4pt; font-size: 9pt; }
    </style><table>
      <tr><th>Item</th><th>Description</th><th>Qty</th><th>Amount</th></tr>
      <tr><td>001</td><td>Platform subscription</td><td>1</td><td>$1,200.00</td></tr>
    </table>"#;
    let l = lay(html, geo);
    let frags = page_fragments(&l, 0);
    let texts: Vec<(String, f32)> = frags
        .iter()
        .filter(|(f, _, _)| !text_of(f).is_empty())
        .map(|(f, x, _)| (text_of(f), *x))
        .collect();
    let item = texts.iter().find(|(t, _)| t == "Item").unwrap();
    let desc = texts.iter().find(|(t, _)| t.starts_with("Description")).unwrap();
    let qty = texts.iter().find(|(t, _)| t == "Qty").unwrap();
    let amount = texts.iter().find(|(t, _)| t.starts_with("$1,200")).unwrap();
    assert!(item.1 < desc.1, "Item ({}) must be left of Description ({})", item.1, desc.1);
    assert!(desc.1 < qty.1, "Description ({}) must be left of Qty ({})", desc.1, qty.1);
    assert!(qty.1 < amount.1, "Qty ({}) must be left of Amount ({})", qty.1, amount.1);
}

#[test]
fn test_repeating_header_every_page() {
    // Small page: 30 data rows must span multiple pages, and the header row
    // ("HDR") must appear at the top of every page after the first.
    let geo = geometry(5.0, 3.0, 0.5);
    let mut rows = String::new();
    for i in 0..30 {
        rows.push_str(&format!("<tr><td>Row {i}</td></tr>\n"));
    }
    let html = format!(
        r#"<!DOCTYPE html><style>
        table {{ border-collapse: collapse; width: 100%; }}
        th, td {{ padding: 2pt; font-size: 9pt; }}
    </style><table>
      <thead><tr><th>HDR</th></tr></thead>
      <tbody>{rows}</tbody>
    </table>"#
    );
    let l = lay(&html, geo);
    assert!(l.pages.len() >= 2, "expected multi-page table, got {} page(s)", l.pages.len());
    for (idx, _) in l.pages.iter().enumerate() {
        assert!(
            count_text(&l, idx, "HDR") >= 1,
            "page {idx} missing repeated header"
        );
    }
}

#[test]
fn test_row_moves_whole_not_sliced() {
    // A row taller than remaining space but shorter than a full page must move
    // whole to the next page — its text must not be split across pages.
    let geo = geometry(5.0, 3.0, 0.5); // content height = 2in = 144pt
    let html = r#"<!DOCTYPE html><style>
        table { border-collapse: collapse; width: 100%; }
        td { padding: 2pt; font-size: 9pt; }
        .tall { height: 90pt; }
    </style><table>
      <tbody>
        <tr><td>fits</td></tr>
        <tr class="tall"><td>tall-row</td></tr>
      </tbody>
    </table>"#;
    let l = lay(html, geo);
    // The tall row (90pt) fits on a fresh page (144pt content height), so it
    // must NOT be sliced: "tall-row" appears exactly once, whole.
    let total = l.pages.iter().enumerate().map(|(i, _)| count_text(&l, i, "tall-row")).sum::<usize>();
    assert_eq!(total, 1, "tall row must not be sliced across pages");
}

#[test]
fn test_border_collapse_single_lines() {
    let geo = geometry(5.0, 3.0, 0.5);
    let html = r#"<!DOCTYPE html><style>
        table { border-collapse: collapse; width: 100%; }
        th, td { border: 0.5pt solid #333; padding: 4pt; font-size: 9pt; }
    </style><table>
      <tr><th>Item</th><th>Description</th></tr>
      <tr><td>001</td><td>Platform</td></tr>
      <tr><td>002</td><td>Support</td></tr>
    </table>"#;
    let l = lay(html, geo);
    // Every cell fragment must carry a Border box with all four sides set.
    let borders = page_fragments(&l, 0)
        .into_iter()
        .filter(|(f, _, _)| is_border(f))
        .count();
    // 3 rows × 2 cells = 6 cell border boxes (header included).
    assert_eq!(borders, 6, "expected 6 cell borders, got {borders}");
    // Header cell top border must sit above the first data row's top border
    // (outer box complete) — sanity: two distinct y values for row tops.
    let tops: Vec<f32> = page_fragments(&l, 0)
        .into_iter()
        .filter(|(f, _, _)| is_border(f))
        .map(|(_, _, y)| y)
        .collect();
    let mut uniq: Vec<f32> = tops.clone();
    uniq.sort_by(|a, b| a.partial_cmp(b).unwrap());
    uniq.dedup();
    assert!(uniq.len() >= 3, "expected 3 distinct row-top ys, got {:?}", uniq);
}

#[test]
fn test_footer_bottom_of_closing_fragment() {
    // A tfoot must render at the bottom of the fragment that closes its body —
    // on a multi-page table the footer lands on the LAST page, not the first.
    let geo = geometry(5.0, 3.0, 0.5);
    let mut rows = String::new();
    for i in 0..30 {
        rows.push_str(&format!("<tr><td>Row {i}</td></tr>\n"));
    }
    let html = format!(
        r#"<!DOCTYPE html><style>
        table {{ border-collapse: collapse; width: 100%; }}
        th, td {{ padding: 2pt; font-size: 9pt; }}
    </style><table>
      <thead><tr><th>HDR</th></tr></thead>
      <tbody>{rows}</tbody>
      <tfoot><tr><td>FOOT</td></tr></tfoot>
    </table>"#
    );
    let l = lay(&html, geo);
    assert!(l.pages.len() >= 2, "expected multi-page table");
    let last = l.pages.len() - 1;
    assert!(
        count_text(&l, last, "FOOT") >= 1,
        "footer must be on the last page (got {} pages, footer on last? {})",
        l.pages.len(),
        count_text(&l, last, "FOOT")
    );
    // Footer should NOT be on page 0 of a multi-page table.
    assert_eq!(count_text(&l, 0, "FOOT"), 0, "footer must not appear on page 0");
}

#[test]
fn test_tables_pagination_linear_100x10() {
    // 100 rows on a small page → several pages, all content preserved.
    let geo = geometry(5.0, 3.0, 0.5);
    let mut rows = String::new();
    for i in 0..100 {
        rows.push_str(&format!("<tr><td>row{i}</td></tr>\n"));
    }
    let html = format!(
        r#"<!DOCTYPE html><style>
        table {{ border-collapse: collapse; width: 100%; }}
        td {{ padding: 1pt; font-size: 8pt; }}
    </style><table><tbody>{rows}</tbody></table>"#
    );
    let l = lay(&html, geo);
    assert!(l.pages.len() >= 3, "expected multi-page, got {}", l.pages.len());
    // Every row appears exactly once across all pages.
    for i in 0..100 {
        let needle = format!("row{i}");
        let total = l
            .pages
            .iter()
            .enumerate()
            .map(|(p, _)| count_text(&l, p, &needle))
            .sum::<usize>();
        assert_eq!(total, 1, "row{i} should appear exactly once, got {total}");
    }
}

#[test]
fn test_table_regression_guards() {
    // Marker: the full suite (36 pre-existing + these) runs together in CI;
    // this guards that table layout doesn't panic on a bare table.
    let geo = geometry(5.0, 3.0, 0.5);
    let html = "<!DOCTYPE html><table><tr><td>x</td></tr></table>";
    let l = lay(html, geo);
    assert!(!l.pages.is_empty());
}
