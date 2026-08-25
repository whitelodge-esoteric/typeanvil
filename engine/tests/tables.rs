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
fn test_footer_repeats_every_fragment() {
    // Spec rule 8 (tables-fragmentation): a tfoot renders at the bottom of
    // the fragment that closes its body AND repeats on every continuation
    // fragment that still has body rows below it. Prince 16.2 repeats the
    // tfoot on every page (verified 2026-08-20: table-stress pages 1-44,
    // invoice pages 1-4).
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
    // The footer repeats on EVERY fragment (spec rule 8), so page 0 carries
    // it too — unlike the pre-CORE-96 behavior where it rendered only once
    // at the end of the table.
    for pi in 0..l.pages.len() {
        assert!(
            count_text(&l, pi, "FOOT") >= 1,
            "footer must repeat on every fragment (page {pi} has {})",
            count_text(&l, pi, "FOOT")
        );
    }
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

// --- CORE-109 giant-row pagination deadlock ----------------------------------

#[test]
fn test_giant_row_defers_at_most_once() {
    // A row that fits a FRESH fragmentainer but not one shrunk by the
    // repeating header must defer once, then place. Pre-fix this re-created
    // the same break-before token every page and paginated to MAX_PAGES.
    let geo = geometry(3.0, 3.0, 0.5); // content height = 2in = 144pt
    let html = r#"<!DOCTYPE html><style>
        table { border-collapse: collapse; width: 100%; }
        th, td { padding: 2pt; font-size: 9pt; }
        /* Row height = cell content + padding (CSS height is not applied to
           rows), so a giant row is built with padding-bottom. */
        .giant { padding-bottom: 125pt; }
    </style><table>
      <thead><tr><th>HDR</th></tr></thead>
      <tbody>
        <tr><td class="giant">giant-row</td></tr>
        <tr><td>after-row</td></tr>
      </tbody>
    </table>"#;
    let l = lay(html, geo);
    // Bounded pagination: the deadlock produced tens of thousands of pages.
    assert!(
        l.pages.len() <= 6,
        "pagination must terminate quickly, got {} pages",
        l.pages.len()
    );
    // The giant row is placed exactly once, whole (never sliced).
    let total = l
        .pages
        .iter()
        .enumerate()
        .map(|(p, _)| count_text(&l, p, "giant-row"))
        .sum::<usize>();
    assert_eq!(total, 1, "giant row must appear exactly once, got {total}");
    // It cannot share page 0 with the header (130pt row + header > 144pt),
    // so it must appear on a LATER page than the first.
    let page_of = (0..l.pages.len())
        .find(|p| count_text(&l, *p, "giant-row") == 1)
        .expect("giant row placed somewhere");
    assert!(page_of >= 1, "giant row must defer past the first page");
    // Content after the giant row survives.
    let after = l
        .pages
        .iter()
        .enumerate()
        .map(|(p, _)| count_text(&l, p, "after-row"))
        .sum::<usize>();
    assert_eq!(after, 1, "content after the giant row must survive");
}

#[test]
fn test_row_taller_than_page_still_places_monolithically() {
    // A row taller than even a FULL fragmentainer bypasses the defer path
    // (monolithic overflow, tables-fragmentation rule 11) and must not hang.
    let geo = geometry(3.0, 3.0, 0.5); // content height = 144pt
    let html = r#"<!DOCTYPE html><style>
        table { border-collapse: collapse; width: 100%; }
        th, td { padding: 2pt; font-size: 9pt; }
        .huge { padding-bottom: 290pt; }
    </style><table>
      <thead><tr><th>HDR</th></tr></thead>
      <tbody>
        <tr><td class="huge">huge-row</td></tr>
        <tr><td>after-row</td></tr>
      </tbody>
    </table>"#;
    let l = lay(html, geo);
    assert!(
        l.pages.len() <= 6,
        "pagination must terminate quickly, got {} pages",
        l.pages.len()
    );
    let total = l
        .pages
        .iter()
        .enumerate()
        .map(|(p, _)| count_text(&l, p, "huge-row"))
        .sum::<usize>();
    assert_eq!(total, 1, "oversized row must appear exactly once");
}

// --- CORE-81 auto table layout acceptance tests ------------------------------
// Each maps to an acceptance criterion in
// `docs/specifications/auto-table-layout.spec.md`.

use typeanvil::table::{
    cell_colspan, distribute_column_widths, intrinsic_column_widths, measure_columns,
    measure_columns_scoped, measure_rows, resolve_freeze_scope, MeasureScope,
};

/// Parse fixture + cascade styles, for the direct table API tests.
fn parse(html: &str) -> (Dom, Vec<typeanvil::css::ComputedStyle>) {
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
    let styles = typeanvil::css::cascade(&dom, &sheet, &geometry(5.0, 3.0, 0.5));
    (dom, styles)
}

fn table_id_of(dom: &Dom) -> usize {
    dom.nodes
        .iter()
        .enumerate()
        .find(|(_, n)| matches!(&n.kind, NodeKind::Element(el) if el.tag == "table"))
        .map(|(i, _)| i)
        .expect("table element")
}

#[test]
fn test_intrinsic_min_max_uncapped() {
    let html = r#"<!DOCTYPE html><style>
        table { border-collapse: collapse; }
        th, td { padding: 2pt 4pt; font-size: 9pt; font-family: Arial, sans-serif; }
    </style><table><tr><td>Disappearing/reappearing clothes</td><td>Qty</td></tr></table>"#;
    let (dom, styles) = parse(html);
    let tid = table_id_of(&dom);
    let (mins, maxs) = intrinsic_column_widths(&dom, &styles, tid, MeasureScope::All);
    assert_eq!(mins.len(), 2);
    // min-content = widest whitespace word ("Disappearing/reappearing", one
    // glued token ≈ 112pt at 9pt Arial) + padding.
    assert!(
        mins[0].get() > 100.0 && mins[0].get() < 135.0,
        "min-content col0 = {} (expect ≈112)",
        mins[0].get()
    );
    // max-content = whole text on one line (≈145pt) + padding; strictly
    // larger than min, and far below the available width (uncapped).
    assert!(
        maxs[0].get() > mins[0].get(),
        "max-content ({}) must exceed min-content ({})",
        maxs[0].get(),
        mins[0].get()
    );
    assert!(maxs[0].get() < 200.0, "max-content col0 = {}", maxs[0].get());
}

#[test]
fn test_distribute_extra_proportional_max() {
    let min = [Scalar(10.0), Scalar(20.0)];
    let max = [Scalar(30.0), Scalar(40.0)];
    // used=100 ≥ sum(max)=70 → col = max + extra×max/sum(max), extra=30.
    let out = distribute_column_widths(&min, &max, Scalar(100.0));
    assert!((out[0].get() - 42.857).abs() < 0.01, "col0 = {}", out[0].get());
    assert!((out[1].get() - 57.143).abs() < 0.01, "col1 = {}", out[1].get());
    assert!((out[0].get() + out[1].get() - 100.0).abs() < 0.01);
}

#[test]
fn test_distribute_middle_branch() {
    let min = [Scalar(10.0), Scalar(10.0), Scalar(10.0)];
    let max = [Scalar(20.0), Scalar(100.0), Scalar(30.0)];
    // sum(min)=30, sum(max)=150; used=90 → extra=60 ∝ (max−min)=(10,90,20).
    let out = distribute_column_widths(&min, &max, Scalar(90.0));
    assert!((out[0].get() - 15.0).abs() < 0.01, "col0 = {}", out[0].get());
    assert!((out[1].get() - 55.0).abs() < 0.01, "col1 = {}", out[1].get());
    assert!((out[2].get() - 20.0).abs() < 0.01, "col2 = {}", out[2].get());
    assert!((out.iter().map(|w| w.get()).sum::<f64>() - 90.0).abs() < 0.01);
}

#[test]
fn test_distribute_overflow_keeps_min() {
    let min = [Scalar(10.0), Scalar(20.0)];
    let max = [Scalar(100.0), Scalar(200.0)];
    let out = distribute_column_widths(&min, &max, Scalar(25.0));
    assert_eq!(out, min.to_vec());
}

#[test]
fn test_table_used_width_resolution() {
    let html = r#"<!DOCTYPE html><style>
        table { border-collapse: collapse; width: 100%; }
        th, td { padding: 2pt 4pt; font-size: 9pt; font-family: Arial, sans-serif; }
    </style><table>
      <tr><th>SKU</th><th>Description</th><th>On hand</th><th>Backorder</th><th>Unit cost</th><th>Value</th></tr>
      <tr><td>ANV-0001</td><td>Anvil standard 150 lb</td><td>24</td><td>3</td><td>120.00</td><td>2880.00</td></tr>
      <tr><td>CLO-1401</td><td>Disappearing/reappearing clothes</td><td>8</td><td>1</td><td>130.00</td><td>1040.00</td></tr>
    </table>"#;
    let (dom, styles) = parse(html);
    let tid = table_id_of(&dom);
    // The cascade carries `width: 100%` as a 1.0 fraction (CORE-81).
    let pct = styles[tid]
        .width_percent
        .expect("width:100% carries a percentage");
    assert!((pct - 1.0).abs() < 0.001, "width_percent = {pct}");
    // Used widths differ per the declared width at a 540pt content box.
    let avail = Scalar(540.0);
    let sum = |cw: &typeanvil::table::ColumnWidths| {
        cw.widths.iter().map(|w| w.get()).sum::<f64>()
    };
    let auto = measure_columns(&dom, &styles, tid, avail, None);
    let pct100 = measure_columns(&dom, &styles, tid, avail, Some(Scalar(avail.get() * pct)));
    let fixed400 = measure_columns(&dom, &styles, tid, avail, Some(Scalar(400.0)));
    assert!(sum(&auto) < 540.0, "auto sum = {}", sum(&auto));
    assert!(
        (sum(&pct100) - 540.0).abs() < 1.0,
        "100% sum = {}",
        sum(&pct100)
    );
    assert!(
        (sum(&fixed400) - 400.0).abs() < 1.0,
        "400pt sum = {}",
        sum(&fixed400)
    );
}

#[test]
fn test_table_stress_description_column_wraps() {
    // 6-column table shaped like the table-stress corpus fixture at the demo
    // geometry (5in × 3in, 0.5in margins → 288pt content): the overflow
    // branch (sum(min) > used) pins Description at its min-content ≈ 112pt
    // and long descriptions wrap to 2+ lines.
    let geo = geometry(5.0, 3.0, 0.5);
    let html = r#"<!DOCTYPE html><style>
        table { border-collapse: collapse; width: 100%; }
        th, td { padding: 2pt 4pt; font-size: 9pt; font-family: Arial, sans-serif; }
    </style><table>
      <thead><tr><th>SKU</th><th>Description</th><th>On hand</th><th>Backorder</th><th>Unit cost</th><th>Value</th></tr></thead>
      <tbody>
        <tr><td>ANV-0001</td><td>Anvil, standard (150 lb)</td><td>24</td><td>3</td><td>120.00</td><td>2,880.00</td></tr>
        <tr><td>CLO-1401</td><td>Disappearing/reappearing clothes</td><td>8</td><td>1</td><td>130.00</td><td>1,040.00</td></tr>
        <tr><td>TRP-1510</td><td>Earthquake pill (double strength)</td><td>14</td><td>0</td><td>55.00</td><td>770.00</td></tr>
      </tbody>
    </table>"#;
    // Direct check: Description column = min-content ≈ 112pt.
    let (dom, styles) = parse(html);
    let tid = table_id_of(&dom);
    let cols = measure_columns(&dom, &styles, tid, Scalar(288.0), Some(Scalar(288.0)));
    let desc = cols.widths[1];
    assert!(
        desc.get() > 100.0 && desc.get() < 135.0,
        "Description column = {} (expect ≈112pt min-content)",
        desc.get()
    );
    // The longest description wraps to 2+ lines: "Earthquake" and "strength"
    // land on different lines (different y).
    let l = lay(html, geo);
    let frags = page_fragments(&l, 0);
    let y_of = |needle: &str| {
        frags
            .iter()
            .filter(|(f, _, _)| text_of(f).contains(needle))
            .map(|(_, _, y)| *y)
            .next()
    };
    let y_eq = y_of("Earthquake").expect("Earthquake text on page 1");
    let y_st = y_of("strength").expect("strength text on page 1");
    assert!(
        (y_st - y_eq).abs() > 3.0,
        "expected 2-line description: Earthquake y={y_eq}, strength y={y_st}"
    );
}

#[test]
fn test_table_stress_page_count_grows() {
    // The corpus fixture at the demo geometry. The auto-layout distribution
    // (overflow branch → min-content columns) wraps the longest descriptions,
    // moving the page count above the old heuristic's 20. NOTE: measured 21 —
    // the residual gap to Prince's 45 is Prince's FIRST-PAGE column freeze
    // (it measures only the header + first-page rows, giving the Description
    // column ≈57pt = the header width; the standard CSS algorithm measures
    // all rows → 112pt). Tracked as a follow-up (CORE-89); this test guards
    // against regressing back to the content-scaling heuristic.
    let src = std::fs::read_to_string("../demo/corpus/table-stress.html")
        .expect("table-stress corpus fixture");
    let geo = geometry(5.0, 3.0, 0.5);
    let l = lay(&src, geo);
    assert!(
        l.pages.len() > 20,
        "table-stress pages = {} (expect > 20; was 20, Prince 45, standard algorithm 21)",
        l.pages.len()
    );
}

#[test]
fn test_two_column_probe_width() {
    // Prince-verified constant (2026-08-20): the same 320pt-wide two-column
    // table splits col1 ≈ 293pt / col2 ≈ 23pt in Prince, matching the
    // css-tables-3 middle branch within measurement error. The engine must
    // land within ±2% of the verified 295pt.
    let html = r#"<!DOCTYPE html><style>
        table { border-collapse: collapse; width: 320pt; }
        th, td { padding: 2pt 4pt; font-size: 9pt; font-family: Arial, sans-serif; }
    </style><table>
      <thead><tr><th>Description</th><th>Qty</th></tr></thead>
      <tbody>
        <tr><td>Earthquake pill double strength trial pack with extra warranty coverage for industrial use</td><td>24</td></tr>
        <tr><td>Disintegrating shotgun with automatic reloading mechanism and carrying case</td><td>3</td></tr>
        <tr><td>Spring loaded boxing glove certified tournament grade leather construction</td><td>13</td></tr>
      </tbody>
    </table>"#;
    let (dom, styles) = parse(html);
    let tid = table_id_of(&dom);
    let cols = measure_columns(&dom, &styles, tid, Scalar(540.0), Some(Scalar(320.0)));
    let long = cols.widths[0];
    assert!(
        (long.get() - 295.0).abs() / 295.0 <= 0.02,
        "long column = {} (expect 295 ±2%)",
        long.get()
    );
}

// --- CORE-89 first-page column freeze acceptance tests -----------------------
// Each maps to an acceptance criterion in
// `docs/specifications/table-first-page-column-freeze.spec.md`.

/// A table-stress-shaped fixture: 6 columns, header + 30 body rows, with the
/// wide 112pt token "Disappearing/reappearing" in a LATE row (row 25) so it
/// lands on page 2+ and must NOT enter the frozen measure.
fn freeze_fixture() -> String {
    let mut rows = String::new();
    for i in 1..=30 {
        let desc = if i == 25 {
            "Disappearing/reappearing clothes"
        } else {
            "Anvil, standard (150 lb)"
        };
        rows.push_str(&format!(
            "<tr><td>ANV-{i:04}</td><td>{desc}</td><td>24</td><td>3</td><td>120.00</td><td>2,880.00</td></tr>"
        ));
    }
    format!(
        r#"<!DOCTYPE html><style>
        table {{ border-collapse: collapse; width: 100%; }}
        th, td {{ padding: 2pt 4pt; font-size: 9pt; font-family: Arial, sans-serif; }}
    </style><table>
      <thead><tr><th>SKU</th><th>Description</th><th>On hand</th><th>Backorder</th><th>Unit cost</th><th>Value</th></tr></thead>
      <tbody>{rows}</tbody>
    </table>"#
    )
}

#[test]
fn test_freeze_excludes_late_wide_rows() {
    // AC 1: the frozen scope excludes the late 112pt token, so Description
    // min-content ≈ the header's own width (≈57pt), NOT 112pt.
    let html = freeze_fixture();
    let (dom, styles) = parse(&html);
    let tid = table_id_of(&dom);
    // Demo geometry: 5in × 3in page, 0.5in margins → 288pt content width,
    // 144pt content height.
    let scope = resolve_freeze_scope(&dom, &styles, tid, Scalar(288.0), Some(Scalar(288.0)), Scalar(144.0));
    assert!(
        matches!(scope, MeasureScope::FirstPage { .. }),
        "frozen scope = {:?} (expect FirstPage — the table fragments)",
        scope
    );
    let frozen = measure_columns_scoped(&dom, &styles, tid, Scalar(288.0), Some(Scalar(288.0)), scope);
    let all = measure_columns(&dom, &styles, tid, Scalar(288.0), Some(Scalar(288.0)));
    let desc_frozen = frozen.widths[1];
    let desc_all = all.widths[1];
    assert!(
        desc_all.get() > 100.0,
        "All-scope Description = {} (expect ≈112pt — the late token enters)",
        desc_all.get()
    );
    assert!(
        desc_frozen.get() < 100.0,
        "frozen Description = {} (expect < 100pt — the late 112pt token is excluded)",
        desc_frozen.get()
    );
    assert!(
        desc_frozen.get() > 40.0,
        "frozen Description = {} (expect ≈ header width ≈ 57pt)",
        desc_frozen.get()
    );
}

#[test]
fn test_freeze_noop_single_fragmentainer() {
    // AC 2: a table that fits one fragmentainer resolves scope All, so the
    // frozen widths are byte-identical to the CORE-81 All-scope measure.
    let html = r#"<!DOCTYPE html><style>
        table { border-collapse: collapse; width: 100%; }
        th, td { padding: 2pt 4pt; font-size: 9pt; font-family: Arial, sans-serif; }
    </style><table>
      <thead><tr><th>SKU</th><th>Description</th></tr></thead>
      <tbody>
        <tr><td>ANV-0001</td><td>Anvil, standard</td></tr>
        <tr><td>RKT-0101</td><td>Rocket skates</td></tr>
      </tbody>
    </table>"#;
    let (dom, styles) = parse(html);
    let tid = table_id_of(&dom);
    let scope = resolve_freeze_scope(&dom, &styles, tid, Scalar(288.0), Some(Scalar(288.0)), Scalar(144.0));
    assert_eq!(scope, MeasureScope::All, "single-fragmentainer table → All scope");
    let frozen = measure_columns_scoped(&dom, &styles, tid, Scalar(288.0), Some(Scalar(288.0)), scope);
    let all = measure_columns(&dom, &styles, tid, Scalar(288.0), Some(Scalar(288.0)));
    assert_eq!(frozen.widths, all.widths, "frozen widths == All-scope widths");
}

#[test]
fn test_freeze_widths_stable_across_pages() {
    // AC 3: freeze-once — every page's table uses the SAME frozen widths. The
    // repeating header's "SKU" cell x is identical on every page; if widths
    // varied per page, the header x would move.
    let html = freeze_fixture();
    let geo = geometry(5.0, 3.0, 0.5);
    let l = lay(&html, geo);
    assert!(l.pages.len() >= 3, "expected multi-page, got {}", l.pages.len());
    let mut sku_xs: Vec<f32> = Vec::new();
    for i in 0..l.pages.len() {
        let frags = page_fragments(&l, i);
        let x = frags
            .iter()
            .filter(|(f, _, _)| text_of(f) == "SKU")
            .map(|(_, x, _)| *x)
            .next();
        sku_xs.push(x.expect("repeating header SKU on every page"));
    }
    sku_xs.dedup_by(|a, b| (*a - *b).abs() < 0.01);
    assert_eq!(
        sku_xs.len(),
        1,
        "header SKU x identical on every page (freeze-once), got {:?}",
        sku_xs
    );
}

#[test]
fn test_freeze_deterministic() {
    // AC 4: the freeze is pure — two renders of the same input produce
    // identical layouts (page count + full fragment tree Debug form).
    let html = freeze_fixture();
    let geo = geometry(5.0, 3.0, 0.5);
    let a = lay(&html, geo);
    let b = lay(&html, geo);
    assert_eq!(a.pages.len(), b.pages.len(), "same page count");
    assert_eq!(
        format!("{:?}", a.pages),
        format!("{:?}", b.pages),
        "identical fragment trees (deterministic freeze)"
    );
}

#[test]
fn test_freeze_header_only_scope() {
    // AC 5: a first fragmentainer shorter than the header alone degrades to
    // FirstPage { body_rows: 0 } without panic; the table still lays out.
    // (NOTE: use a normal 9pt table — a giant-font table trips a PRE-EXISTING
    // pagination hang on main with rows taller than the page, unrelated to
    // the freeze.)
    let html = r#"<!DOCTYPE html><style>
        table { border-collapse: collapse; width: 100%; }
        th, td { padding: 2pt 4pt; font-size: 9pt; font-family: Arial, sans-serif; }
    </style><table>
      <thead><tr><th>SKU</th><th>Description</th></tr></thead>
      <tbody>
        <tr><td>ANV-0001</td><td>Anvil, standard</td></tr>
        <tr><td>RKT-0101</td><td>Rocket skates</td></tr>
      </tbody>
    </table>"#;
    let (dom, styles) = parse(html);
    let tid = table_id_of(&dom);
    // A 5pt fragmentainer is shorter than the header alone (9pt text + padding
    // ≈ 15pt), so the scope degrades to header-only without panic.
    let scope = resolve_freeze_scope(&dom, &styles, tid, Scalar(288.0), Some(Scalar(288.0)), Scalar(5.0));
    assert_eq!(
        scope,
        MeasureScope::FirstPage { body_rows: 0 },
        "header-taller-than-fragmentainer → FirstPage {{ body_rows: 0 }}"
    );
    // No panic, deterministic.
    let l = lay(&html, geometry(5.0, 3.0, 0.5));
    assert!(l.pages.len() >= 1);
}

#[test]
fn test_table_stress_freeze_pages_ge_40() {
    // AC 6 (the ticket's Done line): the corpus fixture at the demo geometry
    // moves from 21 pages (standard algorithm) to ≥ 40 (Prince: 45) once the
    // first-page freeze narrows the Description column to ≈ the header width.
    let src = std::fs::read_to_string("../demo/corpus/table-stress.html")
        .expect("table-stress corpus fixture");
    let geo = geometry(5.0, 3.0, 0.5);
    let l = lay(&src, geo);
    assert!(
        l.pages.len() >= 40,
        "table-stress pages = {} (expect ≥ 40; was 21, Prince 45)",
        l.pages.len()
    );
}

// --- CORE-96: table column-width residual (footer repeat, bold th, colspan) --

#[test]
fn test_colspan_measure_distributes_spanning_cells() {
    // A tfoot's colspan=4 cell ("Total line items: 150") must contribute an
    // EQUAL SHARE to each spanned column (css-tables-3 §10.4.3), and the
    // footer's "158,900.00" (colspan=1) must land in the LAST column — NOT
    // inflate the On hand column to its whole-text width. Before CORE-96 the
    // colspan-blind measure put 158,900.00 into column 2 (On hand → 53pt) and
    // "Total line items: 150" whole text into column 0 (SKU max → 93pt).
    let html = r#"<!DOCTYPE html><style>
        table { border-collapse: collapse; width: 100%; }
        th, td { padding: 2pt 4pt; font-size: 9pt; font-family: Arial, sans-serif; }
    </style><table>
      <thead><tr><th>SKU</th><th>Description</th><th>On hand</th><th>Backorder</th><th>Unit cost</th><th>Value</th></tr></thead>
      <tbody>
        <tr><td>ANV-0001</td><td>Anvil, standard (150 lb)</td><td>24</td><td>3</td><td>120.00</td><td>2,880.00</td></tr>
        <tr><td>ANV-0002</td><td>Anvil, deluxe (300 lb)</td><td>8</td><td>0</td><td>240.00</td><td>1,920.00</td></tr>
      </tbody>
      <tfoot><tr><td colspan="4">Total line items: 150</td><td>—</td><td>158,900.00</td></tr></tfoot>
    </table>"#;
    let (dom, styles) = parse(html);
    let tid = table_id_of(&dom);
    // The footer cell "158,900.00" is the 6th cell (colspan 1) → column 5.
    let footer_cell = dom
        .nodes
        .iter()
        .enumerate()
        .find(|(_, n)| matches!(&n.kind, NodeKind::Element(el) if el.tag == "tfoot"))
        .map(|(id, _)| {
            dom.nodes[id]
                .children
                .iter()
                .find(|c| {
                    matches!(&dom.nodes[**c].kind, NodeKind::Element(el) if el.tag == "tr")
                })
                .copied()
                .unwrap()
        })
        .and_then(|row| {
            dom.nodes[row]
                .children
                .iter()
                .find(|c| {
                    matches!(&dom.nodes[**c].kind, NodeKind::Element(el) if el.tag == "td")
                })
                .copied()
        })
        .unwrap();
    assert_eq!(
        cell_colspan(&dom, footer_cell),
        4,
        "footer total cell spans 4 columns"
    );
    let cols = measure_columns(&dom, &styles, tid, Scalar(288.0), Some(Scalar(288.0)));
    // On hand (col 2) is header/data driven, NOT the footer total.
    assert!(
        cols.widths[2].get() < 40.0,
        "On hand column = {} (expect < 40 — the colspan-blind measure put \
         158,900.00 here at 53pt)",
        cols.widths[2].get()
    );
    // Value (col 5) picks up the footer total token.
    assert!(
        cols.min_widths[5].get() > 48.0,
        "Value min = {} (expect ≈ 53 — the footer total lands here)",
        cols.min_widths[5].get()
    );
    // SKU (col 0) max is NOT inflated by the spanning cell's whole text.
    assert!(
        cols.max_widths[0].get() < 60.0,
        "SKU max = {} (expect < 60 — the spanning cell contributed a share)",
        cols.max_widths[0].get()
    );
}

#[test]
fn test_th_bold_uas_default() {
    // Prince 16.2 html.css line 482: `th { font-weight: bold; }`. The UA
    // default must cascade to header cells (browsers use `bolder`), so the
    // header column measures ~10% wider than a regular td (CORE-96: without
    // it the frozen Description column is ~0.4pt too narrow and borderline
    // rows wrap to an extra line vs Prince).
    let html = r#"<!DOCTYPE html><style>
        table { border-collapse: collapse; }
        td, th { font-size: 9pt; font-family: Arial, sans-serif; }
    </style><table>
      <tr><th>Header</th><td>Body</td></tr>
    </table>"#;
    let (dom, styles) = parse(html);
    let th = dom
        .nodes
        .iter()
        .enumerate()
        .find(|(_, n)| matches!(&n.kind, NodeKind::Element(el) if el.tag == "th"))
        .map(|(id, _)| id)
        .unwrap();
    let td = dom
        .nodes
        .iter()
        .enumerate()
        .find(|(_, n)| matches!(&n.kind, NodeKind::Element(el) if el.tag == "td"))
        .map(|(id, _)| id)
        .unwrap();
    assert!(
        styles[th].font_weight >= 600.0,
        "th font-weight = {} (expect ≥ 600 / bold)",
        styles[th].font_weight
    );
    assert!(
        styles[td].font_weight < 600.0,
        "td font-weight = {} (expect regular)",
        styles[td].font_weight
    );
}

#[test]
fn test_cell_background_survives_border() {
    // CORE-100: a table cell with BOTH a background-color and a border must
    // render both — the background fill AND the border strokes. The old code
    // overwrote FragmentContent::Background with FragmentContent::Border when
    // any border was present, so every corpus table header/footer rendered
    // WHITE (invoice p1: 0 #a8dadc px vs Prince 4,016).
    let html = r#"<!DOCTYPE html><style>
        table { border-collapse: collapse; }
        th { background-color: #a8dadc; border: 0.5pt solid #aaa; font-size: 9pt; }
        td { border: 0.5pt solid #aaa; font-size: 9pt; }
    </style><table>
      <tr><th>HDR</th></tr>
      <tr><td>row</td></tr>
    </table>"#;
    let l = lay(html, geometry(5.0, 3.0, 0.5));
    let frags = page_fragments(&l, 0);
    // A Background fragment must exist (the header fill)…
    assert!(
        frags.iter().any(|(f, _, _)| matches!(f.content, FragmentContent::Background(_))),
        "expected a Background fragment (header fill)"
    );
    // …and a Border fragment must ALSO exist (the cell borders).
    assert!(
        frags.iter().any(|(f, _, _)| matches!(f.content, FragmentContent::Border(_))),
        "expected a Border fragment (cell borders)"
    );
    // The background must be a direct content of the header cell, not
    // replaced by the border: find a Background fragment whose parent has a
    // Border child (the coexistence shape).
    let has_bg_with_border_child = frags.iter().any(|(f, _, _)| {
        matches!(f.content, FragmentContent::Background(_))
            && f.children
                .iter()
                .any(|c| matches!(c.content, FragmentContent::Border(_)))
    });
    assert!(
        has_bg_with_border_child,
        "expected a Background fragment carrying a Border child (coexistence)"
    );
}

#[test]
fn test_row_background_paints_under_cell_backgrounds() {
    // CORE-100 (row half): a `tr { background-color }` must paint as its own
    // fill UNDER the cell fills. The row fragment carries the Background and
    // the cell fragments (its children) carry theirs, so the pdf emitter's
    // pre-order walk draws the row fill first (css-tables-3 paint order).
    // Pre-fix the row had no Background fragment at all (`#00ff00` absent in
    // the triage repro).
    let html = r#"<!DOCTYPE html><style>
        table { border-collapse: collapse; width: 100%; }
        tr { background-color: #00ff00; }
        th, td { background-color: #a8dadc; border: 0.5pt solid #aaa; font-size: 9pt; }
    </style><table>
      <tr><th>HDR</th><th>Qty</th></tr>
      <tr><td>row</td><td>1</td></tr>
    </table>"#;
    let l = lay(html, geometry(5.0, 3.0, 0.5));
    let frags = page_fragments(&l, 0);
    // The row-level coexistence shape: a full-width Background fragment (the
    // 288pt content box) carrying cell Background children. A lone cell bg is
    // narrower (one column) and has no Background children.
    let is_row_bg = |f: &&Fragment| {
        matches!(f.content, FragmentContent::Background(_))
            && (f.size.0.get() - 288.0).abs() < 0.5
            && f
                .children
                .iter()
                .any(|c| matches!(c.content, FragmentContent::Background(_)))
    };
    let row_bg_count = frags.iter().filter(|(f, _, _)| is_row_bg(f)).count();
    assert_eq!(
        row_bg_count, 2,
        "both rows must carry a full-width Background fill above their cells"
    );
}

#[test]
fn test_group_background_paints_under_rows() {
    // CORE-100 (group half): `thead`/`tbody`/`tfoot` backgrounds must paint
    // as the group fragment's own fill, under the row and cell fills.
    let html = r#"<!DOCTYPE html><style>
        table { border-collapse: collapse; width: 100%; }
        thead { background-color: #ff0000; }
        th, td { background-color: #a8dadc; border: 0.5pt solid #aaa; font-size: 9pt; }
    </style><table>
      <thead><tr><th>HDR</th></tr></thead>
      <tbody><tr><td>row</td></tr></tbody>
    </table>"#;
    let l = lay(html, geometry(5.0, 3.0, 0.5));
    let frags = page_fragments(&l, 0);
    // The group shape: a full-width Background fragment (thead fill) whose
    // child row fragments carry cell Background descendants. Only the thead
    // has a background-color, so exactly one such fragment may exist.
    let is_group_bg = |f: &&Fragment| {
        matches!(f.content, FragmentContent::Background(_))
            && (f.size.0.get() - 288.0).abs() < 0.5
            && f.children.iter().any(|row| {
                row.children
                    .iter()
                    .any(|cell| matches!(cell.content, FragmentContent::Background(_)))
            })
    };
    let group_bg_count = frags.iter().filter(|(f, _, _)| is_group_bg(f)).count();
    assert_eq!(
        group_bg_count, 1,
        "expected exactly one full-width Background fragment (thead fill) above row/cell fills"
    );
}

#[test]
fn test_row_height_includes_collapsed_border() {
    // CORE-96: measure_rows adds the collapsed row-start border to the row
    // height (border-collapse: collapse, spec rule 9). Prince's rows measure
    // 0.5pt taller than TA's pre-fix (26.1 vs 25.6 for a 2-line row); the
    // accumulated difference is the table-stress 43→45 page lever.
    let with_border = r#"<!DOCTYPE html><style>
        table { border-collapse: collapse; }
        th, td { padding: 2pt; font-size: 9pt; font-family: Arial, sans-serif; border: 0.5pt solid #aaa; }
    </style><table><tbody>
      <tr><td>Anvil, standard (150 lb)</td><td>24</td></tr>
    </tbody></table>"#;
    let no_border = with_border.replace("border: 0.5pt solid #aaa;", "");
    let (dom1, styles1) = parse(with_border);
    let (dom2, styles2) = parse(&no_border);
    let tid1 = table_id_of(&dom1);
    let tid2 = table_id_of(&dom2);
    let cols1 = measure_columns(&dom1, &styles1, tid1, Scalar(288.0), Some(Scalar(288.0)));
    let cols2 = measure_columns(&dom2, &styles2, tid2, Scalar(288.0), Some(Scalar(288.0)));
    let row1 = dom1
        .nodes
        .iter()
        .enumerate()
        .find(|(_, n)| matches!(&n.kind, NodeKind::Element(el) if el.tag == "tr"))
        .map(|(id, _)| id)
        .unwrap();
    let row2 = dom2
        .nodes
        .iter()
        .enumerate()
        .find(|(_, n)| matches!(&n.kind, NodeKind::Element(el) if el.tag == "tr"))
        .map(|(id, _)| id)
        .unwrap();
    let (h1, _) = measure_rows(&dom1, &styles1, &[row1], &cols1, Scalar(288.0));
    let (h2, _) = measure_rows(&dom2, &styles2, &[row2], &cols2, Scalar(288.0));
    let diff = h1.first().unwrap().get() - h2.first().unwrap().get();
    assert!(
        (diff - 0.5).abs() < 0.01,
        "row height with 0.5pt border = {} vs without = {} (expect +0.5)",
        h1.first().unwrap().get(),
        h2.first().unwrap().get()
    );
}
