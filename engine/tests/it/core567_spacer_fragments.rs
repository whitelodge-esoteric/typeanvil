//! CORE-567: a content-free declared-height block (a spacer) that does not
//! fit the remaining page space fragments across the break (css-break-3
//! §4.4 / §5.3). It is not deferred whole. Shape of the
//! body-background-slr-print reference, where Chromium puts 20px of a 500px
//! spacer on page 1 and 480px on page 2.

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

/// Absolute tops of red background fragments, per page.
fn red_tops(f: &Fragment, base: f64, out: &mut Vec<f64>) {
    let top = base + f.offset.y.get();
    if let FragmentContent::Background(c) = &f.content {
        if (c.r, c.g, c.b) == (255, 0, 0) {
            out.push(top);
        }
    }
    for c in &f.children {
        red_tops(c, top, out);
    }
}

#[test]
fn content_free_declared_height_spacer_fragments_at_page_break() {
    // 134pt box, then a 100pt spacer (10pt fits on page 1), then a red box.
    // Fragmented spacer: 90pt on page 2, red at content y 90. Deferred whole
    // (the pre-fix behavior): red at content y 100.
    let css = "html,body{margin:0;padding:0} .a{height:134pt;background:blue} \
        .s{height:100pt} .r{height:20pt;background:red}";
    let body = "<div class=\"a\"></div><div class=\"s\"></div><div class=\"r\"></div>";
    let dom = Dom::parse(&format!("<!DOCTYPE html>{body}")).expect("parse html");
    let l = layout(&dom, &Stylesheet::parse(css), geo());
    assert_eq!(l.pages.len(), 2);
    let mut tops = Vec::new();
    red_tops(&l.pages[1].root, 0.0, &mut tops);
    assert_eq!(tops.len(), 1, "one red fragment on page 2: {tops:?}");
    let content_y = tops[0] - 36.0;
    assert!(
        (content_y - 90.0).abs() < 0.5,
        "red box at content y {content_y}, expected 90 (spacer fragmented)"
    );
}
