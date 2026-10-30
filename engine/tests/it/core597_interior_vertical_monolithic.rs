//! CORE-597: an interior vertical box in a horizontal page flow is
//! monolithic (writing-modes-layout spec Behavior 35). Real fonts.
//!
//! - It moves whole to the next page when it does not fit.
//! - When it is taller than a page it overflows that page; it is never
//!   sliced and adds no pages of its own.
//! - Every word is placed exactly once.
//!
//! Both interior paths are covered: a text-only box (CORE-555 vertical line
//! path) and a box with a block child (`Ctx::layout_interior_vertical`).

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::frag::{Fragment, FragmentContent};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::{layout, Layout};

/// 300pt x 225pt page, no margins.
fn geo() -> PageGeometry {
    PageGeometry {
        width: Scalar(300.0),
        height: Scalar(225.0),
        margin_top: Scalar(0.0),
        margin_right: Scalar(0.0),
        margin_bottom: Scalar(0.0),
        margin_left: Scalar(0.0),
    }
}

fn lay(src: &str) -> Layout {
    let dom = Dom::parse(src).expect("parse");
    let mut css = String::new();
    for (id, n) in dom.nodes.iter().enumerate() {
        if let NodeKind::Element(el) = &n.kind {
            if el.tag == "style" {
                css.push_str(&dom.text_content(id));
            }
        }
    }
    layout(&dom, &Stylesheet::parse(&css), geo())
}

/// A 100px (75pt) block, then a vertical-rl box holding `inner`.
fn doc(vstyle: &str, inner: &str) -> String {
    format!(
        "<!DOCTYPE html><html><head><style>\
         body {{ margin: 0; font-family: sans-serif; font-size: 20px; line-height: 30px }}\
         p {{ margin: 0; height: 100px }}\
         .v {{ writing-mode: vertical-rl; margin: 0; {vstyle} }}\
         </style></head><body><p>before</p><div class=\"v\">{inner}</div></body></html>"
    )
}

fn words() -> Vec<String> {
    (1..=40).map(|i| format!("w{i}")).collect()
}

/// Words per page, plus the max physical bottom of any text line per page.
fn words_by_page(laid: &Layout) -> Vec<(Vec<String>, f64)> {
    fn walk(f: &Fragment, x: f64, y: f64, out: &mut Vec<String>, bottom: &mut f64) {
        let (ax, ay) = (x + f.offset.x.get(), y + f.offset.y.get());
        if let FragmentContent::Text(run) = &f.content {
            out.extend(run.text.split_whitespace().map(str::to_string));
            *bottom = bottom.max(ay + f.size.1.get());
        }
        for c in &f.children {
            walk(c, ax, ay, out, bottom);
        }
    }
    laid.pages
        .iter()
        .map(|p| {
            let (mut w, mut b) = (Vec::new(), 0.0);
            walk(&p.root, 0.0, 0.0, &mut w, &mut b);
            (w, b)
        })
        .collect()
}

fn check(laid: &Layout, expect_overflow: bool) {
    let pages = words_by_page(laid);
    assert_eq!(pages.len(), 2, "box moves whole to page 2, adds no pages: {pages:?}");
    assert_eq!(pages[0].0, vec!["before".to_string()], "page 1 holds only the block");
    let mut all: Vec<String> = pages[1].0.clone();
    all.retain(|w| w.starts_with('w'));
    assert_eq!(all.len(), 40, "every word placed once, on page 2: {:?}", pages[1].0);
    let mut sorted = all.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), 40, "no word duplicated");
    if expect_overflow {
        assert!(pages[1].1 > 225.0, "over-tall box overflows page 2 (bottom {})", pages[1].1);
    } else {
        assert!(pages[1].1 <= 225.01, "auto-height box fits page 2 (bottom {})", pages[1].1);
    }
}

/// Text-only box, auto height: deferred whole, columns capped at the page.
#[test]
fn text_only_vertical_box_defers_whole() {
    check(&lay(&doc("", &words().join(" "))), false);
}

/// Text-only box taller than a page: overflows, never split.
#[test]
fn text_only_vertical_box_taller_than_page_overflows() {
    check(&lay(&doc("height: 600px", &words().join(" "))), true);
}

/// Box with a block child (logical-frame path), auto height: deferred whole.
#[test]
fn mixed_vertical_box_defers_whole() {
    let w = words();
    let inner = format!("<div>{}</div><div>{}</div>", w[..20].join(" "), w[20..].join(" "));
    check(&lay(&doc("", &inner)), false);
}

/// Box with a block child taller than a page: overflows, never split.
#[test]
fn mixed_vertical_box_taller_than_page_overflows() {
    let w = words();
    let inner = format!("<div>{}</div><div>{}</div>", w[..20].join(" "), w[20..].join(" "));
    check(&lay(&doc("height: 600px", &inner)), true);
}
