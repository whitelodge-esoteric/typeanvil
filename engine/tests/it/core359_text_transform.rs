//! CORE-359: `text-transform: uppercase | lowercase | capitalize` must change
//! the laid-out text (and therefore the PDF text layer).

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeKind};
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
        if let NodeKind::Element(el) = &node.kind {
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
        out.push('|');
    }
    for c in &f.children {
        collect(c, out);
    }
}

#[test]
fn uppercase_non_ascii() {
    let t = page_text("<style>p{text-transform:uppercase}</style><p>Théodore flight</p>");
    assert!(t.contains("THÉODORE FLIGHT"), "{t}");
}

#[test]
fn lowercase_non_ascii() {
    let t = page_text("<style>p{text-transform:lowercase}</style><p>ÉCOLE Flight</p>");
    assert!(t.contains("école flight"), "{t}");
}

#[test]
fn capitalize_words() {
    let t = page_text("<style>p{text-transform:capitalize}</style><p>théodore o'neil-smith (x)</p>");
    assert!(t.contains("Théodore O'neil-Smith (X)"), "{t}");
}

#[test]
fn inherited_and_overridden_by_child() {
    let t = page_text(
        "<style>p{text-transform:uppercase} .n{text-transform:none}</style>\
         <p>aa <span>bb</span> <b class=n>cc</b></p>",
    );
    assert!(t.contains("AA BB cc"), "{t}");
}

#[test]
fn capitalize_does_not_restart_mid_word_across_inline() {
    let t = page_text("<style>p{text-transform:capitalize}</style><p>ab<span>cd</span> ef</p>");
    assert!(t.contains("Abcd Ef"), "{t}");
}

#[test]
fn none_is_unchanged() {
    let t = page_text("<p>Mixed Case</p>");
    assert!(t.contains("Mixed Case"), "{t}");
}
