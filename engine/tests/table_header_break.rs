//! CORE-354: a table header stays with its first body row and is never
//! painted twice in one fragment. Spec: `tables-fragmentation.spec.md`
//! rule 7 (repeating header, CORE-354 sub-rules) and acceptance 14.
//!
//! Geometry is the harness page (5in x 3in, 0.5in margins = 144pt content).
//! A filler block of known height leaves a known gap above the table, so
//! each fixture picks which part of the table fits on page 1.

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::frag::{Fragment, FragmentContent};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::{layout, Layout};

fn geometry() -> PageGeometry {
    let i = |v: f64| Scalar(v * 72.0);
    PageGeometry {
        width: i(5.0),
        height: i(3.0),
        margin_top: i(0.5),
        margin_right: i(0.5),
        margin_bottom: i(0.5),
        margin_left: i(0.5),
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
    layout(&dom, &Stylesheet::parse(&css), geometry())
}

fn texts(f: &Fragment, out: &mut Vec<String>) {
    if let FragmentContent::Text(run) = &f.content {
        out.push(run.text.trim().to_string());
    }
    for c in &f.children {
        texts(c, out);
    }
}

/// Text runs of every page, in fragment-tree order.
fn page_texts(l: &Layout) -> Vec<Vec<String>> {
    l.pages
        .iter()
        .map(|p| {
            let mut v = Vec::new();
            texts(&p.root, &mut v);
            v
        })
        .collect()
}

fn count(page: &[String], needle: &str) -> usize {
    page.iter().filter(|t| t.as_str() == needle).count()
}

/// The ticket's fixture: `filler_pt` of block content, then a table.
fn fixture(filler_pt: f64, thead: &str, tfoot: &str) -> String {
    format!(
        r#"<!DOCTYPE html><html><head><style>
body {{ font-family: Arial, sans-serif; font-size: 9pt; margin: 0; }}
.filler {{ height: {filler_pt}pt; }}
table {{ border-collapse: collapse; width: 100%; }}
th, td {{ border: 0.5pt solid #aaa; padding: 2pt 4pt; }}
</style></head><body>
<div class="filler">filler</div>
<table>
  <thead>{thead}</thead>{tfoot}
  <tbody>
    <tr><td>row1</td><td>1</td></tr>
    <tr><td>row2</td><td>2</td></tr>
    <tr><td>row3</td><td>3</td></tr>
  </tbody>
</table>
</body></html>"#
    )
}

const ONE_ROW_HEAD: &str = "<tr><th>HeadA</th><th>HeadB</th></tr>";
const TWO_ROW_HEAD: &str =
    "<tr><th>HeadA</th><th>HeadB</th></tr><tr><th>SubA</th><th>SubB</th></tr>";
const FOOT: &str = "<tfoot><tr><td>FootA</td><td>FootB</td></tr></tfoot>";

/// Invariants every fixture must hold: each header/footer label at most
/// once per page, never on a page without a body row, and every body row
/// exactly once in the document.
fn assert_header_invariants(l: &Layout, labels: &[&str]) {
    let pages = page_texts(l);
    for (pi, page) in pages.iter().enumerate() {
        let has_body = ["row1", "row2", "row3"].iter().any(|r| count(page, r) > 0);
        for label in labels {
            let n = count(page, label);
            assert!(n <= 1, "page {pi}: `{label}` painted {n} times: {page:?}");
            assert!(
                n == 0 || has_body,
                "page {pi}: `{label}` with no body row (orphaned chrome): {page:?}"
            );
        }
    }
    for r in ["row1", "row2", "row3"] {
        let total: usize = pages.iter().map(|p| count(p, r)).sum();
        assert_eq!(total, 1, "`{r}` must appear exactly once: {pages:?}");
    }
}

#[test]
fn header_moves_with_first_row_when_row_does_not_fit() {
    // 125pt filler leaves 19pt: the header row fits, header + row1 does not.
    let l = lay(&fixture(125.0, ONE_ROW_HEAD, ""));
    let pages = page_texts(&l);
    assert_eq!(count(&pages[0], "HeadA"), 0, "header orphaned on page 1: {pages:?}");
    assert_header_invariants(&l, &["HeadA", "HeadB"]);
}

#[test]
fn footer_not_orphaned_with_header() {
    // thead + tfoot reserve fits below the 110pt filler, row1 does not.
    let l = lay(&fixture(110.0, ONE_ROW_HEAD, FOOT));
    let pages = page_texts(&l);
    assert_eq!(count(&pages[0], "HeadA"), 0, "header orphaned on page 1: {pages:?}");
    assert_eq!(count(&pages[0], "FootA"), 0, "footer orphaned on page 1: {pages:?}");
    assert_header_invariants(&l, &["HeadA", "FootA"]);
}

#[test]
fn deferred_header_painted_once() {
    // 138pt filler leaves 6pt: the header itself does not fit.
    let l = lay(&fixture(138.0, ONE_ROW_HEAD, ""));
    assert_header_invariants(&l, &["HeadA", "HeadB"]);
}

#[test]
fn split_header_painted_once_and_whole() {
    // 118pt filler: the first header row fits, the second does not.
    let l = lay(&fixture(118.0, TWO_ROW_HEAD, ""));
    let pages = page_texts(&l);
    assert_header_invariants(&l, &["HeadA", "SubA"]);
    let first = pages.iter().position(|p| count(p, "row1") > 0).unwrap();
    assert_eq!(count(&pages[first], "HeadA"), 1, "{pages:?}");
    assert_eq!(count(&pages[first], "SubA"), 1, "{pages:?}");
}

#[test]
fn guard_header_repeats_once_per_page_from_page_top() {
    // Guard (passes pre-fix): a long table at the page top repeats the
    // header exactly once on every page.
    let rows: String = (1..=20).map(|i| format!("<tr><td>r{i}</td><td>{i}</td></tr>")).collect();
    let html = fixture(0.0, ONE_ROW_HEAD, "").replace(
        "<tr><td>row1</td><td>1</td></tr>",
        &rows,
    );
    let l = lay(&html);
    let pages = page_texts(&l);
    assert!(pages.len() >= 2, "{pages:?}");
    for (pi, p) in pages.iter().enumerate() {
        assert_eq!(count(p, "HeadA"), 1, "page {pi}: {p:?}");
    }
}

#[test]
fn header_taller_than_page_resumes_without_duplicate() {
    // At the page top the table cannot move. A header taller than the page
    // splits (spec edge case) and its continuation resumes on page 2; the
    // resumed header must not also be repeated there (pins the
    // "repeat only after it completed once" sub-rule on its own: the
    // defer-whole rule never fires at a page top).
    let lines: String = (1..=20).map(|i| format!("L{i}<br>")).collect();
    let html = fixture(0.0, &format!("{ONE_ROW_HEAD}<tr><th>{lines}</th><th>T</th></tr>"), "")
        .replace("<div class=\"filler\">filler</div>", "");
    let l = lay(&html);
    let pages = page_texts(&l);
    // No header line twice on one page. (Once the split header completes
    // it repeats whole on later pages, per rule 7 — not asserted here.)
    for (pi, p) in pages.iter().enumerate() {
        for i in 1..=20 {
            let needle = format!("L{i}");
            assert!(count(p, &needle) <= 1, "page {pi}: `{needle}` twice: {p:?}");
        }
        assert!(count(p, "HeadA") <= 1, "page {pi}: {p:?}");
    }
    for r in ["row1", "row2", "row3"] {
        let total: usize = pages.iter().map(|p| count(p, r)).sum();
        assert_eq!(total, 1, "`{r}` must appear exactly once: {pages:?}");
    }
}

#[test]
fn footer_not_orphaned_when_header_does_not_fit() {
    // A two-line header that does not fit below the 118pt filler leaves a
    // zero-height header fragment; the footer must not print on page 1
    // without the header and a body row (credit-note p2 shape).
    let head = "<tr><th>HeadA</th><th>Head<br>B</th></tr>";
    let foot = "<tfoot><tr><td>FootA</td><td>FootB</td></tr><tr><td>FootC</td><td>FootD</td></tr></tfoot>";
    let l = lay(&fixture(118.0, head, foot));
    let pages = page_texts(&l);
    assert_eq!(count(&pages[0], "FootA"), 0, "footer orphaned on page 1: {pages:?}");
    assert_header_invariants(&l, &["HeadA", "FootA", "FootC"]);
    let first = pages.iter().position(|p| count(p, "row1") > 0).unwrap();
    assert_eq!(count(&pages[first], "FootA"), 1, "{pages:?}");
    assert_eq!(count(&pages[first], "FootC"), 1, "{pages:?}");
}
