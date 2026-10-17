//! In-flow flex/grid item stacking contexts (CORE-202).
//!
//! A flex/grid ITEM with a computed `z-index` creates its own stacking
//! context even though it is not positioned (css-flexbox-1 §6.6 / css-grid-1
//! §5). The emitter must paint such an item's whole subtree as one unit,
//! ordered by (z-index, tree order), so a higher-z item's background covers a
//! lower-z one regardless of DOM order.
//!
//! This test pins the engine-visible invariant behind that: the placed item
//! fragments carry a `z_index` stamp — the key the emitter groups paint by —
//! in tree order. Without the stamp the flat-list emitter cannot reorder, and
//! this walk finds no stamps, so the test fails.

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::frag::Fragment;
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

/// Every `z_index` stamp on the first page, in pre-order (tree) order.
fn z_index_stamps(l: &Layout) -> Vec<i32> {
    fn walk(f: &Fragment, out: &mut Vec<i32>) {
        if let Some(z) = f.z_index {
            out.push(z);
        }
        for c in &f.children {
            walk(c, out);
        }
    }
    let mut out = Vec::new();
    walk(&l.pages[0].root, &mut out);
    out
}

#[test]
fn flex_items_carry_z_index_in_tree_order() {
    // Three overlapping flex items declared in REVERSED paint order: DOM order
    // is z=3, z=2, z=1, with negative margins so they overlap. The emitter
    // must paint z=1 first, then z=2, then z=3 on top. The invariant below is
    // the stamp the emitter keys on: tree order still reads 3, 2, 1 — the
    // reorder happens at paint time, not placement time.
    let css = r#"
        @page { margin: 0; }
        body { margin: 0; }
        .edge { display: flex; }
        .box { width: 50px; height: 50px; margin-right: -20px; }
    "#;
    let body = r#"
        <div class="edge">
            <div class="box" style="z-index:3; background:hotpink"></div>
            <div class="box" style="z-index:2; background:cyan"></div>
            <div class="box" style="z-index:1; background:yellow"></div>
        </div>
    "#;
    let l = lay(css, body);
    assert_eq!(
        z_index_stamps(&l),
        vec![3, 2, 1],
        "each in-flow flex item must carry its z_index stamp in tree order; \
         the emitter sorts paint by it so z=3 covers z=2 covers z=1"
    );
}
