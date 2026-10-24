//! CORE-569: a content-free declared-height box fragments at a page break
//! even when it paints (background), in block flow and inside a table cell.
//! A `contain: size` box is monolithic and still defers whole
//! (css-contain-2 §3.2; Chromium monolithic-overflow-018).

use typeanvil::css::Stylesheet;
use typeanvil::dom::Dom;
use typeanvil::frag::{Fragment, FragmentContent};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::layout;

fn geo() -> PageGeometry {
    // Harness geometry: 5in x 3in, 0.5in margins -> 144pt content height.
    PageGeometry {
        width: Scalar(360.0),
        height: Scalar(216.0),
        margin_top: Scalar(36.0),
        margin_right: Scalar(36.0),
        margin_bottom: Scalar(36.0),
        margin_left: Scalar(36.0),
    }
}

/// Absolute (top, height) of background fragments with colour `rgb`.
fn bands(f: &Fragment, base: f64, rgb: (u8, u8, u8), out: &mut Vec<(f64, f64)>) {
    let top = base + f.offset.y.get();
    if let FragmentContent::Background(c) = &f.content {
        if (c.r, c.g, c.b) == rgb {
            out.push((top, f.size.1.get()));
        }
    }
    for c in &f.children {
        bands(c, top, rgb, out);
    }
}

fn render(css: &str, body: &str) -> typeanvil::layout::Layout {
    let dom = Dom::parse(&format!("<!DOCTYPE html>{body}")).expect("parse html");
    layout(&dom, &Stylesheet::parse(css), geo())
}

const GOLD: (u8, u8, u8) = (255, 215, 0);
const GREEN: (u8, u8, u8) = (0, 128, 0);

#[test]
fn painted_content_free_block_fragments_at_page_break() {
    // 100pt box, then a 100pt gold box: 44pt fits on page 1.
    let l = render(
        "html,body{margin:0} .a{height:100pt} .g{height:100pt;background:gold}",
        "<div class=\"a\"></div><div class=\"g\"></div>",
    );
    assert_eq!(l.pages.len(), 2);
    let mut p1 = Vec::new();
    bands(&l.pages[0].root, 0.0, GOLD, &mut p1);
    assert_eq!(p1.len(), 1, "gold slice on page 1 (fragmented): {p1:?}");
    assert!((p1[0].1 - 44.0).abs() < 0.5, "page-1 slice {p1:?}");
}

#[test]
fn painted_spacer_in_declared_height_parent_keeps_deferral() {
    // A painted spacer inside a declared-height parent overflows that parent
    // (ink overflow). It does not slice: the 100pt gold box moves whole to
    // page 2 (section-with-overflow-001 shape, out-of-gate regression).
    let l = render(
        "html,body{margin:0} .a{height:100pt} .p{height:20pt} .g{height:100pt;background:gold}",
        "<div class=\"a\"></div><div class=\"p\"><div class=\"g\"></div></div>",
    );
    let mut p1 = Vec::new();
    bands(&l.pages[0].root, 0.0, GOLD, &mut p1);
    assert!(p1.is_empty(), "no gold slice on page 1: {p1:?}");
}

#[test]
fn table_cell_with_content_free_box_fragments_at_page_break() {
    // table-fragmentation-002a shape: the gold box in a <td> splits across
    // the break like the div-based reference (2 pages, not 3).
    let css = "body{margin:0} table{inline-size:3in;border-spacing:0.25in;\
        border:0.25in solid black;padding:0.25in} td{vertical-align:top;padding:0;\
        border:0.25in solid orange} .content{block-size:1.5in;background:gold}";
    let body = "<div style=\"height:0.25in\">BEFORE</div><table><tbody><tr><td>\
        <div class=\"content\"></div></td></tr></tbody></table>\
        <div style=\"height:0.25in\">AFTER</div>";
    let l = render(css, body);
    assert_eq!(l.pages.len(), 2, "cell content fragments (Chromium: 2 pages)");
    let mut p1 = Vec::new();
    bands(&l.pages[0].root, 0.0, GOLD, &mut p1);
    assert!(!p1.is_empty(), "gold starts on page 1");
}

#[test]
fn contain_size_box_stays_monolithic() {
    // Same shape as the first test, with contain:size: the green box defers
    // whole to page 2.
    let l = render(
        "html,body{margin:0} .a{height:100pt} .g{height:100pt;background:green;contain:size}",
        "<div class=\"a\"></div><div class=\"g\"></div>",
    );
    assert_eq!(l.pages.len(), 2);
    let mut p1 = Vec::new();
    bands(&l.pages[0].root, 0.0, GREEN, &mut p1);
    assert!(p1.is_empty(), "no green on page 1: {p1:?}");
    let mut p2 = Vec::new();
    bands(&l.pages[1].root, 0.0, GREEN, &mut p2);
    assert_eq!(p2.len(), 1);
    assert!((p2[0].1 - 100.0).abs() < 0.5, "whole box on page 2: {p2:?}");
}
