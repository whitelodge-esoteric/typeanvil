//! CORE-387: an element-level `content: ''` (the clearfix idiom) must not
//! replace the element's children. The weasyprint-invoice `footer` carries
//! `content: ''`, which blanked its abspos total table.

use typeanvil::css::Stylesheet;
use typeanvil::dom::Dom;
use typeanvil::frag::{Fragment, FragmentContent};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::layout;

fn geometry() -> PageGeometry {
    PageGeometry {
        width: Scalar(8.5 * 72.0),
        height: Scalar(11.0 * 72.0),
        margin_top: Scalar(72.0),
        margin_right: Scalar(72.0),
        margin_bottom: Scalar(72.0),
        margin_left: Scalar(72.0),
    }
}

fn page_text(html: &str) -> String {
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
    let laid = layout(&dom, &sheet, geometry());
    let mut out = String::new();
    collect(&laid.pages[0].root, &mut out);
    out
}

fn collect(f: &Fragment, out: &mut String) {
    if let FragmentContent::Text(run) = &f.content {
        out.push_str(&run.text);
        out.push(' ');
    }
    for c in &f.children {
        collect(c, out);
    }
}

#[test]
fn empty_string_content_keeps_children() {
    let text = page_text(
        "<style>footer { content: ''; display: block; height: 100pt }</style>\
         <body><footer><p>KEEPME</p></footer></body>",
    );
    assert!(text.contains("KEEPME"), "children dropped: {text:?}");
}

#[test]
fn empty_content_footer_keeps_abspos_table() {
    let text = page_text(
        "<style>footer { content: ''; display: block; height: 100pt }\
         table#total { position: absolute; bottom: 0; margin: 0 -1cm; \
         border: 1cm solid #eee; width: 10cm }</style>\
         <body><p>BODY</p><footer><table id=\"total\"><tr><td>TOTALDUE</td></tr></table></footer></body>",
    );
    assert!(text.contains("TOTALDUE"), "abspos table dropped: {text:?}");
}

#[test]
fn non_empty_content_still_replaces_children() {
    let text = page_text(
        "<style>footer { content: 'GEN'; display: block }</style>\
         <body><footer><p>HIDDEN</p></footer></body>",
    );
    assert!(text.contains("GEN"), "generated content missing: {text:?}");
    assert!(!text.contains("HIDDEN"), "children not replaced: {text:?}");
}
