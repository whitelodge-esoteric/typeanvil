//! CORE-469: a grid container (single implicit column, auto height) that
//! holds a tall fragmented block item followed by bare text keeps the text
//! (WPT css-page/monolithic-overflow-007).

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
fn grid_keeps_bare_text_after_tall_item() {
    let html = "<!DOCTYPE html><style>body{margin:0}</style>\
        <div style=\"display:grid; background:yellow;\">\
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
        "bare text in the grid container was dropped: {all:?}"
    );
}

#[test]
fn grid_with_explicit_columns_still_uses_grid_path() {
    // Guard: a two-column grid of block items (no bare text) keeps both
    // items side by side.
    let html = "<!DOCTYPE html><style>body{margin:0}</style>\
        <div style=\"display:grid; grid-template-columns:100px 100px;\">\
        <div>AAA</div><div>BBB</div></div>";
    let l = lay(html);
    let mut all = Vec::new();
    for page in &l.pages {
        texts(&page.root, &mut all);
    }
    assert!(all.iter().any(|t| t.contains("AAA")));
    assert!(all.iter().any(|t| t.contains("BBB")));
}
