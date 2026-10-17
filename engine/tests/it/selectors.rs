//! Structural pseudo-class matching across text nodes (CORE-155).
//!
//! The `selectors` crate drives `:nth-of-type` / `:last-of-type` / adjacent
//! sibling matching through `SelectorsElement::prev_sibling_element()` and
//! `next_sibling_element()`. Those accessors must return the nearest ELEMENT
//! sibling, skipping text nodes (mirroring Servo's own DOM implementation).
//! Returning the raw adjacent sibling aborts the walk at any intervening
//! whitespace text node, so `div:nth-of-type(2)` silently failed to match
//! whenever a newline separated the elements, and `:last-of-type` degenerated
//! into matching every sibling.
//!
//! These tests assert the invariant at the layout seam: whitespace between
//! element siblings must not change which elements a structural pseudo-class
//! matches. A matched `div` with `background-color` contributes one
//! `FragmentContent::Background` fragment, so counting those fragments (and
//! their colors) is a direct observation of the selector's decision.

use typeanvil::css::{Color, Stylesheet};
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::frag::{Fragment, FragmentContent};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::{layout, Layout};

// --- helpers ---------------------------------------------------------------

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
    let sheet = Stylesheet::parse(&css);
    layout(&dom, &sheet, geometry())
}

/// Every background fill painted anywhere in the document, in paint order.
fn background_colors(l: &Layout) -> Vec<Color> {
    fn rec(f: &Fragment, out: &mut Vec<Color>) {
        if let FragmentContent::Background(c) = &f.content {
            out.push(*c);
        }
        for c in &f.children {
            rec(c, out);
        }
    }
    let mut out = Vec::new();
    for page in &l.pages {
        rec(&page.root, &mut out);
    }
    out
}

const RED: Color = Color::rgb(255, 0, 0);
const GREEN: Color = Color::rgb(0, 255, 0);

// --- tests -----------------------------------------------------------------

#[test]
fn nth_of_type_matches_across_whitespace_text_nodes() {
    // A newline between the two divs must not break `:nth-of-type(2)`.
    let html = r#"<html><head><style>
        div:nth-of-type(2) { background-color: #ff0000; width: 40pt; height: 20pt; }
    </style></head><body><div>a</div>
<div>b</div></body></html>"#;
    let bg = background_colors(&lay(html));
    assert_eq!(
        bg,
        vec![RED],
        ":nth-of-type(2) must match exactly one div even with a text node between siblings \
         (got {bg:?})"
    );
}

#[test]
fn nth_of_type_parity_with_and_without_whitespace() {
    // The match result must be identical whether or not whitespace separates
    // the siblings — the defect's core invariant.
    let tight = r#"<html><head><style>
        div:nth-of-type(2) { background-color: #ff0000; width: 40pt; height: 20pt; }
    </style></head><body><div>a</div><div>b</div></body></html>"#;
    let spaced = r#"<html><head><style>
        div:nth-of-type(2) { background-color: #ff0000; width: 40pt; height: 20pt; }
    </style></head><body>
        <div>a</div>
        <div>b</div>
    </body></html>"#;
    assert_eq!(background_colors(&lay(tight)), vec![RED]);
    assert_eq!(
        background_colors(&lay(spaced)),
        vec![RED],
        "indentation whitespace must not change the :nth-of-type match"
    );
}

#[test]
fn last_of_type_matches_only_the_last_sibling() {
    // Before the fix this degenerated to matching EVERY sibling.
    let html = r#"<html><head><style>
        div:last-of-type { background-color: #00ff00; width: 40pt; height: 20pt; }
    </style></head><body>
        <div>a</div>
        <div>b</div>
        <div>c</div>
    </body></html>"#;
    let bg = background_colors(&lay(html));
    assert_eq!(
        bg,
        vec![GREEN],
        ":last-of-type must match exactly the last div (got {bg:?})"
    );
}
