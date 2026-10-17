//! CORE-390: `position: running(name)` + `content: element(name[, kw])`.
//!
//! Invariants: a running element leaves the body flow, and each page's margin
//! box carries a copy chosen by the css-gcpm-3 §1.3 page policy.

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::frag::{Fragment, FragmentContent};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::layout;

fn inches(v: f64) -> Scalar {
    Scalar(v * 72.0)
}

fn lay(html: &str) -> typeanvil::layout::Layout {
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
    layout(
        &dom,
        &sheet,
        PageGeometry {
            width: inches(5.0),
            height: inches(3.0),
            margin_top: inches(0.5),
            margin_right: inches(0.5),
            margin_bottom: inches(0.5),
            margin_left: inches(0.5),
        },
    )
}

fn texts(f: &Fragment, out: &mut String) {
    if let FragmentContent::Text(r) = &f.content {
        out.push_str(&r.text);
        out.push(' ');
    }
    for c in &f.children {
        texts(c, out);
    }
}

fn body_text(l: &typeanvil::layout::Layout, page: usize) -> String {
    let mut s = String::new();
    texts(&l.pages[page].root, &mut s);
    s
}

fn margin_text(l: &typeanvil::layout::Layout, page: usize) -> String {
    let mut s = String::new();
    for mb in &l.pages[page].margin_boxes {
        texts(&mb.fragment, &mut s);
    }
    s
}

fn has_image(f: &Fragment) -> bool {
    matches!(f.content, FragmentContent::Image(_)) || f.children.iter().any(has_image)
}

/// Six paragraphs at 80px spacing spread over several 5x3in pages; running
/// elements are interleaved so each page sees a different mix.
fn doc(content: &str) -> String {
    format!(
        "<style>.r{{position:running(r)}} @page{{@top-right{{content:{content}}}}} \
         p{{margin:0 0 80px}}</style>\
         <div class=\"r\">AAA</div><p>p1</p><p>p2</p><p>p3</p>\
         <div class=\"r\">BBB</div><p>p4</p><p>p5</p><div class=\"r\">CCC</div><p>p6</p>"
    )
}

#[test]
fn running_element_leaves_body_and_repeats_in_margin_box() {
    let l = lay(&doc("element(r)"));
    assert!(l.pages.len() >= 3, "fixture must paginate: {}", l.pages.len());
    for p in 0..l.pages.len() {
        let body = body_text(&l, p);
        assert!(
            !body.contains("AAA") && !body.contains("BBB") && !body.contains("CCC"),
            "page {p} body still holds a running element: {body}"
        );
        let m = margin_text(&l, p);
        assert!(
            m.contains("AAA") || m.contains("BBB") || m.contains("CCC"),
            "page {p} margin box lacks the running element: {m:?}"
        );
    }
}

#[test]
fn first_policy_prefers_page_assignment_then_carries() {
    let l = lay(&doc("element(r, first)"));
    assert!(margin_text(&l, 0).contains("AAA"));
    // The last page holds CCC's p6 (or later); carried value is never empty.
    let last = l.pages.len() - 1;
    assert!(!margin_text(&l, last).trim().is_empty());
}

#[test]
fn start_policy_ignores_assignments_on_the_page() {
    let l = lay(&doc("element(r, start)"));
    // Page 0 starts before any assignment: nothing to show.
    assert!(margin_text(&l, 0).trim().is_empty(), "{:?}", margin_text(&l, 0));
    assert!(!margin_text(&l, l.pages.len() - 1).trim().is_empty());
}

#[test]
fn last_policy_takes_final_assignment_on_page() {
    let l = lay(&doc("element(r, last)"));
    // Whichever page holds both AAA... the last assigned wins; never AAA once
    // BBB has been seen on the same page.
    for p in 0..l.pages.len() {
        let m = margin_text(&l, p);
        assert!(!(m.contains("AAA") && m.contains("BBB")), "page {p}: {m:?}");
    }
}

#[test]
fn first_except_is_empty_on_pages_that_assign() {
    let l = lay(&doc("element(r, first-except)"));
    // Page 0 assigns AAA, so first-except shows nothing there.
    assert!(margin_text(&l, 0).trim().is_empty(), "{:?}", margin_text(&l, 0));
}

#[test]
fn running_element_with_image_and_text_keeps_both() {
    // 1x1 red PNG.
    let png = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
    let html = format!(
        "<style>.r{{position:running(r)}} @page{{@top-right{{content:element(r)}}}}</style>\
         <div class=\"r\"><img src=\"{png}\" width=\"20\" height=\"20\">LOGO</div><p>body</p>"
    );
    let l = lay(&html);
    assert!(margin_text(&l, 0).contains("LOGO"));
    let has = l.pages[0].margin_boxes.iter().any(|m| has_image(&m.fragment));
    assert!(has, "margin box lacks the running element's image");
    assert!(!body_text(&l, 0).contains("LOGO"));
}
