//! CORE-237: declared-height boxes inside monolithic containers (grid rows,
//! floats, inline-blocks) must fragment to the same page count as the same
//! box as a plain block.
//!
//! The block path fragments `height:350vh` across pages (CORE-152/167
//! continuation, layout.rs:4524). The container paths defeat it:
//! - grid lays the item against the full row height (grid.rs:463), so the
//!   continuation never fires;
//! - inline-block lays the atomic against `f64::MAX` (layout.rs:3280);
//! - float lays the box against the page bottom but never routes an over-tall
//!   box through the continuation.
//!
//! Invariant: the page count of a declared-height box is the same whether it
//! is a plain block, a grid item, a float, or an inline-block.

use typeanvil::css::Stylesheet;
use typeanvil::dom::Dom;
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::layout;

/// 5x3in page, 0.5in margins — the harness PageSpec for print reftests.
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

fn lay(html: &str) -> typeanvil::layout::Layout {
    let dom = Dom::parse(html).expect("parse html");
    let mut css = String::new();
    for (id, node) in dom.nodes.iter().enumerate() {
        if let typeanvil::dom::NodeKind::Element(el) = &node.kind {
            if el.tag == "style" {
                css.push_str(&dom.text_content(id));
                css.push('\n');
            }
        }
    }
    let sheet = Stylesheet::parse(&css);
    layout(&dom, &sheet, geo())
}

const BASE: &str = "body { margin: 0 }";

/// The plain-block baseline: `height:350vh` fragments to 4 pages.
#[test]
fn plain_block_declared_height_fragments_to_four_pages() {
    let html = format!(
        "<style>{}</style><div style=\"height:350vh; background:hotpink;\"></div>",
        BASE
    );
    let l = lay(&html);
    assert_eq!(l.pages.len(), 4, "plain block should fragment to 4 pages");
}

/// A grid item with the same declared height must reach the same count.
#[test]
fn grid_item_declared_height_fragments_to_four_pages() {
    let html = format!(
        "<style>{}</style>\
         <div style=\"display:grid; background:yellow;\">\
           <div style=\"contain:size; height:350vh; width:50px; background:hotpink;\"></div>\
         </div>",
        BASE
    );
    let l = lay(&html);
    assert_eq!(l.pages.len(), 4, "grid item should fragment to 4 pages");
}

/// An inline-block with the same declared height must reach the same count.
#[test]
fn inline_block_declared_height_fragments_to_four_pages() {
    let html = format!(
        "<style>{}</style>\
         <div style=\"display:inline-block; vertical-align:top; width:50px; height:350vh; background:hotpink;\"></div><br>",
        BASE
    );
    let l = lay(&html);
    assert_eq!(l.pages.len(), 4, "inline-block should fragment to 4 pages");
}

/// A float wrapper with a declared-height child must reach the same count.
#[test]
fn float_declared_height_fragments_to_four_pages() {
    let html = format!(
        "<style>{}</style>\
         <div style=\"float:left; width:100%; background:yellow;\">\
           <div style=\"contain:size; width:50px; height:350vh; background:hotpink;\"></div>\
         </div>",
        BASE
    );
    let l = lay(&html);
    assert_eq!(l.pages.len(), 4, "float should fragment to 4 pages");
}