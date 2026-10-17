//! CORE-368: tables inside fragmentainers they did not fit, especially
//! multicol columns.
//!
//! The table path was reached only by real `<table>` markup until CORE-368
//! stopped the paged pass rewriting div tables to `display: block`. The div
//! tables in WPT `css-break/table/repeated-section/*` then exposed four
//! defects, each pinned below:
//!
//! 1. A body row's last-resort placement counted the header as progress, so
//!    a row deferred past the header met the same repeated header on the
//!    next fragment, forever.
//! 2. A repeated header that did not fit was SPLIT, and its resume token
//!    replaced the body row's, so the table restarted from row 1 on every
//!    fragment (the 100,000-page runaway).
//! 3. Auto table layout measured only text, so a cell holding a sized
//!    `<div>` measured 0 wide and nothing in it painted.
//! 4. Multicol ignored the container's definite height and `column-fill`,
//!    so every set was balanced from a content estimate that cannot see the
//!    table's repeated chrome, and the set's tail spilled onto extra pages.
//! 5. Repeated chrome followed no rule from css-tables-3 §8 / Chromium:
//!    every header/footer group repeated, in source order, sized against the
//!    page even inside a column. Now only the FIRST header (laid first) and
//!    FIRST footer (laid last) are chrome, and they repeat only when
//!    `break-inside: avoid` (UA default for `thead`/`tfoot`) and at most a
//!    quarter of the fragmentainer.

use typeanvil::css::{cascade, Color, Stylesheet};
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::frag::{Fragment, FragmentContent};
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

fn page_text(l: &Layout, page: usize) -> String {
    fn rec(f: &Fragment, out: &mut String) {
        if let FragmentContent::Text(run) = &f.content {
            out.push_str(&run.text);
            out.push(' ');
        }
        for c in &f.children {
            rec(c, out);
        }
    }
    let mut s = String::new();
    rec(&l.pages[page].root, &mut s);
    s
}

/// Page-absolute `(x, y, w, h)` of every fill of `color` on `page`.
fn fills(l: &Layout, page: usize, color: (u8, u8, u8)) -> Vec<(f64, f64, f64, f64)> {
    fn rec(f: &Fragment, px: f64, py: f64, c: (u8, u8, u8), out: &mut Vec<(f64, f64, f64, f64)>) {
        let ax = px + f.offset.x.get();
        let ay = py + f.offset.y.get();
        if let FragmentContent::Background(Color { r, g, b, .. }) = &f.content {
            if (*r, *g, *b) == c {
                out.push((ax, ay, f.size.0.get(), f.size.1.get()));
            }
        }
        for ch in &f.children {
            rec(ch, ax, ay, c, out);
        }
    }
    let mut out = Vec::new();
    rec(&l.pages[page].root, 0.0, 0.0, color, &mut out);
    out
}

const GREEN: (u8, u8, u8) = (0, 128, 0);

/// Defect 1: with nothing above the table, a monolithic first body row that
/// does not fit under the header is placed on the first page anyway (it
/// overflows), as Chromium does (WPT monolithic-overflow-023). Deferring it
/// would only meet the same header on the next page.
#[test]
fn header_is_not_progress_for_the_first_body_row() {
    let l = lay(
        "<style>body { margin: 0 } td { padding: 0 }</style>\
         <table style=\"border-spacing:0\">\
         <thead><tr><td>HDR</td></tr></thead>\
         <tbody><tr><td>BIGROW<div style=\"height:140pt; contain:size\"></div></td></tr>\
         <tr><td>AFTER</td></tr></tbody></table>",
    );
    assert!(page_text(&l, 0).contains("BIGROW"), "pages: {:?}", (0..l.pages.len()).map(|p| page_text(&l, p)).collect::<Vec<_>>());
    // The overflow occupies the following page before AFTER resumes, as in
    // Chromium's `[HDR row] [HDR] [HDR after]` for the same shape.
    let after: Vec<usize> = (0..l.pages.len()).filter(|p| page_text(&l, *p).contains("AFTER")).collect();
    assert_eq!(after.len(), 1, "AFTER lands once: {after:?}");
    assert!(after[0] >= 1, "AFTER follows the overflowing row");
}

/// Defect 2: a repeated header taller than what is left of a column is
/// never split, so the body row keeps its progress and the table ends
/// (WPT repeated-section/background-001 looped to the page cap).
#[test]
fn repeated_header_that_does_not_fit_never_restarts_the_table() {
    let l = lay(
        "<style>body { margin: 0 }</style>\
         <p>Test passes if there is a filled green square and <strong>no red</strong>.</p>\
         <div style=\"columns:2; column-fill:auto; gap:0; width:100px; height:400px\">\
         <div style=\"display:table; width:100%\">\
         <div style=\"display:table-header-group; break-inside:avoid\">\
         <div style=\"height:100px; background:green\"></div></div>\
         <div style=\"height:400px\"></div></div></div><p>END</p>",
    );
    assert!(l.pages.len() <= 6, "table must terminate, got {} pages", l.pages.len());
    let ends = (0..l.pages.len()).filter(|p| page_text(&l, *p).contains("END")).count();
    assert_eq!(ends, 1, "the trailing content lands exactly once");
}

