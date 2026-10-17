//! CORE-393: a multicol container's own background and border paint behind
//! its columns at the container's border-box size.

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::frag::{Fragment, FragmentContent};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::{layout, Layout};

fn inches(v: f64) -> Scalar {
    Scalar(v * 72.0)
}

fn lay(html: &str) -> Layout {
    let dom = Dom::parse(html).expect("parse html");
    let mut css = String::new();
    for (id, node) in dom.nodes.iter().enumerate() {
        if let NodeKind::Element(el) = &node.kind {
            if el.tag == "style" {
                css.push_str(&dom.text_content(id));
            }
        }
    }
    let g = PageGeometry {
        width: inches(5.0),
        height: inches(3.0),
        margin_top: inches(0.5),
        margin_right: inches(0.5),
        margin_bottom: inches(0.5),
        margin_left: inches(0.5),
    };
    layout(&dom, &Stylesheet::parse(&css), g)
}

/// `(kind, x, y, w, h)` of Background / Border fragments, page-absolute.
fn boxes(l: &Layout) -> Vec<(&'static str, f64, f64, f64, f64)> {
    fn rec(f: &Fragment, px: f64, py: f64, out: &mut Vec<(&'static str, f64, f64, f64, f64)>) {
        let ax = px + f.offset.x.get();
        let ay = py + f.offset.y.get();
        let kind = match &f.content {
            FragmentContent::Background(_) => Some("bg"),
            FragmentContent::Border(_) => Some("border"),
            _ => None,
        };
        if let Some(k) = kind {
            out.push((k, ax, ay, f.size.0.get(), f.size.1.get()));
        }
        for c in &f.children {
            rec(c, ax, ay, out);
        }
    }
    let mut out = Vec::new();
    for p in &l.pages {
        rec(&p.root, 0.0, 0.0, &mut out);
    }
    out
}

const BODY: &str = "<p>one two three four five six seven eight nine ten eleven twelve</p>";

#[test]
fn definite_height_container_paints_background() {
    let html = format!(
        "<style>body{{margin:0}} .mc{{columns:4;column-fill:auto;width:100pt;height:100pt;background:green}}</style><div class=\"mc\">{BODY}</div>"
    );
    let b = boxes(&lay(&html));
    assert!(
        b.iter().any(|x| x.0 == "bg" && (x.3 - 100.0).abs() < 0.1 && (x.4 - 100.0).abs() < 0.1),
        "expected a 100x100 background, got {b:?}"
    );
}

#[test]
fn auto_height_container_paints_background() {
    let html = format!(
        "<style>body{{margin:0}} .mc{{columns:2;width:200pt;background:green}}</style><div class=\"mc\">{BODY}</div>"
    );
    let b = boxes(&lay(&html));
    assert!(
        b.iter().any(|x| x.0 == "bg" && (x.3 - 200.0).abs() < 0.1 && x.4 > 1.0),
        "expected a 200pt wide background, got {b:?}"
    );
}

#[test]
fn container_border_paints_with_background() {
    let html = format!(
        "<style>body{{margin:0}} .mc{{columns:2;width:200pt;height:60pt;background:green;border:4pt solid black}}</style><div class=\"mc\">{BODY}</div>"
    );
    let b = boxes(&lay(&html));
    assert!(b.iter().any(|x| x.0 == "bg"), "no background: {b:?}");
    assert!(b.iter().any(|x| x.0 == "border"), "no border: {b:?}");
}
