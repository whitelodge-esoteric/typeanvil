//! List marker acceptance tests (CORE-385, `docs/specifications/list-markers.spec.md`).

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::frag::{Fragment, FragmentContent};
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

/// (text, absolute x of baseline origin) for every text run on page 1.
fn runs(html: &str) -> Vec<(String, f64)> {
    let dom = Dom::parse(html).unwrap();
    let mut css = String::new();
    for (id, node) in dom.nodes.iter().enumerate() {
        if let NodeKind::Element(el) = &node.kind {
            if el.tag == "style" {
                css.push_str(&dom.text_content(id));
            }
        }
    }
    let ss = Stylesheet::parse(&css);
    let l = layout(&dom, &ss, geo());
    let mut out = Vec::new();
    fn walk(f: &Fragment, ox: f64, out: &mut Vec<(String, f64)>) {
        let x = ox + f.offset.x.get();
        if let FragmentContent::Text(r) = &f.content {
            out.push((r.text.clone(), ox + r.baseline.x.get()));
        }
        for c in &f.children {
            walk(c, x, out);
        }
    }
    walk(&l.pages[0].root, 0.0, &mut out);
    out
}

fn texts(html: &str) -> Vec<String> {
    runs(html).into_iter().map(|r| r.0).collect()
}

#[test]
fn ol_and_ul_paint_markers() {
    let t = texts("<ol><li>one</li><li>two</li></ol><ul><li>a</li></ul>");
    assert!(t.contains(&"1. ".to_string()), "{t:?}");
    assert!(t.contains(&"2. ".to_string()), "{t:?}");
    assert!(t.contains(&"\u{2022} ".to_string()), "{t:?}");
}

#[test]
fn ol_start_reversed_and_value() {
    let t = texts("<ol start=\"5\"><li>a</li><li>b</li></ol>");
    assert!(t.contains(&"5. ".to_string()) && t.contains(&"6. ".to_string()), "{t:?}");
    let t = texts("<ol reversed><li>a</li><li>b</li></ol>");
    assert!(t.contains(&"2. ".to_string()) && t.contains(&"1. ".to_string()), "{t:?}");
    let t = texts("<ol><li value=\"7\">a</li><li>b</li></ol>");
    assert!(t.contains(&"7. ".to_string()) && t.contains(&"8. ".to_string()), "{t:?}");
}

#[test]
fn list_style_type_keywords() {
    let t = texts("<style>ol{list-style-type:lower-roman}</style><ol><li>a</li><li>b</li><li>c</li><li>d</li></ol>");
    assert!(t.contains(&"iv. ".to_string()), "{t:?}");
    let t = texts("<style>ol{list-style-type:upper-alpha}</style><ol><li>a</li><li>b</li></ol>");
    assert!(t.contains(&"B. ".to_string()), "{t:?}");
    let t = texts("<style>ul{list-style:none}</style><ul><li>a</li></ul>");
    assert_eq!(t.len(), 1, "no marker expected: {t:?}");
}

#[test]
fn outside_marker_hangs_left_of_content_edge() {
    let r = runs("<ul><li>word</li></ul>");
    let marker = r.iter().find(|(t, _)| t == "\u{2022} ").expect("marker");
    let body = r.iter().find(|(t, _)| t == "word").expect("body");
    assert!(marker.1 < body.1 - 1.0, "marker {} body {}", marker.1, body.1);
}
