//! Page-margin box painting order and `z-index` (CORE-179).
//!
//! css-page-3 §3.1 fixes the page's painting order: page background, document
//! canvas, page borders, document contents, then page-margin boxes. The
//! document canvas, page borders and ALL document contents behave as one
//! `z-index: 0` stacking context, so a margin box paints either in front of
//! that whole group or behind it — never interleaved with it. `z-index`
//! applies to margin boxes as if they were positioned, each box is its own
//! stacking context, and the DEFAULT order among boxes is
//! `@top-left-corner` first, then clockwise.
//!
//! These tests pin the two engine-visible invariants behind that:
//!
//! 1. every margin box carries its spec paint order (box identity, NOT
//!    declaration order — paint-order-002 declares its boxes in a different
//!    order and must paint identically) plus its own `z-index`;
//! 2. a page-anchored out-of-flow box with a NEGATIVE `z-index` attaches
//!    BEFORE the in-flow content, so the pre-order content walk paints it
//!    underneath (CSS2.1 Appendix E: negative stacking contexts paint after
//!    the context's own background, before any in-flow block background).

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::{layout, Layout};
use typeanvil::paged::MarginBoxName;

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

/// Locate the page-root children that carry, respectively, the out-of-flow
/// 50x50px box (37.5pt square) and the in-flow content — the 100x10px block
/// (75x7.5pt) sits INSIDE the in-flow body fragment, so the content's slot is
/// its nearest page-root ancestor. Positions are the emitter's paint order,
/// which is the pre-order walk of `root.children`.
fn root_children_in_paint_order(l: &Layout) -> (usize, usize) {
    fn holds(f: &typeanvil::frag::Fragment, w: f64, h: f64) -> bool {
        (f.size.0.get() == w && f.size.1.get() == h)
            || f.children.iter().any(|c| holds(c, w, h))
    }
    let page = &l.pages[0];
    let abspos = page
        .root
        .children
        .iter()
        .position(|c| c.size.0.get() == 37.5 && c.size.1.get() == 37.5)
        .expect("the 50x50px out-of-flow box has a page-root fragment");
    let content = page
        .root
        .children
        .iter()
        .position(|c| holds(c, 75.0, 7.5))
        .expect("the in-flow 100x10px block sits inside a page-root fragment");
    (abspos, content)
}

// --- the spec's default paint order -----------------------------------------

/// The order is box identity, so a `(name, index)` table is the invariant.
/// Written out in full: a swapped pair would silently change which box wins
/// an overlap.
#[test]
fn paint_order_is_clockwise_from_top_left_corner() {
    use MarginBoxName::*;
    let expected = [
        TopLeftCorner,
        TopLeft,
        TopCenter,
        TopRight,
        TopRightCorner,
        RightTop,
        RightMiddle,
        RightBottom,
        BottomRightCorner,
        BottomRight,
        BottomCenter,
        BottomLeft,
        BottomLeftCorner,
        LeftBottom,
        LeftMiddle,
        LeftTop,
    ];
    for (i, name) in expected.iter().enumerate() {
        assert_eq!(
            name.paint_order() as usize,
            i,
            "{name:?} must be #{i} in the css-page-3 §3.1 default paint order"
        );
    }
    // Every box has a distinct slot: a missing/duplicated index would make two
    // boxes tie and fall back to declaration order.
    let mut seen: Vec<u8> = expected.iter().map(|n| n.paint_order()).collect();
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), 16, "the 16 margin boxes need 16 distinct slots");
}

// --- margin boxes leave the content tree carrying their stacking key --------

