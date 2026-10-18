//! Positioned `z-index:auto` boxes paint in tree order (CORE-520).
//!
//! css2 Appendix E: a positioned descendant with `z-index: auto` paints as an
//! atomic unit, in tree order, above earlier positioned boxes. The emitter
//! paints page-wide phases (all backgrounds, then all images), so an abspos
//! `<img>` painted above a LATER abspos box's background. The fix stamps each
//! page-anchored positioned box with a `z_index` so the emitter paints it as
//! its own unit in (z, tree order). This test pins the stamp on both the image
//! box and the later box, in tree order.

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::frag::Fragment;
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::{layout, Layout};

fn inches(v: f64) -> Scalar {
    Scalar(v * 72.0)
}

fn lay(css: &str, body: &str) -> Layout {
    let html = format!(
        r#"<!DOCTYPE html><html><head><style>{css}</style></head><body>{body}</body></html>"#
    );
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
    let geometry = PageGeometry {
        width: inches(5.0),
        height: inches(3.0),
        margin_top: inches(0.5),
        margin_right: inches(0.5),
        margin_bottom: inches(0.5),
        margin_left: inches(0.5),
    };
    layout(&dom, &sheet, geometry)
}

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
fn abspos_auto_boxes_are_stamped_in_tree_order() {
    let css = r#"
        @page { margin: 0; }
        body { margin: 0; }
        .a { position: absolute; top: 0; left: 0; width: 100px; height: 100px; background: red; }
        .b { position: absolute; top: 10px; left: 10px; width: 50px; height: 50px; background: white; }
        .c { position: absolute; top: 20px; left: 20px; width: 20px; height: 20px; background: blue; z-index: 5; }
    "#;
    let body = r#"<div class="a"></div><div class="b"></div><div class="c"></div>"#;
    let l = lay(css, body);
    assert_eq!(
        z_index_stamps(&l),
        vec![0, 0, 5],
        "auto positioned boxes must carry z 0 stamps in tree order so the \
         emitter paints each as a unit; explicit z-index is kept"
    );
}
