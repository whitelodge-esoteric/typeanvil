//! CORE-563: the root element's margins offset its box inside the page area
//! (CSS 2.2 §8.3) in every writing mode, `vw`/`vh` margins resolve against
//! the page content box, and a declared-extent box whose viewport-unit sum
//! fills the page exactly is not pushed to the next page by float rounding
//! (css-break/block-002-wm-vlr/vrl).

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeId};
use typeanvil::frag::Fragment;
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::layout;

/// 5in x 3in page, 0.5in margins: content box 288pt x 144pt at (36, 36).
fn geo() -> PageGeometry {
    PageGeometry {
        width: Scalar(360.0),
        height: Scalar(216.0),
        margin_top: Scalar(36.0),
        margin_right: Scalar(36.0),
        margin_bottom: Scalar(36.0),
        margin_left: Scalar(36.0),
    }
}

fn by_id(dom: &Dom, id: &str) -> NodeId {
    dom.nodes
        .iter()
        .position(|n| n.kind.element().is_some_and(|e| e.id.as_deref() == Some(id)))
        .unwrap_or_else(|| panic!("no #{id}"))
}

fn rect_of(f: &Fragment, id: NodeId, px: f64, py: f64) -> Option<(f64, f64, f64, f64)> {
    let (x, y) = (px + f.offset.x.get(), py + f.offset.y.get());
    if f.source == Some(id) && f.size.0.get() > 0.0 {
        return Some((x, y, f.size.0.get(), f.size.1.get()));
    }
    f.children.iter().find_map(|c| rect_of(c, id, x, y))
}

fn first_rect(html: &str) -> (usize, (f64, f64, f64, f64)) {
    first_rect_css(html, "")
}

fn first_rect_css(html: &str, css: &str) -> (usize, (f64, f64, f64, f64)) {
    let dom = Dom::parse(html).unwrap();
    let l = layout(&dom, &Stylesheet::parse(css), geo());
    let b = by_id(&dom, "b");
    let r = rect_of(&l.pages[0].root, b, 0.0, 0.0).expect("#b on page 1");
    (l.pages.len(), r)
}

/// horizontal-tb: `html { margin-top: 36pt }` moves the body down by 36pt.
#[test]
fn root_margin_top_offsets_body_horizontal() {
    let html = r#"<!DOCTYPE html><html style="margin:0;margin-top:36pt"><body style="margin:0"><div id=b style="height:36pt"></div></body></html>"#;
    let (_, (x, y, w, _)) = first_rect(html);
    assert!((y - 72.0).abs() < 0.5, "y={y} want 72 (36 page margin + 36 root margin)");
    assert!((x - 36.0).abs() < 0.5 && (w - 288.0).abs() < 0.5, "x={x} w={w}");
}

/// Root inline margins narrow the body on horizontal-tb.
#[test]
fn root_inline_margins_narrow_body() {
    let html = r#"<!DOCTYPE html><html style="margin:0 10pt 0 20pt"><body style="margin:0"><div id=b style="height:36pt"></div></body></html>"#;
    let (_, (x, _, w, _)) = first_rect(html);
    assert!((x - 56.0).abs() < 0.5, "x={x} want 56");
    assert!((w - 258.0).abs() < 0.5, "w={w} want 258");
}

/// vertical-rl: block-start is the right edge; vertical-lr: the left edge.
#[test]
fn root_margin_block_start_offsets_body_vertical() {
    for (wm, rl) in [("vertical-rl", true), ("vertical-lr", false)] {
        let html = format!(
            r#"<!DOCTYPE html><html style="writing-mode:{wm};margin:0;margin-block-start:72pt"><body style="margin:0"><div id=b style="width:36pt"></div></body></html>"#
        );
        let (_, (x, _, w, _)) = first_rect(&html);
        assert!((w - 36.0).abs() < 0.5, "{wm}: w={w}");
        let want = if rl { 324.0 - 72.0 - 36.0 } else { 36.0 + 72.0 };
        assert!((x - want).abs() < 0.5, "{wm}: x={x} want {want}");
    }
}

/// `margin-block-start: 50vw` on the root resolves against the page content
/// width (288pt), not stylo's 1024px viewport.
#[test]
fn root_viewport_margin_resolves_against_page() {
    let html = r#"<!DOCTYPE html><html><body><div id=b style="width:36pt"></div></body></html>"#;
    let css = "html,body{margin:0;writing-mode:vertical-rl} html{margin-block-start:50vw}";
    let (_, (x, _, _, _)) = first_rect_css(html, css);
    let want = 324.0 - 144.0 - 36.0;
    assert!((x - want).abs() < 0.5, "x={x} want {want}");
}

/// 80vw + 20vw block sizes fill a vertical page exactly; float rounding of
/// the viewport fractions must not push the second box to a new page.
#[test]
fn exact_fit_viewport_block_sizes_stay_on_one_page() {
    for wm in ["vertical-rl", "vertical-lr"] {
        let html = r#"<!DOCTYPE html><html><body><div class=a></div><div class=b id=b></div></body></html>"#;
        let css = format!(
            "html,body{{margin:0;writing-mode:{wm}}} .a{{block-size:80vw}} .b{{block-size:20vw}}"
        );
        let dom = Dom::parse(html).unwrap();
        let l = layout(&dom, &Stylesheet::parse(&css), geo());
        assert_eq!(l.pages.len(), 1, "{wm}: 80vw + 20vw fits one page");
    }
}

/// The root block-start margin is truncated at a break (css-break-3 §5.2):
/// page 2 starts at the content edge. block-002-wm-vrl shape: 80vw margin +
/// 20vw + 100vw + 10vw boxes = 3 pages, each page exactly full.
#[test]
fn root_margin_then_exact_fit_viewport_boxes_paginate_like_chromium() {
    for wm in ["vertical-rl", "vertical-lr"] {
        let html = r#"<!DOCTYPE html><html><body><div class=a></div><div class=b id=b></div><div class=c></div></body></html>"#;
        let css = format!(
            "html,body{{margin:0;writing-mode:{wm}}} html{{margin-block-start:80vw}} \
             .a{{block-size:20vw}} .b{{block-size:100vw}} .c{{block-size:10vw}}"
        );
        let dom = Dom::parse(html).unwrap();
        let l = layout(&dom, &Stylesheet::parse(&css), geo());
        assert_eq!(l.pages.len(), 3, "{wm}: page count");
        let b = by_id(&dom, "b");
        let (x, _, w, _) = rect_of(&l.pages[1].root, b, 0.0, 0.0).expect("#b on page 2");
        assert!((w - 288.0).abs() < 0.5, "{wm}: #b fills page 2, w={w}");
        assert!((x - 36.0).abs() < 0.5, "{wm}: x={x}");
    }
}
