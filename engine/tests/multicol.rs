//! Multi-column acceptance tests — one per acceptance criterion in
//! `docs/specifications/multicol.spec.md` (CORE-63).

use std::path::Path;
use std::process::Command;

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeId, NodeKind};
use typeanvil::frag::{Fragment, FragmentContent, FragmentKind};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::{layout, Layout};

const EPS: f64 = 1e-6;

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

fn style_text(html: &str) -> &str {
    let s = html.find("<style>").expect("style element");
    let e = html.find("</style>").expect("style end");
    &html[s + "<style>".len()..e]
}

fn dom_of(html: &str) -> Dom {
    Dom::parse(html).expect("dom parses")
}

fn lay(html: &str) -> Layout {
    let dom = dom_of(html);
    let ss = Stylesheet::parse(&style_text(html));
    layout(&dom, &ss, geometry())
}

fn node_id_by_class(dom: &Dom, class: &str) -> NodeId {
    dom.nodes
        .iter()
        .enumerate()
        .find_map(|(i, n)| match &n.kind {
            NodeKind::Element(el) if el.classes.iter().any(|c| c == class) => Some(i as NodeId),
            _ => None,
        })
        .expect("element with class must exist") as NodeId
}

/// Fragments whose `source` is the given node.
fn find_source<'a>(frag: &'a Fragment, id: NodeId, out: &mut Vec<&'a Fragment>) {
    if frag.source == Some(id) {
        out.push(frag);
    }
    for c in &frag.children {
        find_source(c, id, out);
    }
}

/// The index of the container's column child whose subtree contains a
/// fragment with `source == id`, plus a reference to that fragment.
fn column_containing<'a>(
    container: &'a Fragment,
    id: NodeId,
) -> Option<(usize, &'a Fragment)> {
    for (k, col) in container.children.iter().enumerate() {
        let mut found = Vec::new();
        find_source(col, id, &mut found);
        if !found.is_empty() {
            return Some((k, col));
        }
    }
    None
}

/// The multicol container fragment on a page: a block fragment whose children
/// include at least two column fragments (source-less blocks with content).
fn page_mc<'a>(layout: &'a Layout, page: usize) -> &'a Fragment {
    let root = &layout.pages[page].root;
    fn is_column(f: &Fragment) -> bool {
        f.source.is_none() && f.kind == FragmentKind::Block && !f.children.is_empty()
    }
    fn walk<'a>(f: &'a Fragment) -> Option<&'a Fragment> {
        if f.children.iter().filter(|c| is_column(c)).count() >= 2 {
            return Some(f);
        }
        for c in &f.children {
            if let Some(r) = walk(c) {
                return Some(r);
            }
        }
        None
    }
    walk(root).expect("multicol container fragment exists")
}

// --- 1. Geometry ---------------------------------------------------------

#[test]
fn three_equal_columns_with_gap() {
    let html = r#"<html><head><style>
        body { margin: 0; font-size: 12pt; line-height: 1.2; }
        p, div { margin: 0; padding: 0; }
        .mc { column-count: 3; }
    </style></head>
    <body><div class="mc">
        <p>Alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon phi chi psi omega.</p>
    </div></body></html>"#;
    let layout = lay(html);
    let container = page_mc(&layout, 0);
    // 3 columns of equal width separated by the default 1em (12pt) gap.
    // col_w = (288 - 2*12) / 3 = 88pt.
    assert_eq!(container.children.len(), 3, "three column children");
    let xs: Vec<f64> = container.children.iter().map(|c| c.offset.x.get()).collect();
    assert_close(xs[0], 0.0, "column 0 at the content edge");
    assert_close(xs[1], 100.0, "column 1 after gap");
    assert_close(xs[2], 200.0, "column 2 after two gaps");
    for c in &container.children {
        assert_close(c.size.0.get(), 88.0, "column width");
    }
}

// --- 2. Balance ----------------------------------------------------------

