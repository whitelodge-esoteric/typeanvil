//! CORE-467: a column flex container (auto height) that holds a tall
//! fragmented block item followed by bare text keeps the text. The text
//! lays on the page where the item ends (WPT css-page/monolithic-overflow-005).

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
fn flex_column_keeps_bare_text_after_tall_item() {
    let html = "<!DOCTYPE html><style>body{margin:0}</style>\
        <div style=\"display:flex; flex-flow:column; background:yellow;\">\
        <div style=\"contain:size; height:350vh; width:50px; background:hotpink;\"></div>\n\
        Yellow background, page 4.\n</div>";
    let l = lay(html);
    assert!(l.pages.len() >= 4, "pages = {}", l.pages.len());
    let mut all = Vec::new();
    for page in &l.pages {
        texts(&page.root, &mut all);
    }
    assert!(
        all.iter().any(|t| t.contains("Yellow background")),
        "bare text in the flex column was dropped: {all:?}"
    );
}
