//! CORE-466: a declared-height inline-block that spans several pages keeps
//! its line box open beside it. The text that follows it on the same line
//! lays at the line top on EVERY page the box occupies (css2 §9.4.2), like
//! the text beside a tall float.

use typeanvil::css::Stylesheet;
use typeanvil::dom::Dom;
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

#[test]
fn text_repeats_beside_inline_block_on_every_page() {
    let html = "<!DOCTYPE html><style>body{margin:0}</style>\
        <div style=\"background:yellow\">\
        <div style=\"display:inline-block;vertical-align:top;width:50px;height:350vh;\
        background:hotpink\"></div>\n On the first page\n</div>\n\
        This text should be at the middle of the fourth page.";
    let l = lay(html);
    assert!(l.pages.len() >= 4, "pages = {}", l.pages.len());
    for (n, page) in l.pages.iter().enumerate().take(4) {
        let mut v = Vec::new();
        texts(&page.root, &mut v);
        assert!(
            v.iter().any(|t| t.contains("On the first page")),
            "page {} lacks the line beside the box: {v:?}",
            n + 1
        );
    }
}