/// Defect 3: a cell whose only content is a `width:25px` block measures
/// 25px wide, so the block paints at its declared size.
#[test]
fn declared_width_block_sizes_its_auto_table_column() {
    let l = lay(
        "<style>body { margin: 0 } td { padding: 0 }</style>\
         <table style=\"border-spacing:0\"><tr><td>\
         <div style=\"width:25px; height:20px; background:green\"></div></td></tr></table>",
    );
    let f = fills(&l, 0, GREEN);
    assert_eq!(f.len(), 1, "{f:?}");
    assert!((f[0].2 - 18.75).abs() < 0.01, "25px = 18.75pt wide, got {}", f[0].2);
}

/// Defect 4: `column-fill: auto` with a definite height fills 100px-tall
/// columns in turn. The table (20px repeated header + 80px of body per
/// column, 320px of body) fits exactly in the four columns on ONE page,
/// matching Chromium (WPT repeated-section/header).
#[test]
fn column_fill_auto_with_definite_height_fills_fixed_columns() {
    let l = lay(
        "<style>body { margin: 0 }</style>\
         <div style=\"columns:4; column-fill:auto; gap:0; width:100px; height:100px\">\
         <div style=\"display:table\">\
         <div style=\"display:table-header-group; break-inside:avoid\">\
         <div style=\"height:20px; background:green\"></div></div>\
         <div style=\"width:25px; height:320px; background:green\"></div></div></div>",
    );
    assert_eq!(l.pages.len(), 1, "four 100px columns on one page");
    let f = fills(&l, 0, GREEN);
    let xs: std::collections::BTreeSet<i64> = f.iter().map(|r| (r.0 * 10.0).round() as i64).collect();
    assert_eq!(xs.len(), 4, "green in all four columns: {f:?}");
    let bottom = f.iter().map(|r| r.1 + r.3).fold(0.0, f64::max);
    assert!(bottom <= 36.0 + 75.0 + 0.01, "nothing below the 100px column box: {bottom}");
}

/// Guard: `column-fill` reaches the computed style through the paged pass,
/// combinator selector included; `balance` is the initial value.
#[test]
fn column_fill_is_computed() {
    let html = "<style>#w div { column-fill: auto }</style>\
                <section id=\"w\"><div id=\"a\">x</div></section><div id=\"b\">y</div>";
    let (dom, sheet) = parse(html);
    let styles = cascade(&dom, &sheet, &geometry());
    let by_id = |want: &str| {
        dom.nodes
            .iter()
            .position(|n| matches!(&n.kind, NodeKind::Element(el) if el.id.as_deref() == Some(want)))
            .unwrap()
    };
    assert!(styles[by_id("a")].column_fill_auto);
    assert!(!styles[by_id("b")].column_fill_auto);
}

fn rows(n: usize) -> String {
    (0..n).map(|i| format!("<tr><td>r{i}</td></tr>")).collect()
}

fn header_pages(l: &Layout) -> Vec<bool> {
    (0..l.pages.len()).map(|p| page_text(l, p).contains("HD")).collect()
}

/// Defect 5a: `<thead>` repeats by default (UA `break-inside: avoid`); an
/// author `break-inside: auto` stops it. Chromium does both (probe q2).
#[test]
fn thead_repeats_only_while_break_inside_avoid() {
    let t = |extra: &str| {
        lay(&format!(
            "<style>body {{ margin: 0 }} td {{ padding: 0 }} {extra}</style>\
             <table><thead><tr><td>HD</td></tr></thead><tbody>{}</tbody></table>",
            rows(40)
        ))
    };
    let default = header_pages(&t(""));
    assert!(default.len() > 1 && default.iter().all(|h| *h), "{default:?}");
    let auto = header_pages(&t("thead { break-inside: auto }"));
    assert!(auto.len() > 1, "{auto:?}");
    assert!(auto[0] && auto[1..].iter().all(|h| !*h), "header only on the first page: {auto:?}");
}

