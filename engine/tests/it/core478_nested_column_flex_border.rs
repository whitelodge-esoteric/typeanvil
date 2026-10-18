//! CORE-478: a column flex container nested in a column flex container must
//! be tall enough for its bordered children plus its own border. The content
//! measure (`measure_block`) omitted the box's own top/bottom border, so the
//! nested container came out one border short and clipped its last child
//! (WPT css-break/flexbox 069a/b/c once the author border pass applied).

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeId, NodeKind};
use typeanvil::frag::{Fragment, FragmentContent};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::layout;

fn inches(v: f64) -> Scalar {
    Scalar(v * 72.0)
}

const HTML: &str = r#"<!DOCTYPE html>
<style>
body { margin: 0; font-size: 0.25in; }
.flexbox { display: flex; flex-direction: column; border: 0.25in solid black; }
.item { border: 4px solid purple; }
.nested { border: 4px solid gold; }
</style>
<div class="flexbox">
  <div class="flexbox nested">
    <div class="item">1</div>
    <div class="item">2</div>
  </div>
  <div class="flexbox nested" style="break-before: page">
    <div class="item">3</div>
    <div class="item">4</div>
  </div>
</div>
"#;

fn find_source<'a>(frag: &'a Fragment, id: NodeId, out: &mut Vec<&'a Fragment>) {
    if frag.source == Some(id) {
        out.push(frag);
    }
    for c in &frag.children {
        find_source(c, id, out);
    }
}

fn id_by_class(dom: &Dom, class: &str, nth: usize) -> NodeId {
    dom.nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| matches!(&n.kind, NodeKind::Element(el) if el.classes.iter().any(|c| c == class)))
        .nth(nth)
        .map(|(i, _)| i as NodeId)
        .expect("element with class")
}

#[test]
fn nested_column_flex_contains_bordered_children() {
    let dom = Dom::parse(HTML).expect("parses");
    let s = HTML.find("<style>").unwrap() + 7;
    let e = HTML.find("</style>").unwrap();
    let ss = Stylesheet::parse(&HTML[s..e]);
    let geom = PageGeometry {
        width: inches(5.0),
        height: inches(3.0),
        margin_top: inches(0.5),
        margin_right: inches(0.5),
        margin_bottom: inches(0.5),
        margin_left: inches(0.5),
    };
    let laid = layout(&dom, &ss, geom);
    assert_eq!(laid.pages.len(), 2, "forced break gives two pages");
    let page = &laid.pages[0].root;

    // The outer container fragments here (its 2nd child breaks to page 2),
    // so its bottom border belongs to the LAST fragment only. Painting it
    // on page 1 draws a black band over the nested box's second item.
    let outer_id = id_by_class(&dom, "flexbox", 0);
    let mut outer = Vec::new();
    find_source(page, outer_id, &mut outer);
    let outer = outer[0];
    assert!(outer.break_token.is_some(), "outer fragments on page 1");
    fn bottoms(f: &Fragment, out: &mut Vec<f64>) {
        if let FragmentContent::Border(b) = &f.content {
            out.push(b.bottom.get());
        }
        for c in &f.children {
            bottoms(c, out);
        }
    }
    let mut own = Vec::new();
    if let FragmentContent::Border(b) = &outer.content {
        own.push(b.bottom.get());
    }
    for c in &outer.children {
        if c.source.is_none() {
            bottoms(c, &mut own);
        }
    }
    assert!(
        own.iter().all(|b| b.abs() < 1e-6),
        "fragmented outer flex box paints a bottom border: {own:?}"
    );
}

/// CORE-485: the top border band belongs to the FIRST fragment of a split
/// flex box. On the continuation page it overpainted the first child.
#[test]
fn continuation_fragment_has_no_top_border() {
    let dom = Dom::parse(HTML).expect("parses");
    let s = HTML.find("<style>").unwrap() + 7;
    let e = HTML.find("</style>").unwrap();
    let ss = Stylesheet::parse(&HTML[s..e]);
    let geom = PageGeometry {
        width: inches(5.0),
        height: inches(3.0),
        margin_top: inches(0.5),
        margin_right: inches(0.5),
        margin_bottom: inches(0.5),
        margin_left: inches(0.5),
    };
    let laid = layout(&dom, &ss, geom);
    let outer_id = id_by_class(&dom, "flexbox", 0);
    let mut outer = Vec::new();
    find_source(&laid.pages[1].root, outer_id, &mut outer);
    fn tops(f: &Fragment, out: &mut Vec<f64>) {
        if let FragmentContent::Border(b) = &f.content {
            out.push(b.top.get());
        }
        for c in &f.children {
            if c.source.is_none() {
                tops(c, out);
            }
        }
    }
    let mut own = Vec::new();
    tops(outer[0], &mut own);
    assert!(
        own.iter().all(|t| t.abs() < 1e-6),
        "continuation outer flex box paints a top border: {own:?}"
    );
}
