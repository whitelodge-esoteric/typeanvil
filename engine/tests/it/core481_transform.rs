//! CORE-481: CSS `transform` (translate / rotate / scale) is applied to
//! boxes and to the abspos descendants of a transformed box
//! (WPT css-break/transform-023-print).

use typeanvil::css::Stylesheet;
use typeanvil::dom::Dom;
use typeanvil::frag::Fragment;
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::layout;

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

/// Every transformed fragment as (width, height, matrix).
fn transformed(f: &Fragment, out: &mut Vec<(f64, f64, [f64; 6])>) {
    if let Some(m) = f.transform {
        out.push((f.size.0.get(), f.size.1.get(), m));
    }
    for c in &f.children {
        transformed(c, out);
    }
}

fn all_transformed(l: &typeanvil::layout::Layout) -> Vec<(f64, f64, [f64; 6])> {
    let mut v = Vec::new();
    for p in &l.pages {
        transformed(&p.root, &mut v);
    }
    v
}

#[test]
fn translate_on_zero_size_abspos_box_moves_its_abspos_child() {
    // transform-023: the 51px child must be shifted by 49px (36.75pt).
    let html = "<!DOCTYPE html><style>body{margin:0}</style>\
        <div style=\"width:100px; height:100px; background:red;\">\
        <div style=\"position:absolute; width:0; height:0; transform:translateX(49px);\">\
        <div style=\"position:absolute; width:51px; height:100px; background:green;\"></div>\
        </div></div>";
    let t = all_transformed(&lay(html));
    assert!(
        t.iter().any(|(w, _, m)| (w - 38.25).abs() < 0.01
            && (m[4] - 36.75).abs() < 0.01
            && m[5].abs() < 0.01),
        "51px child lacks a 49px translate: {t:?}"
    );
}

#[test]
fn abspos_child_with_declared_width_is_not_clamped_to_zero_width_parent() {
    let html = "<!DOCTYPE html><style>body{margin:0}</style>\
        <div style=\"position:absolute; width:0; height:0;\">\
        <div style=\"position:absolute; width:51px; height:100px; background:green;\"></div>\
        </div>";
    let l = lay(html);
    let mut widths = Vec::new();
    fn walk(f: &Fragment, out: &mut Vec<f64>) {
        out.push(f.size.0.get());
        for c in &f.children {
            walk(c, out);
        }
    }
    walk(&l.pages[0].root, &mut widths);
    assert!(
        widths.iter().any(|w| (w - 38.25).abs() < 0.01),
        "51px abspos child collapsed: {widths:?}"
    );
}

#[test]
fn rotate_and_scale_attach_a_matrix_about_the_center() {
    let html = "<!DOCTYPE html><style>body{margin:0}</style>\
        <div style=\"width:100px; height:40px; background:blue; transform:rotate(90deg);\"></div>\
        <div style=\"width:100px; height:40px; background:blue; transform:scale(2);\"></div>";
    let t = all_transformed(&lay(html));
    assert_eq!(t.len(), 2, "{t:?}");
    // rotate(90deg): a=0 b=1 c=-1 d=0; center (37.5, 15) stays fixed.
    let (_, _, r) = t[0];
    assert!(r[0].abs() < 1e-9 && (r[1] - 1.0).abs() < 1e-9 && (r[2] + 1.0).abs() < 1e-9);
    let cx = r[0] * 37.5 + r[2] * 15.0 + r[4];
    let cy = r[1] * 37.5 + r[3] * 15.0 + r[5];
    assert!((cx - 37.5).abs() < 0.01 && (cy - 15.0).abs() < 0.01, "center moved: {cx},{cy}");
    // scale(2): a=d=2.
    let (_, _, s) = t[1];
    assert!((s[0] - 2.0).abs() < 1e-9 && (s[3] - 2.0).abs() < 1e-9);
}

#[test]
fn no_transform_leaves_fragments_untransformed() {
    let html = "<!DOCTYPE html><style>body{margin:0}</style>\
        <div style=\"width:100px; height:40px; background:blue;\"></div>";
    assert!(all_transformed(&lay(html)).is_empty());
}

#[test]
fn transform_does_not_move_siblings() {
    // css-transforms-1 §2: a transform never changes layout of other boxes.
    fn y_of_second(html: &str) -> f64 {
        let l = lay(html);
        let mut ys = Vec::new();
        fn walk(f: &Fragment, out: &mut Vec<f64>) {
            if f.size.1.get() > 0.0 && f.source.is_some() {
                out.push(f.offset.y.get());
            }
            for c in &f.children {
                walk(c, out);
            }
        }
        walk(&l.pages[0].root, &mut ys);
        *ys.last().unwrap()
    }
    let a = "<!DOCTYPE html><style>body{margin:0}</style>\
        <div style=\"height:40px;\"></div><div style=\"height:20px;\">x</div>";
    let b = "<!DOCTYPE html><style>body{margin:0}</style>\
        <div style=\"height:40px; transform:translateY(500px);\"></div><div style=\"height:20px;\">x</div>";
    assert_eq!(y_of_second(a), y_of_second(b));
}
