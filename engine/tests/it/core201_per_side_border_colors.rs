//! Per-side border colors (CORE-201).
//!
//! css-backgrounds-3 §4.5: `border-bottom-color` sets ONE side's color only.
//! The engine used to fold every per-side color longhand onto a single shared
//! border color, so `border: 10px solid black; border-bottom-color: cyan`
//! repainted all four bands cyan (`border-side-color-repaints`). These tests
//! pin the invariant that each side keeps its OWN color end-to-end:
//!
//! 1. the block layout path attaches a `BorderBox` whose per-side colors
//!    match the cascade (top/right/left stay black, bottom becomes cyan);
//! 2. the hand-rolled `border-*` cascade pass and stylo's computed values
//!    agree on the same per-side resolution;
//! 3. the page-margin-box path keeps each side's OWN declared color instead
//!    of painting every band with the first declared side's color;
//! 4. a side with no color at all still paints (the `currentColor` UA
//!    fallback, resolved to black by consumers).

use typeanvil::css::Stylesheet;
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

fn lay(css: &str, body: &str) -> Layout {
    let html =
        format!(r#"<!DOCTYPE html><html><head><style>{css}</style></head><body>{body}</body></html>"#);
    let dom = Dom::parse(&html).expect("parse html");
    let mut sheet_css = String::new();
    for (id, node) in dom.nodes.iter().enumerate() {
        if let NodeKind::Element(el) = &node.kind {
            if el.tag == "style" {
                sheet_css.push_str(&dom.text_content(id));
                sheet_css.push('\n');
            }
        }
    }
    let sheet = Stylesheet::parse(&sheet_css);
    layout(&dom, &sheet, geometry())
}

/// Collect every `BorderBox` fragment in the tree (page-root pre-order).
fn border_boxes(page: &typeanvil::frag::Fragmentainer) -> Vec<&typeanvil::frag::BorderBox> {
    fn walk<'a>(
        f: &'a Fragment,
        out: &mut Vec<&'a typeanvil::frag::BorderBox>,
    ) {
        if let FragmentContent::Border(b) = &f.content {
            out.push(b);
        }
        for c in &f.children {
            walk(c, out);
        }
    }
    let mut out = Vec::new();
    walk(&page.root, &mut out);
    for mb in &page.margin_boxes {
        walk(&mb.fragment, &mut out);
    }
    out
}

/// The css-backgrounds-3 §4.5 exact case from the issue: a black shorthand
/// border plus `border-bottom-color: cyan` — only the bottom band may be
/// cyan.
#[test]
fn border_bottom_color_paints_only_the_bottom_side() {
    let css = r#"
        .filler { border: 10px solid black; }
        .specialborder { border-bottom-color: cyan; }
    "#;
    let l = lay(css, r#"<div class="filler specialborder">x</div>"#);
    let page = &l.pages[0];

    let b = border_boxes(page)
        .into_iter()
        .find(|b| b.top.get() > 0.0)
        .expect("the box's border attaches as a BorderBox fragment");
    let cyan = typeanvil::css::Color { r: 0, g: 255, b: 255, a: 255 };
    let black = typeanvil::css::Color { r: 0, g: 0, b: 0, a: 255 };

    assert_eq!(b.bottom_color, cyan, "bottom band repainted by the longhand");
    assert_eq!(b.top_color, black, "top band keeps the shorthand black");
    assert_eq!(b.right_color, black, "right band keeps the shorthand black");
    assert_eq!(b.left_color, black, "left band keeps the shorthand black");
}

/// The same case inline: `border-bottom-color` after `border` in ONE style
/// attribute must resolve identically (the inline cascade goes through the
/// same per-side winner slots).
#[test]
fn inline_border_bottom_color_paints_only_the_bottom_side() {
    let css = r#""#;
    let body = r#"<div style="border: 10px solid black; border-bottom-color: cyan">x</div>"#;
    let l = lay(css, body);
    let page = &l.pages[0];

    let b = border_boxes(page)
        .into_iter()
        .find(|b| b.top.get() > 0.0)
        .expect("the box's border attaches as a BorderBox fragment");
    let cyan = typeanvil::css::Color { r: 0, g: 255, b: 255, a: 255 };
    let black = typeanvil::css::Color { r: 0, g: 0, b: 0, a: 255 };

    assert_eq!(b.bottom_color, cyan, "bottom band repainted by the longhand");
    assert_eq!(b.top_color, black, "top band keeps the shorthand black");
    assert_eq!(b.right_color, black, "right band keeps the shorthand black");
    assert_eq!(b.left_color, black, "left band keeps the shorthand black");
}

/// A color ONLY on one side with nothing else declared: the other sides
/// resolve the `currentColor` UA fallback (black) and paint nothing extra.
#[test]
fn colorless_sides_resolve_the_currentcolor_fallback() {
    let css = r#"
        .filler { border: 10px solid; border-bottom-color: cyan; }
    "#;
    let l = lay(css, r#"<div class="filler">x</div>"#);
    let page = &l.pages[0];

    let b = border_boxes(page)
        .into_iter()
        .find(|b| b.top.get() > 0.0)
        .expect("the box's border attaches as a BorderBox fragment");
    let cyan = typeanvil::css::Color { r: 0, g: 255, b: 255, a: 255 };
    let black = typeanvil::css::Color { r: 0, g: 0, b: 0, a: 255 };

    assert_eq!(b.bottom_color, cyan, "the only declared side is cyan");
    assert_eq!(b.top_color, black, "undeclared side resolves `currentColor` to black");
    assert_eq!(b.right_color, black, "undeclared side resolves `currentColor` to black");
    assert_eq!(b.left_color, black, "undeclared side resolves `currentColor` to black");
}

/// Page-margin boxes already keep per-side `(width, colour)` pairs; the
/// fragment must reflect them per side instead of one shared color from the
/// first declared side.
#[test]
fn margin_box_border_keeps_each_sides_own_color() {
    let css = r#"
        @page {
          margin: 20px;
          @top-center {
            content: "t";
            border: 10px solid black;
            border-bottom: 10px solid cyan;
          }
        }
    "#;
    let l = lay(css, "<div>x</div>");
    let page = &l.pages[0];

    let b = border_boxes(page)
        .into_iter()
        .filter(|b| b.top.get() > 0.0)
        .collect::<Vec<_>>();
    // Document content has no border here; the only bordered box is the
    // margin box's own fragment.
    assert!(!b.is_empty(), "the margin box carries a border fragment");
    let b = *b.first().expect("a margin-box border fragment exists");
    let cyan = typeanvil::css::Color { r: 0, g: 255, b: 255, a: 255 };
    let black = typeanvil::css::Color { r: 0, g: 0, b: 0, a: 255 };

    assert_eq!(b.bottom_color, cyan, "the bottom override stays cyan");
    assert_eq!(b.top_color, black, "top keeps the shorthand black");
    assert_eq!(b.right_color, black, "right keeps the shorthand black");
    assert_eq!(b.left_color, black, "left keeps the shorthand black");
}