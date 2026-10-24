//! CORE-565: a declared-height box whose block child overflows it. The
//! overflow is ink overflow (css-overflow-3, css-break-3 §4.4): it must not
//! push the following siblings to a later page. Shape of the
//! block-002-wm-{vlr,vrl} references, translated to horizontal-tb.

use typeanvil::css::Stylesheet;
use typeanvil::dom::Dom;
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

fn pages(css: &str, body: &str) -> usize {
    let dom = Dom::parse(&format!("<!DOCTYPE html>{body}")).expect("parse html");
    let css = format!("html,body{{margin:0;padding:0}} *{{box-sizing:border-box}} {css}");
    layout(&dom, &Stylesheet::parse(&css), geo()).pages.len()
}

/// b1's declared extent ends exactly at the page-1 bottom; its child
/// overflows onto page 2. b2 (a full page) must start on page 2, so b3
/// lands on page 3. Before the fix b1 was deferred whole to page 2 and its
/// continuation pushed b2 to page 3 (5 pages).
#[test]
fn box_ending_at_page_bottom_does_not_defer_for_overflowing_child() {
    let css = ".sp{height:104pt} .b1{height:40pt;border:2px solid black} \
        .b1>div{height:144pt;background:grey} .b2{height:144pt;background:blue} \
        .b3{height:20pt;background:red}";
    let body = "<div class=\"sp\"></div><div class=\"b1\"><div></div></div>\
        <div class=\"b2\"></div><div class=\"b3\"></div>";
    assert_eq!(pages(css, body), 3);
}

/// Within one page: a 30pt box holding a 90pt child. The following sibling
/// starts 30pt below the box top (the child's overflow is ink overflow),
/// so it overlaps the child (css-overflow-3; block-002-wm-* page 3).
#[test]
fn sibling_follows_declared_extent_not_child_overflow() {
    use typeanvil::frag::{Fragment, FragmentContent};
    fn rec(f: &Fragment, py: f64, out: &mut Vec<(f64, f64)>) {
        let ay = py + f.offset.y.get();
        if let FragmentContent::Background(_) = &f.content {
            out.push((ay, f.size.1.get()));
        }
        for c in &f.children {
            rec(c, ay, out);
        }
    }
    let dom = Dom::parse(
        "<!DOCTYPE html><div style=\"height:30pt\"><div style=\"height:90pt;background:grey\"></div></div>\
         <div style=\"height:20pt;background:blue\"></div>",
    )
    .expect("parse html");
    let css = Stylesheet::parse("html,body{margin:0;padding:0}");
    let l = layout(&dom, &css, geo());
    assert_eq!(l.pages.len(), 1);
    let mut bgs = Vec::new();
    rec(&l.pages[0].root, 0.0, &mut bgs);
    // Content top is 36pt: grey child at 36, blue sibling at 36 + 30.
    assert!(bgs.iter().any(|&(y, h)| (y - 66.0).abs() < 0.01 && (h - 20.0).abs() < 0.01), "{bgs:?}");
}

/// The block-002-wm reference shape (horizontal): three bordered boxes with
/// overflowing 100vh children, after an 80vh spacer. Chromium: 3 pages.
#[test]
fn block_002_reference_shape_is_three_pages() {
    let css = ".sp{height:80vh} \
        .b{width:50vw;border:5px solid black;margin-left:20px} \
        .b1{height:20vh;border-bottom-style:none} \
        .b2{height:100vh;border-top-style:none;border-bottom-style:none} \
        .b3{height:10vh;border-top-style:none} \
        .b>div{width:50vw;height:100vh;border:5px solid grey;margin-left:20px} \
        .b1>div{border-bottom-style:none;margin-top:-5px} \
        .b2>div{border-top-style:none;border-bottom-style:none} \
        .b3>div{height:30vh;border-top-style:none} \
        .after{width:60vw;height:30vh;margin-top:5vh;border:5px solid blue}";
    let body = "<div class=\"sp\"></div>\
        <div class=\"b b1\"><div></div></div>\
        <div class=\"b b2\"><div></div></div>\
        <div class=\"b b3\"><div></div></div>\
        <div class=\"after\"></div>";
    assert_eq!(pages(css, body), 3);
}
