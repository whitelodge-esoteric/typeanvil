//! CORE-558: a row flex item's content-based flex base size (max-content)
//! includes the item's own horizontal padding. It used to hold the content
//! width only, so the box handed to the text was `padding` too narrow: the
//! last word wrapped ("Dec 15," / "2018") and padded chips sat edge to edge
//! (showcase weasyprint-ticket, text layer `5:10PMDEC 15,`).

use typeanvil::css::Stylesheet;
use typeanvil::dom::Dom;
use typeanvil::frag::{Fragment, FragmentContent};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::layout;

fn geo() -> PageGeometry {
    PageGeometry {
        width: Scalar(600.0),
        height: Scalar(400.0),
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
fn padded_auto_width_items_do_not_wrap() {
    let html = "<!DOCTYPE html><style>body{margin:0}</style>\
        <div style=\"display:flex; width:500px;\">\
        <div style=\"padding:0 20px;\">5:10pm</div>\
        <div style=\"padding:0 20px;\">Dec 15, 2018</div>\
        <div style=\"padding:0 20px;\">Coach</div></div>";
    let runs = all_text(html);
    assert!(
        runs.iter().any(|r| r.contains("Dec 15, 2018")),
        "padded flex item wrapped inside its own max-content width: {runs:?}"
    );
}
