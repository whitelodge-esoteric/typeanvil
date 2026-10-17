//! CORE-394: an atomic inline taller than its multicol column is monolithic
//! (css-break-3 §4.4). It is placed whole in the column and overflows; it is
//! not sliced across columns.

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

/// `(x, y, w, h)` of Background fragments, page-absolute.
fn backgrounds(l: &Layout) -> Vec<(f64, f64, f64, f64)> {
    fn rec(f: &Fragment, px: f64, py: f64, out: &mut Vec<(f64, f64, f64, f64)>) {
        let ax = px + f.offset.x.get();
        let ay = py + f.offset.y.get();
        if matches!(f.content, FragmentContent::Background(_)) {
            out.push((ax, ay, f.size.0.get(), f.size.1.get()));
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

#[test]
fn tall_inline_block_is_not_sliced_across_columns() {
    let html = "<style>body{margin:0} .mc{columns:3;column-fill:auto;column-gap:0;width:300pt;height:50pt} .ib{display:inline-block;width:100pt;height:100pt;background:blue} .r{height:10pt;background:red;width:20pt}</style><div class=\"mc\"><div class=\"ib\"></div></div>";
    let l = lay(html);
    let b: Vec<_> = backgrounds(&l)
        .into_iter()
        .filter(|r| (r.3 - 100.0).abs() < 0.1)
        .collect();
    assert_eq!(b.len(), 1, "box must be one whole fragment: {b:?}");
    assert!((b[0].3 - 100.0).abs() < 0.1, "box must be 100pt tall: {b:?}");
}

#[test]
fn tall_inline_block_nested_in_block_stays_whole() {
    // WPT css-break/tall-line-in-short-fragmentainer-001 shape.
    let html = "<style>body{margin:0} .mc{columns:2;column-fill:auto;column-gap:0;width:100pt;height:50pt} .ib{display:inline-block;vertical-align:top;width:50pt;height:100pt;background:green}</style><div class=\"mc\"><div><div class=\"ib\"></div></div></div>";
    let l = lay(html);
    let b: Vec<_> = backgrounds(&l)
        .into_iter()
        .filter(|r| (r.3 - 100.0).abs() < 0.1)
        .collect();
    assert_eq!(b.len(), 1, "box must be one whole 100pt fragment: {:?}", backgrounds(&l));
}

#[test]
fn tall_inline_block_in_second_flow_item_stays_whole() {
    // Guard: the following block resumes after the whole box, in the next column.
    let html = "<style>body{margin:0} .mc{columns:3;column-fill:auto;column-gap:0;width:300pt;height:50pt} .ib{display:inline-block;width:100pt;height:100pt;background:blue} .r{height:10pt;background:red;width:20pt}</style><div class=\"mc\"><div class=\"ib\"></div><div class=\"r\"></div></div>";
    let l = lay(html);
    let all = backgrounds(&l);
    let blue: Vec<_> = all.iter().filter(|r| (r.3 - 100.0).abs() < 0.1).collect();
    assert_eq!(blue.len(), 1, "{all:?}");
    assert!((blue[0].3 - 100.0).abs() < 0.1, "{all:?}");
}