#[test]
fn balanced_columns_differ_by_at_most_one_line() {
    let html = r#"<html><head><style>
        body { margin: 0; font-size: 12pt; line-height: 1.2; }
        p, div { margin: 0; padding: 0; }
        .mc { column-count: 2; }
    </style></head>
    <body><div class="mc">
        <p>First short paragraph.</p>
        <p>Second paragraph with a few more words to keep things interesting here.</p>
    </div></body></html>"#;
    let layout = lay(html);
    let container = page_mc(&layout, 0);
    let lh = 14.4;
    let heights: Vec<f64> = container
        .children
        .iter()
        .map(|c| c.size.1.get())
        .collect();
    assert_eq!(heights.len(), 2, "two columns");
    let max = heights.iter().cloned().fold(0.0f64, f64::max);
    let min = heights.iter().cloned().fold(f64::MAX, f64::min);
    assert!(
        (max - min) <= lh + EPS,
        "column heights differ by at most one line box: {heights:?}"
    );
    assert!(max > 0.0, "columns have content");
}

// --- 3. Spanner ----------------------------------------------------------

#[test]
fn spanner_spans_full_width_and_resumes_columns() {
    let html = r#"<html><head><style>
        body { margin: 0; font-size: 12pt; line-height: 1.2; }
        p, div, h2 { margin: 0; padding: 0; }
        .mc { column-count: 2; }
        h2.s { column-span: all; }
    </style></head>
    <body><div class="mc">
        <p>Alpha beta gamma delta epsilon zeta eta theta iota kappa.</p>
        <h2 class="s">Spanner heading</h2>
        <p>Mu nu xi omicron pi rho sigma tau upsilon phi chi psi omega.</p>
    </div></body></html>"#;
    let layout = lay(html);
    let container = page_mc(&layout, 0);
    // Two balanced sets (2 columns each) plus the spanner = 5 children.
    assert_eq!(container.children.len(), 5, "set1(2) + spanner + set2(2)");
    // The spanner (middle child) spans the full content width (288pt).
    let spanner = &container.children[2];
    assert_close(spanner.size.0.get(), 288.0, "spanner spans full content width");
    // Column children are narrow (col_w = (288 - 12)/2 = 138pt).
    for (k, c) in container.children.iter().enumerate() {
        if k != 2 {
            assert_close(c.size.0.get(), 138.0, "column width");
        }
    }
    // The spanner sits below the first set and above the second set.
    let set1_top = container.children[0].offset.y.get();
    let spanner_top = spanner.offset.y.get();
    let set2_top = container.children[3].offset.y.get();
    assert!(
        spanner_top >= set1_top - EPS && set2_top >= spanner_top + spanner.size.1.get() - EPS,
        "spanner between the two sets"
    );
}

// --- 4. Nested multicol ---------------------------------------------------

#[test]
fn nested_multicol_constrained_by_outer_column() {
    let html = r#"<html><head><style>
        body { margin: 0; font-size: 12pt; line-height: 1.2; }
        p, div { margin: 0; padding: 0; }
        .outer { column-count: 2; }
        .inner { column-count: 2; }
    </style></head>
    <body><div class="outer">
        <div class="inner">
            <p>Alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu.</p>
        </div>
    </div></body></html>"#;
    let layout = lay(html);
    let dom = dom_of(html);
    let inner_id = node_id_by_class(&dom, "inner");
    // The inner container is inside an outer column: find it in the tree.
    let outer = page_mc(&layout, 0);
    // The inner div is a child of the first column (source None, cols >= 2).
    let mut inner: Option<&Fragment> = None;
    for col in &outer.children {
        fn find_mc<'a>(f: &'a Fragment) -> Option<&'a Fragment> {
            if f.children.len() >= 2
                && f.children
                    .iter()
                    .filter(|c| c.source.is_none() && !c.children.is_empty())
                    .count()
                    >= 2
            {
                return Some(f);
            }
            for c in &f.children {
                if let Some(r) = find_mc(c) {
                    return Some(r);
                }
            }
            None
        }
        if let Some(m) = find_mc(col) {
            inner = Some(m);
        }
    }
    let inner = inner.expect("inner multicol container exists");
    // The inner container's width equals the outer column's content width
    // (138pt) — constrained by every enclosing context.
    assert_close(inner.size.0.get(), 138.0, "inner container = outer column width");
    // And its own columns are narrower still: (138 - 12) / 2 = 63pt.
    for c in &inner.children {
        assert_close(c.size.0.get(), 63.0, "inner column width");
    }
    let _ = inner_id;
}