/// Defect 5b: a header over a quarter of the page does not repeat.
/// 144pt of page content: 36pt repeats, 40pt does not (Chromium: 47px
/// repeats, 49px does not, on a 192px page).
#[test]
fn header_taller_than_a_quarter_of_the_page_does_not_repeat() {
    let t = |h: f64| {
        header_pages(&lay(&format!(
            "<style>body {{ margin: 0 }} td {{ padding: 0 }}</style>\
             <table style=\"border-spacing:0\"><thead><tr><td>HD<div style=\"height:{h}pt\"></div></td></tr></thead>\
             <tbody>{}</tbody></table>",
            rows(40)
        )))
    };
    let small = t(15.0);
    assert!(small.len() > 1 && small.iter().all(|h| *h), "{small:?}");
    let big = t(30.0);
    assert!(big.len() > 1 && big[0] && big[1..].iter().all(|h| !*h), "{big:?}");
}

/// Defect 5c: inside a multicol column the quarter is of the COLUMN.
/// 100px columns: a ~32px header (20px line + 12px block, > 25px) appears
/// once, though it is under a quarter of the 192px page (Chromium probe
/// mc/mh30).
#[test]
fn quarter_rule_uses_the_column_as_fragmentainer() {
    let l = lay(&format!(
        "<style>body {{ margin: 0 }} td {{ padding: 0; line-height: 20px }}</style>\
         <div style=\"columns:4; column-fill:auto; gap:0; width:400px; height:100px\">\
         <table style=\"border-spacing:0\"><thead><tr><td>HD<div style=\"height:12px\"></div></td></tr></thead>\
         <tbody>{}</tbody></table></div>",
        rows(12)
    ));
    let text: String = (0..l.pages.len()).map(|p| page_text(&l, p)).collect();
    assert_eq!(text.matches("HD").count(), 1, "{text}");
}

/// Defect 5d: only the first header/footer group is chrome, laid first and
/// last; later ones are body groups in source order (WPT
/// sections-and-captions-mixed-order, which Chromium renders 1..8 in order).
#[test]
fn only_the_first_header_and_footer_groups_are_chrome() {
    let l = lay(
        "<style>body { margin: 0 }</style>\
         <div style=\"display:table\">\
         <div style=\"display:table-footer-group\">F6</div>\
         <div style=\"display:table-row-group\">B2</div>\
         <div style=\"display:table-footer-group\">B3</div>\
         <div style=\"display:table-header-group\">H1</div>\
         <div style=\"display:table-header-group\">B4</div>\
         <div style=\"display:table-row-group\">B5</div></div>",
    );
    assert_eq!(l.pages.len(), 1);
    let words: Vec<String> = page_text(&l, 0).split_whitespace().map(str::to_string).collect();
    assert_eq!(words, ["H1", "B2", "B3", "B4", "B5", "F6"]);
}

const PURPLE: (u8, u8, u8) = (128, 0, 128);

/// Defect 6: the footer reservation counts the footer cell's declared
/// `height`. `measure_rows` ignored it, so the reservation was 18pt short;
/// once the body row stopped deferring past the header (defect 1) the
/// repeated footer no longer fit and vanished from every page but the last
/// (WPT table-fragmentation-003a; Chromium repeats it on all three).
#[test]
fn declared_cell_height_counts_in_footer_reservation() {
    let l = lay(
        "<style>body { margin: 0 } td { padding: 0 }\
         table { border-spacing: 9pt } tfoot td { height: 18pt; background: purple }\
         thead td { height: 18pt }</style>\
         <table><thead><tr><td></td></tr></thead>\
         <tbody><tr><td><div style=\"height:144pt\"></div></td></tr></tbody>\
         <tfoot><tr><td></td></tr></tfoot></table>",
    );
    assert!(l.pages.len() >= 2, "{} pages", l.pages.len());
    for p in 0..l.pages.len() {
        let f = fills(&l, p, PURPLE);
        assert_eq!(f.len(), 1, "footer once on page {p}: {f:?}");
        assert!(f[0].1 + f[0].3 <= 36.0 + 144.0 + 0.01, "footer inside the page on page {p}: {f:?}");
    }
}

const BLUE: (u8, u8, u8) = (0, 0, 255);

/// Defect 7: with content above it, a fixed-height multicol never sends an
/// item too tall for its column to the next page; the item overflows its
/// column in place (WPT tall-line-in-short-fragmentainer-000,
/// block-in-inline-000; Chromium: one page).
#[test]
fn fixed_height_multicol_keeps_a_too_tall_item_on_its_page() {
    let l = lay(
        "<style>body { margin: 0 }</style><p>Intro text above the columns.</p>\
         <div style=\"columns:3; column-fill:auto; column-gap:0; width:300px; height:50px\">\
         <div style=\"display:inline-block; width:100px; height:100px; background:blue\"></div></div>",
    );
    assert_eq!(l.pages.len(), 1, "one page");
    // Only "stays on this page" is pinned: the engine still slices a line
    // taller than its fragmentainer (pre-existing, pages too) instead of
    // letting the monolithic box overflow whole.
    let f = fills(&l, 0, BLUE);
    assert!(!f.is_empty(), "the box paints on the first page: {f:?}");
}
