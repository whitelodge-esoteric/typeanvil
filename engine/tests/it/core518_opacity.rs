//! CORE-518: CSS `opacity` applies group opacity to the element and its
//! content (css-color-4 §3.2). The box fragment carries the opacity, and the
//! PDF composites the subtree through a transparency group (`/ca` < 1 in an
//! ExtGState).

use typeanvil::css::Stylesheet;
use typeanvil::dom::Dom;
use typeanvil::frag::Fragment;
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::{layout, Layout};

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

fn lay(html: &str) -> Layout {
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
    layout(&dom, &Stylesheet::parse(&css), geo())
}

fn collect(f: &Fragment, out: &mut Vec<f32>) {
    if f.opacity < 1.0 {
        out.push(f.opacity);
    }
    for c in &f.children {
        collect(c, out);
    }
}

fn opacities(l: &Layout) -> Vec<f32> {
    let mut v = Vec::new();
    for p in &l.pages {
        collect(&p.root, &mut v);
    }
    v
}

const HTML_HALF: &str = "<!DOCTYPE html><style>body{margin:0}\
    .c{width:100px;height:50px;background:#00f;opacity:.5;border:2px solid #000}\
    </style><div class=\"c\">hi</div>";

const HTML_OPAQUE: &str = "<!DOCTYPE html><style>body{margin:0}\
    .c{width:100px;height:50px;background:#00f;border:2px solid #000}\
    </style><div class=\"c\">hi</div>";

#[test]
fn opacity_is_carried_on_the_box_fragment() {
    let o = opacities(&lay(HTML_HALF));
    assert_eq!(o, vec![0.5], "expected one fragment with opacity 0.5: {o:?}");
}

#[test]
fn default_opacity_leaves_fragments_opaque() {
    assert!(opacities(&lay(HTML_OPAQUE)).is_empty());
}

#[test]
fn out_of_range_opacity_is_clamped() {
    let html = "<!DOCTYPE html><style>body{margin:0}\
        .c{width:100px;height:50px;background:#00f;opacity:-3}\
        </style><div class=\"c\"></div>";
    assert_eq!(opacities(&lay(html)), vec![0.0]);
}

fn has_ca_half(pdf: &[u8]) -> bool {
    let s = String::from_utf8_lossy(pdf);
    s.contains("/ca 0.5") || s.contains("/ca  0.5")
}

#[test]
fn pdf_paints_semitransparent_group_only_when_opacity_set() {
    let half = typeanvil::pdf::render(&lay(HTML_HALF)).expect("render");
    assert!(has_ca_half(&half), "no /ca 0.5 ExtGState in the PDF");
    let opaque = typeanvil::pdf::render(&lay(HTML_OPAQUE)).expect("render");
    assert!(!has_ca_half(&opaque));
}