// --- 5. Fragment across pages --------------------------------------------

#[test]
fn multicol_fragments_across_pages() {
    let long = "filler filler filler filler filler filler filler filler filler filler ";
    let html = format!(
        r#"<html><head><style>
        body {{ margin: 0; font-size: 12pt; line-height: 1.2; }}
        p, div {{ margin: 0; padding: 0; }}
        .mc {{ column-count: 2; }}
    </style></head>
    <body><div class="mc">
        <p>{long}{long}{long}{long}{long}{long}{long}{long}{long}{long}{long}{long}</p>
    </div></body></html>"#
    );
    let layout = lay(&html);
    assert!(
        layout.pages.len() >= 2,
        "container taller than a page fragments: {} pages",
        layout.pages.len()
    );
    // Page 1 has a multicol container, page 2 has a fresh one (the
    // continuation re-balances the remaining content).
    let _p1 = page_mc(&layout, 0);
    let p2 = page_mc(&layout, 1);
    assert!(!p2.children.is_empty(), "page 2 has a fresh set of columns");
}

// --- 6. Break-inside: avoid inside a column --------------------------------

#[test]
fn avoid_box_moves_to_next_column() {
    let html = r#"<html><head><style>
        body { margin: 0; font-size: 12pt; line-height: 1.2; }
        p, div { margin: 0; padding: 0; }
        .mc { column-count: 2; }
        .keep { break-inside: avoid; }
    </style></head>
    <body><div class="mc">
        <p>Alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma.</p>
        <div class="keep"><p>Keep this together.</p></div>
    </div></body></html>"#;
    let layout = lay(html);
    let dom = dom_of(html);
    let keep_id = node_id_by_class(&dom, "keep");
    let container = page_mc(&layout, 0);
    // The keep box must be a child of the SECOND column (column index 1).
    let (col_idx, keep_frag) = column_containing(container, keep_id)
        .expect("keep box inside a column");
    assert_eq!(col_idx, 1, "box moved to the next column");
    // And it starts at the top of that column.
    assert!(
        keep_frag.offset.y.get() < EPS * 10.0,
        "keep box at the column top"
    );
}

// --- 7. (css-multicol WPT subset: harness follow-up — see close-out) ------

// --- 8. Determinism -------------------------------------------------------

#[test]
fn output_is_deterministic_with_multicol() {
    let html = r#"<html><head><style>
        body { margin: 0; font-size: 12pt; line-height: 1.2; }
        p, div { margin: 0; padding: 0; }
        .mc { column-count: 3; }
    </style></head>
    <body><div class="mc">
        <p>Alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon phi chi psi omega.</p>
    </div></body></html>"#;
    let a = render_cli(html);
    let b = render_cli(html);
    assert_eq!(a, b, "byte-identical renders");
}

