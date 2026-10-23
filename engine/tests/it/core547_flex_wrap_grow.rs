//! CORE-547: in a `flex-wrap: wrap` row container a `flex: 1` item grows to
//! fill the free space of its line (css-flexbox-1 §9.7). It used to keep its
//! zero basis, so its text laid out in a zero-width box and vanished from the
//! PDF (showcase weasyprint-ticket: "Dec 15, 2018", "Coach").

use typeanvil::css::Stylesheet;
use typeanvil::dom::Dom;
use typeanvil::frag::{Fragment, FragmentContent};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::layout;

fn geo() -> PageGeometry {
    PageGeometry {
        width: Scalar(360.0),
        height: Scalar(216.0),
        margin_top: Scalar(0.0),
        margin_right: Scalar(0.0),
        margin_bottom: Scalar(0.0),
        margin_left: Scalar(0.0),
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

fn texts(f: &Fragment, out: &mut Vec<String>) {
    if let FragmentContent::Text(run) = &f.content {
        out.push(run.text.clone());
    }
    for c in &f.children {
        texts(c, out);
    }
}

fn all_text(html: &str) -> Vec<String> {
    let l = lay(html);
    let mut all = Vec::new();
    for page in &l.pages {
        texts(&page.root, &mut all);
    }
    all
}

#[test]
fn wrap_container_grows_flex_one_item() {
    let html = "<!DOCTYPE html><style>body{margin:0}</style>\
        <div style=\"display:flex; flex-wrap:wrap; width:300px; justify-content:space-between;\">\
        <section style=\"flex:1;\">Dec 15, 2018 Coach</section>\
        <section style=\"width:80px;\">Ticket</section></div>";
    let all = all_text(html).join(" ");
    assert!(all.contains("Coach"), "flex:1 item text was dropped: {all:?}");
    assert!(all.contains("Ticket"), "fixed item text was dropped: {all:?}");
}

#[test]
fn wrap_container_nowrap_equivalent_text() {
    // Same markup without wrap keeps working (guard).
    let html = "<!DOCTYPE html><style>body{margin:0}</style>\
        <div style=\"display:flex; width:300px;\">\
        <section style=\"flex:1;\">Dec 15, 2018 Coach</section>\
        <section style=\"width:80px;\">Ticket</section></div>";
    let all = all_text(html).join(" ");
    assert!(all.contains("Coach"), "{all:?}");
}
