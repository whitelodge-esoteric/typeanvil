//! CORE-564: a definite-width in-flow block with `auto` inline margins
//! absorbs the free space (css2 §10.3.3). A lone `margin-left: auto`
//! pushes the box to the end edge; two auto margins center it. The
//! body-background-{vrl,srl}-print refs depend on this.

use typeanvil::css::Stylesheet;
use typeanvil::dom::Dom;
use typeanvil::frag::{Fragment, FragmentContent};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::layout;

fn geo() -> PageGeometry {
    // Harness geometry: 5in x 3in, 0.5in margins -> 288pt content width.
    PageGeometry {
        width: Scalar(360.0),
        height: Scalar(216.0),
        margin_top: Scalar(36.0),
        margin_right: Scalar(36.0),
        margin_bottom: Scalar(36.0),
        margin_left: Scalar(36.0),
    }
}

/// Page-absolute x of every background fragment with the given width.
fn bg_x(html: &str, width: f64) -> Vec<f64> {
    fn rec(f: &Fragment, px: f64, py: f64, w: f64, out: &mut Vec<f64>) {
        let ax = px + f.offset.x.get();
        let ay = py + f.offset.y.get();
        if let FragmentContent::Background(_) = &f.content {
            if (f.size.0.get() - w).abs() < 0.01 {
                out.push(ax);
            }
        }
        for c in &f.children {
            rec(c, ax, ay, w, out);
        }
    }
    let dom = Dom::parse(html).expect("parse html");
    let l = layout(&dom, &Stylesheet::parse("body{margin:0}"), geo());
    let mut out = Vec::new();
    rec(&l.pages[0].root, 0.0, 0.0, width, &mut out);
    out
}

// 200px = 150pt; free space = 288 - 150 = 138pt.
const BOX: &str = "width:200px; height:20px; background:blue";

#[test]
fn margin_left_auto_pushes_box_to_end_edge() {
    let html = format!("<!DOCTYPE html><div style=\"margin-left:auto; {BOX}\"></div>");
    assert_eq!(bg_x(&html, 150.0), vec![174.0]);
}

#[test]
fn margin_left_auto_inside_definite_height_parent() {
    // body-background-vrl-print-ref.html shape.
    let html = format!(
        "<!DOCTYPE html><div style=\"height:100px; background:gray\">\
         <div style=\"margin-left:auto; {BOX}\"></div></div>"
    );
    assert_eq!(bg_x(&html, 150.0), vec![174.0]);
}

#[test]
fn both_auto_margins_center_box() {
    let html = format!("<!DOCTYPE html><div style=\"margin:0 auto; {BOX}\"></div>");
    assert_eq!(bg_x(&html, 150.0), vec![105.0]);
}

#[test]
fn over_constrained_auto_margin_keeps_start_edge() {
    let html = "<!DOCTYPE html><div style=\"margin-left:auto; width:500px; height:20px; background:blue\"></div>";
    assert_eq!(bg_x(html, 375.0), vec![36.0]);
}