fn render_cli(html: &str) -> Vec<u8> {
    let dir = std::env::temp_dir().join(format!("typeanvil_multicol_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("in.html");
    let out = dir.join("out.pdf");
    std::fs::write(&src, html).unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_typeanvil"))
        .args([
            "render",
            src.to_str().unwrap(),
            "--page-width",
            "5in",
            "--page-height",
            "3in",
            "--margin-top",
            "0.5in",
            "--margin-right",
            "0.5in",
            "--margin-bottom",
            "0.5in",
            "--margin-left",
            "0.5in",
            "-o",
            out.to_str().unwrap(),
        ])
        .status()
        .expect("CLI runs");
    assert!(status.success(), "CLI exit ok");
    std::fs::read(&out).unwrap()
}

fn assert_close(got: f64, want: f64, msg: &str) {
    assert!(
        (got - want).abs() < EPS,
        "{msg}: got {got}, want {want}"
    );
}

// --- CORE-78 regression: table × multicol ---------------------------------

fn row_id_by_text(dom: &Dom, needle: &str) -> NodeId {
    dom.nodes
        .iter()
        .enumerate()
        .find_map(|(i, n)| match &n.kind {
            NodeKind::Element(el)
                if el.tag == "tr" && dom.text_content(i as NodeId).contains(needle) =>
            {
                Some(i as NodeId)
            }
            _ => None,
        })
        .expect("row element exists")
}

/// A table tall enough to fragment inside columns paginates to a small
/// number of pages instead of looping to MAX_PAGES (CORE-78).
///
/// Regression: `fill_columns_partial` anchored its columns at the page
/// bottom (`set_top = bottom_limit`), so the first table row could never
/// fit (`top == bottom_limit` → always deferred with a break-before token)
/// and the resume token re-created the same state every page. The rows are
/// NOT lost in the token chain — the geometry just never advances. Columns
/// must start at the container's real content cursor.
#[test]
fn table_fragments_inside_multicol() {
    let mut html = String::from(
        r#"<html><head><style>
        body { font-size: 10pt; line-height: 1.2; }
        .two-col { column-count: 2; }
        table { border-collapse: collapse; width: 100%; }
        th, td { border: 1px solid #999; padding: 3pt 5pt; }
        thead { display: table-header-group; }
        tr { break-inside: avoid; }
    </style></head>
    <body><div class="two-col">
      <p>Intro before the table.</p>
      <table><thead><tr><th>A</th><th>B</th></tr></thead><tbody>"#,
    );
    for i in 1..=12 {
        html.push_str(&format!("<tr><td>r{i}</td><td>x{i}</td></tr>"));
    }
    html.push_str("</tbody></table></div></body></html>");

    // Pre-fix this call ran to MAX_PAGES (100k); the assert below is the
    // guard. 5x3in page, 0.5in margins, two ~144pt columns: the 12 rows
    // span 1-2 pages.
    let layout = lay(&html);
    assert!(
        (1..=3).contains(&layout.pages.len()),
        "expected 1-3 pages, got {}",
        layout.pages.len()
    );

    // The table actually progressed: row 1 is laid on page 1 and the LAST
    // row (r12) is laid somewhere. Pre-fix, only row 0 of the tbody was
    // deferred and re-created forever — r12 never appeared.
    let dom = dom_of(&html);
    let first_row = row_id_by_text(&dom, "r1");
    let last_row = row_id_by_text(&dom, "r12");
    assert_ne!(first_row, last_row, "row ids distinct");

    let mut first_frags = Vec::new();
    find_source(&layout.pages[0].root, first_row, &mut first_frags);
    assert!(!first_frags.is_empty(), "first row laid on page 1");

    let mut last_frags = Vec::new();
    for page in &layout.pages {
        find_source(&page.root, last_row, &mut last_frags);
    }
    assert!(!last_frags.is_empty(), "last row laid (progress, no row-0 loop)");
}

/// Bare-text line fragments inside a column carry PARENT-RELATIVE baselines.
///
/// Regression (CORE-102): `fill_one_column` rebased each child fragment's
/// `offset` into the column's coordinate space but left the child LINE
/// fragment's `TextRun.baseline` at its ABSOLUTE page coordinates. The PDF
/// emitter re-adds the accumulated parent origin, so every glyph in a bare
/// text item landed a second time offset right/down by the column origin
/// (e.g. words at x=72/144/216 instead of 36/72/108 — the moz-multicol3
/// page-2 geometry divergence). Block children were unaffected because the
/// block path rebases nested text baselines itself.
#[test]
fn bare_text_column_baselines_are_parent_relative() {
    let html = r#"<html><head><style>
        body { margin: 0; font-size: 12pt; line-height: 1.2; }
        .mc { column-count: 3; }
    </style></head>
    <body><div class="mc">alpha beta gamma delta</div></body></html>"#;
    let layout = lay(html);
    let container = page_mc(&layout, 0);

    // Every bare-text line inside every column must sit within its own
    // column's box: baseline.x <= column width. Pre-fix, column 1's line
    // carried an absolute baseline (~container width + column offset), far
    // outside [0, col_w].
    let col_w = container.children[0].size.0.get();
    for (ci, col) in container.children.iter().enumerate() {
        for child in &col.children {
            if let FragmentContent::Text(run) = &child.content {
                let bx = run.baseline.x.get();
                assert!(
                    bx >= -EPS && bx <= col_w + EPS,
                    "column {ci}: text baseline x={bx} outside parent-relative \
                     [0, {col_w}] — absolute baseline leaked through"
                );
            }
        }
    }
}
