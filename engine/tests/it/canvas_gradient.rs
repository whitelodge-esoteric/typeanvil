//! Canvas background gradients and per-page slices (CORE-556).
//!
//! A `linear-gradient` on the root/body propagates to the canvas
//! (css-backgrounds-3 §2.11.2). The canvas is one strip: the page content
//! areas end to end along the page progression axis in document order,
//! page 1 at the strip origin. These tests check the slice geometry and the
//! painted colour directly against the Chromium print of the WPT fixtures
//! `css-page/body-background-{vlr,vrl,slr,srl}-print` at 800x600px pages
//! (page 1 lightgray / page 2 white, except `srl` whose `to left` gradient
//! gives page 1 white / page 2 lightgray). They do not compare two renders
//! of this engine.

use typeanvil::css::{Color, Stylesheet};
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::{layout, Layout};

const LIGHTGRAY: Color = Color { r: 211, g: 211, b: 211, a: 255 };
const WHITE: Color = Color { r: 255, g: 255, b: 255, a: 255 };

fn geometry() -> PageGeometry {
    PageGeometry {
        width: Scalar(360.0),
        height: Scalar(216.0),
        margin_top: Scalar(36.0),
        margin_right: Scalar(36.0),
        margin_bottom: Scalar(36.0),
        margin_left: Scalar(36.0),
    }
}

fn layout_html(html: &str) -> Layout {
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

/// The body-background fixture shape: two monolithic boxes, 800x600px
/// pages with no margin, one gradient on body.
fn fixture(mode: &str, gradient: &str) -> String {
    format!(
        r#"<!DOCTYPE html><html><head><style>
        :root {{ writing-mode: {mode}; }}
        body {{ margin: 0; background: {gradient}; }}
        @page {{ size: 800px 600px; margin: 0; }}
        </style></head><body>
        <div style="width:600px; height:100px; break-inside:avoid; background:blue;"></div>
        <div style="width:800px; height:100px; break-inside:avoid; background:hotpink;"></div>
        </body></html>"#
    )
}

/// Colour of each page's canvas slice at the page content center.
fn page_centers(l: &Layout) -> Vec<Color> {
    l.pages
        .iter()
        .map(|p| {
            let s = p.canvas_gradient.as_ref().expect("canvas gradient slice on every page");
            s.color_at(p.content_size.0.to_f32() / 2.0, p.content_size.1.to_f32() / 2.0)
        })
        .collect()
}

const SPLIT_90: &str = "linear-gradient(90deg, lightgray 0%, lightgray 50%, white 50%, white 100%)";

#[test]
fn vertical_lr_slices_follow_document_order() {
    let l = layout_html(&fixture("vertical-lr", SPLIT_90));
    assert_eq!(l.pages.len(), 2);
    assert_eq!(page_centers(&l), vec![LIGHTGRAY, WHITE]);
}

#[test]
fn vertical_rl_slices_follow_document_order_not_physical() {
    // Pages progress right to left, yet Chromium paints page 1 with the
    // strip's LEFT (lightgray) half: the strip is in document order.
    let l = layout_html(&fixture("vertical-rl", SPLIT_90));
    assert_eq!(l.pages.len(), 2);
    assert_eq!(page_centers(&l), vec![LIGHTGRAY, WHITE]);
}

#[test]
fn sideways_lr_slices_follow_document_order() {
    let l = layout_html(&fixture("sideways-lr", SPLIT_90));
    assert_eq!(l.pages.len(), 2);
    assert_eq!(page_centers(&l), vec![LIGHTGRAY, WHITE]);
}

#[test]
fn sideways_rl_to_left_puts_white_on_page_one() {
    // `to left`: 0% at the strip's right end. Chromium: page 1 white,
    // page 2 lightgray (the fixture's assertion text says the reverse; its
    // reference gradient and the browser agree with this).
    let g = "linear-gradient(to left, lightgray 0%, lightgray 50%, white 50%, white 100%)";
    let l = layout_html(&fixture("sideways-rl", g));
    assert_eq!(l.pages.len(), 2);
    assert_eq!(page_centers(&l), vec![WHITE, LIGHTGRAY]);
}

#[test]
fn vertical_strip_runs_along_x_with_page_offsets() {
    let l = layout_html(&fixture("vertical-rl", SPLIT_90));
    let w = l.pages[0].content_size.0;
    let h = l.pages[0].content_size.1;
    let s0 = l.pages[0].canvas_gradient.as_ref().unwrap();
    let s1 = l.pages[1].canvas_gradient.as_ref().unwrap();
    assert_eq!(s0.strip_size, (Scalar(w.get() * 2.0), h));
    assert_eq!(s0.offset, (Scalar::ZERO, Scalar::ZERO));
    assert_eq!(s1.offset, (w, Scalar::ZERO));
}

#[test]
fn horizontal_strip_runs_along_y() {
    let html = r#"<!DOCTYPE html><html><head><style>
        body { margin: 0; background: linear-gradient(lightgray 50%, white 50%); }
        @page { size: 800px 600px; margin: 0; }
        </style></head><body>
        <div style="height:600px"></div><div style="height:100px"></div>
        </body></html>"#;
    let l = layout_html(html);
    assert_eq!(l.pages.len(), 2);
    let s1 = l.pages[1].canvas_gradient.as_ref().unwrap();
    assert_eq!(s1.offset, (Scalar::ZERO, l.pages[0].content_size.1));
    assert_eq!(page_centers(&l), vec![LIGHTGRAY, WHITE]);
}

#[test]
fn gradient_body_is_the_donor() {
    // The body donates its gradient to the canvas and the color slot
    // stays empty (no solid color competes with the gradient).
    let l = layout_html(&fixture("vertical-lr", SPLIT_90));
    assert!(l.pages.iter().all(|p| p.canvas_gradient.is_some()));
    assert_eq!(l.pages[0].canvas_background, None);
}

#[test]
fn no_gradient_no_slice() {
    let html = r#"<html><head><style>
        body { background: red; }
        </style></head><body>x</body></html>"#;
    let l = layout_html(html);
    assert!(l.pages[0].canvas_gradient.is_none());
    assert_eq!(l.pages[0].canvas_background, Some(Color::rgb(255, 0, 0)));
}

#[test]
fn smooth_gradient_interpolates_and_pads() {
    let html = r#"<html><head><style>
        body { background: linear-gradient(90deg, black, white); }
        </style></head><body>x</body></html>"#;
    let l = layout_html(html);
    let p = &l.pages[0];
    let s = p.canvas_gradient.as_ref().unwrap();
    let w = p.content_size.0.to_f32();
    assert_eq!(s.color_at(0.0, 10.0), Color::rgb(0, 0, 0));
    assert_eq!(s.color_at(w, 10.0), Color::rgb(255, 255, 255));
    let mid = s.color_at(w / 2.0, 10.0);
    assert!((mid.r as i32 - 128).abs() <= 1, "mid {mid:?}");
}

#[test]
fn canvas_gradient_emits_a_pdf_shading() {
    let l = layout_html(&fixture("vertical-lr", SPLIT_90));
    let bytes = typeanvil::pdf::render(&l).unwrap();
    let hay = String::from_utf8_lossy(&bytes);
    assert!(
        hay.contains("/ShadingType") || hay.contains("/Pattern"),
        "no shading/pattern object in the PDF"
    );
}
