//! Border widths in viewport units (CORE-468, monolithic-overflow-021).
//!
//! css-values-4 §7.8: `50vh` is half the viewport height (here the page
//! content box: 5in x 3in page, 0.5in margins -> 2in = 144pt tall, 4in =
//! 288pt wide). stylo resolved a `vh` border against its fixed 1024x768
//! viewport, so `border-bottom: 50vh solid` painted a few pixels instead of
//! half a page.

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::geom::{PageGeometry, Scalar};

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

/// Cascade `body` and return (border_top, right, bottom, left) of `#t`.
fn borders(css: &str, body: &str) -> (f64, f64, f64, f64) {
    let html = format!(
        r#"<!DOCTYPE html><html><head><style>{css}</style></head><body>{body}</body></html>"#
    );
    let dom = Dom::parse(&html).expect("parse html");
    let mut sheet_css = String::new();
    let mut target = None;
    for (id, node) in dom.nodes.iter().enumerate() {
        if let NodeKind::Element(el) = &node.kind {
            if el.tag == "style" {
                sheet_css.push_str(&dom.text_content(id));
                sheet_css.push('\n');
            }
            if el.attr("id") == Some("t") {
                target = Some(id);
            }
        }
    }
    let sheet = Stylesheet::parse(&sheet_css);
    let styles = typeanvil::css::cascade(&dom, &sheet, &geometry());
    let s = &styles[target.expect("#t")];
    (
        s.border_top.get(),
        s.border_right.get(),
        s.border_bottom.get(),
        s.border_left.get(),
    )
}

#[test]
fn inline_border_bottom_vh_resolves_against_page_content_height() {
    let (t, r, b, l) = borders("", r#"<div id="t" style="border-bottom:50vh solid green"></div>"#);
    assert!((b - 72.0).abs() < 0.01, "bottom {b}");
    assert_eq!((t, r, l), (0.0, 0.0, 0.0));
}

#[test]
fn stylesheet_border_width_vw_resolves_against_page_content_width() {
    let (t, r, b, l) = borders("#t{border:10vw solid red}", r#"<div id="t"></div>"#);
    for w in [t, r, b, l] {
        assert!((w - 28.8).abs() < 0.01, "width {w}");
    }
}

#[test]
fn inline_beats_stylesheet_and_px_borders_are_untouched() {
    let (_, _, b, _) = borders(
        "#t{border-bottom:10vh solid red}",
        r#"<div id="t" style="border-bottom:20vh solid red"></div>"#,
    );
    assert!((b - 28.8).abs() < 0.01, "bottom {b}");
    let (_, _, b, _) = borders("", r#"<div id="t" style="border-bottom:12px solid red"></div>"#);
    assert!((b - 9.0).abs() < 0.01, "px bottom {b}");
}

#[test]
fn vh_border_without_a_style_stays_zero() {
    let (_, _, b, _) = borders("", r#"<div id="t" style="border-bottom:50vh"></div>"#);
    assert_eq!(b, 0.0);
}