#[test]
fn margin_boxes_attach_with_their_stacking_key() {
    // Declared in a deliberately non-clockwise order (paint-order-002 does
    // exactly this): the recorded order must follow the BOX, not the source.
    let css = r#"
        @page {
          margin: 20px;
          @left-top { content: "lt"; }
          @top-left-corner { content: "tlc"; }
          @bottom-right-corner { content: "brc"; }
        }
    "#;
    let l = lay(css, "<div>x</div>");
    let page = &l.pages[0];
    assert_eq!(page.margin_boxes.len(), 3, "three margin boxes attached");

    let mut keys: Vec<(i32, u8)> = page
        .margin_boxes
        .iter()
        .map(|m| (m.z_index, m.order))
        .collect();
    assert!(
        keys.contains(&(0, MarginBoxName::TopLeftCorner.paint_order())),
        "top-left-corner keeps its own slot: {keys:?}"
    );
    assert!(
        keys.contains(&(0, MarginBoxName::LeftTop.paint_order())),
        "left-top keeps its own slot even though it was declared FIRST: {keys:?}"
    );
    assert!(
        keys.contains(&(0, MarginBoxName::BottomRightCorner.paint_order())),
        "bottom-right-corner keeps its own slot: {keys:?}"
    );

    // Margin boxes are their own stacking contexts and never interleave with
    // document content, so they are NOT content-tree children.
    assert!(
        !page.root.children.iter().any(|c| c
            .children
            .iter()
            .any(|g| matches!(g.content, typeanvil::frag::FragmentContent::Border(_)))),
        "a margin box fragment must not sit in the content tree"
    );
    keys.sort();
    keys.dedup();
    assert_eq!(keys.len(), 3, "each box gets its own key");
}

#[test]
fn z_index_is_parsed_in_the_margin_context_and_defaults_to_auto() {
    let css = r#"
        @page {
          margin: 20px;
          @top-center { content: "c"; z-index: -2; }
          @bottom-center { content: "b"; }
          @left-middle { content: "m"; z-index: 7; }
        }
    "#;
    let l = lay(css, "<div>x</div>");
    let page = &l.pages[0];
    let z_of = |order: u8| page.margin_boxes.iter().find(|m| m.order == order).map(|m| m.z_index);

    assert_eq!(
        z_of(MarginBoxName::TopCenter.paint_order()),
        Some(-2),
        "declared negative z-index reaches the fragment"
    );
    assert_eq!(
        z_of(MarginBoxName::LeftMiddle.paint_order()),
        Some(7),
        "declared positive z-index reaches the fragment"
    );
    assert_eq!(
        z_of(MarginBoxName::BottomCenter.paint_order()),
        Some(0),
        "an undeclared z-index is `auto`, which the painting model treats as 0"
    );
}

#[test]
fn an_empty_document_still_renders_its_margin_boxes() {
    let css = r#"@page { margin: 20px; @top-center { content: "head"; } }"#;
    let l = lay(css, "");
    let page = &l.pages[0];
    assert_eq!(
        page.margin_boxes.len(),
        1,
        "margin boxes attach even with no document content"
    );
}

// --- negative z-index out-of-flow content -----------------------------------

/// The invariant that fails without the CSS2.1 Appendix E ordering: the
/// in-flow content walk is pre-order, so "painted below the content" can only
/// mean "earlier in `root.children`".
#[test]
fn negative_z_index_abspos_attaches_before_the_in_flow_content() {
    let css = r#"
        @page { margin: 0; }
        body { margin: 0; }
        .behind { position: absolute; left: 0; top: 0; width: 50px; height: 50px;
                  background: cyan; z-index: -1; }
    "#;
    let body = r#"<div class="behind"></div><div style="width:100px;height:10px;background:#ddd"></div>"#;
    let l = lay(css, body);
    let (abspos, content) = root_children_in_paint_order(&l);
    assert!(
        abspos < content,
        "a negative-z box must attach BEFORE the in-flow content \
         (out-of-flow at {abspos}, content at {content}); the emitter paints \
         root children in order, so attaching after would paint it on top"
    );
}

/// The same geometry with a NON-negative z-index keeps the opposite order —
/// this is what makes the test above specific to the negative case.
#[test]
fn zero_z_index_abspos_still_attaches_after_the_in_flow_content() {
    let css = r#"
        @page { margin: 0; }
        body { margin: 0; }
        .front { position: absolute; left: 0; top: 0; width: 50px; height: 50px;
                 background: cyan; }
    "#;
    let body = r#"<div class="front"></div><div style="width:100px;height:10px;background:#ddd"></div>"#;
    let l = lay(css, body);
    let (abspos, content) = root_children_in_paint_order(&l);
    assert!(
        abspos > content,
        "an auto/0-z box stays in front of the content (out-of-flow at \
         {abspos}, content at {content})"
    );
}
