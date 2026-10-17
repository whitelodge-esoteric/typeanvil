//! Bare-text page-context boundaries (CORE-158).
//!
//! css-page-3 §4.2: a change of page context forces a page break between
//! adjacent in-flow content. Chromium applies that to EVERY boundary between
//! in-flow content whose page context differs — including boundaries next to a
//! bare text run, which takes its containing block's context (bare text
//! directly in `body` takes the default page).
//!
//! The engine previously broke only between two adjacent block-level
//! elements and stopped the comparison at any bare text run. These tests pin
//! the Chromium behavior; see
//! `docs/research/css-page/named-page-boundary-model.md` for the probe
//! evidence that measured it.

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeKind};
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

/// Lay out a document body (with @page margin 0) and return the layout.
fn lay_body(body: &str) -> Layout {
    lay_doc("", "@page { margin: 0; }", body)
}

/// Lay out `<html {html_attrs}>` with `css` and `body`. A `writing-mode` on the
/// ROOT element establishes the PAGE's own flow (css-page-3 §3), which the
/// CORE-155 orthogonal-flow predicate compares against.
fn lay_doc(html_attrs: &str, css: &str, body: &str) -> Layout {
    let html = format!(
        r#"<!DOCTYPE html><html{html_attrs}><head><style>{css}</style></head><body>{body}</body></html>"#
    );
    let dom = Dom::parse(&html).expect("parse html");
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
    layout(&dom, &sheet, geometry())
}

const A: &str = r#"<div style="page:a">A</div>"#;
const B: &str = r#"<div style="page:b">B</div>"#;
const C: &str = r#"<div style="page:a">C</div>"#;

// --- P1/P2: block-to-block boundaries already worked -------------------------

#[test]
fn p1_block_to_block_breaks() {
    assert_eq!(lay_body(&format!("{A}{B}")).pages.len(), 2);
}

#[test]
fn p2_block_chain_breaks() {
    assert_eq!(lay_body(&format!("{A}{B}{C}")).pages.len(), 3);
}

// --- P3-P9: boundaries adjacent to a bare text run ---------------------------

#[test]
fn p3_named_text_named_breaks_twice() {
    // a -> default -> a : two boundaries, three pages.
    assert_eq!(
        lay_body(&format!(r#"{A}X{C}"#)).pages.len(),
        3,
        "a bare text run between two page:a blocks takes the default page, \
         so both boundaries break (Chromium: 3 pages)"
    );
}

#[test]
fn p4_named_then_text_breaks() {
    // a -> default : one boundary, two pages.
    assert_eq!(
        lay_body(&format!("{A}X")).pages.len(),
        2,
        "trailing bare text takes the default page (Chromium: 2 pages)"
    );
}

#[test]
fn p5_text_then_named_breaks() {
    // default -> a : one boundary, two pages.
    assert_eq!(
        lay_body(&format!("X{A}")).pages.len(),
        2,
        "leading bare text takes the default page (Chromium: 2 pages)"
    );
}

#[test]
fn p6_named_text_named_other_breaks_twice() {
    // a -> default -> b : two boundaries, three pages.
    assert_eq!(
        lay_body(&format!("{A}X{B}")).pages.len(),
        3,
        "bare text between page:a and page:b (Chromium: 3 pages)"
    );
}

#[test]
fn p7_nested_then_bare_text_breaks() {
    // The page:b child inside a page:a wrapper, then bare text in that wrapper
    // (context a) -> b -> a : one boundary, two pages.
    assert_eq!(
        lay_body(r#"<div style="page:a"><div style="page:b">B</div>text</div>"#)
            .pages
            .len(),
        2,
        "nested page:b then bare text in the page:a wrapper (Chromium: 2 pages)"
    );
}

#[test]
fn p9_named_text_named_text_breaks_three_times() {
    // a -> default -> b -> default : four pages.
    assert_eq!(
        lay_body(&format!("{A}X{B}Y")).pages.len(),
        4,
        "alternating named and default contexts (Chromium: 4 pages)"
    );
}

// --- the target WPT fixture, reduced ----------------------------------------

#[test]
fn page_name_002_shape_renders_eight_pages() {
    // The first half of .wpt/css/css-page/page-name-002-print.html: the page
    // count depends on bare-text boundaries inside and after the page:a
    // wrapper.
    let body = concat!(
        r#"<div style="page:a;">1st page</div>"#,
        r#"<div style="page:a;"><div style="page:b;">2nd page</div>3rd page</div>"#,
        r#"<div style="page:a;">Also 3rd page</div>"#,
        "4th page",
    );
    assert_eq!(
        lay_body(body).pages.len(),
        4,
        "1st | 2nd | 3rd+Also 3rd | 4th (Chromium renders the fixture's first \
         half as four pages)"
    );
}

// --- CORE-155: page changes inside a mode-SWITCHING wrapper -----------------
//
// The orthogonal-flow suppression tests the writing mode IN EFFECT at the
// page-declaring boxes against the PAGE's own flow mode (css-writing-modes-3
// §7.1), not "is there any intermediate `writing-mode` declaration". The
// orthogonal-writing family pins all four shapes.

#[test]
fn page_change_breaks_when_inner_mode_matches_page_flow() {
    // page-name-orthogonal-writing-004: an `horizontal-tb` wrapper (the page
    // flow's own mode — the root declares nothing) nested inside a
    // `vertical-rl` one. The page:a -> page:b change must break: the fixture's
    // reference forces the same break with `break-after: page`, and Chromium
    // renders both as two pages.
    let body = concat!(
        r#"<div style="writing-mode:vertical-rl">"#,
        r#"<div style="writing-mode:horizontal-tb">"#,
        r#"<div style="page:a">a</div><div style="page:b">b</div>"#,
        r#"</div></div>"#,
    );
    assert_eq!(
        lay_body(body).pages.len(),
        2,
        "an htb wrapper inside a vrl wrapper is NOT orthogonal to an htb page"
    );
}

#[test]
fn page_change_suppressed_when_inner_mode_orthogonal_to_page_flow() {
    // page-name-orthogonal-writing-003: the page flow is horizontal-tb and the
    // page-declaring pair sits DIRECTLY in the `vertical-rl` wrapper, so the
    // subtree is orthogonal and the change stays suppressed (1 page — the
    // fixture's reference has no break and both sides match).
    let body = concat!(
        r#"<div style="writing-mode:vertical-rl">"#,
        r#"<div style="page:a">a</div><div style="page:b">b</div>"#,
        r#"</div>"#,
    );
    assert_eq!(
        lay_body(body).pages.len(),
        1,
        "a vrl subtree under an htb page suppresses the page change"
    );
}
