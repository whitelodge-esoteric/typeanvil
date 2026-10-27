//! CORE-591: two layout faults exposed once CORE-547 made the showcase
//! weasyprint-ticket's `flex: 1` section render at all.
//!
//! 1. A finished multicol container (`dl { columns: 4; padding: 1cm 0 }`)
//!    painted its background through its bottom padding, but the flow
//!    advanced only to the end of its content box. The next sibling (the
//!    ticket's chip row) was placed inside the band.
//! 2. Per-line flex-wrap grow counted item margins in the used space while
//!    the block path applies them again, so negative margins over-grew the
//!    item (weasyprint-report's `margin: 0 -2cm` cover bands).

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::frag::{Fragment, FragmentContent};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::{layout, Layout};

fn lay(html: &str) -> Layout {
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
    let g = PageGeometry {
        width: Scalar(400.0),
        height: Scalar(400.0),
        margin_top: Scalar(0.0),
        margin_right: Scalar(0.0),
        margin_bottom: Scalar(0.0),
        margin_left: Scalar(0.0),
    };
    layout(&dom, &Stylesheet::parse(&css), g)
}

/// Absolute (y, height) of every background fragment, plus (text, baseline y).
fn walk(f: &Fragment, px: f64, py: f64, bgs: &mut Vec<(f64, f64, f64, f64)>, texts: &mut Vec<(String, f64)>) {
    let ax = px + f.offset.x.get();
    let ay = py + f.offset.y.get();
    match &f.content {
        FragmentContent::Background(_) => {
            bgs.push((ax, ay, f.size.0.get(), f.size.1.get()));
        }
        FragmentContent::Text(run) => texts.push((run.text.clone(), ay)),
        _ => {}
    }
    for c in &f.children {
        walk(c, ax, ay, bgs, texts);
    }
}

fn collect(l: &Layout) -> (Vec<(f64, f64, f64, f64)>, Vec<(String, f64)>) {
    let (mut bgs, mut texts) = (Vec::new(), Vec::new());
    walk(&l.pages[0].root, 0.0, 0.0, &mut bgs, &mut texts);
    (bgs, texts)
}

#[test]
fn sibling_after_padded_multicol_starts_below_its_band() {
    let html = "<!DOCTYPE html><style>body{margin:0;font-size:10pt}\
        dl{columns:2;margin:0;padding:30pt 0;background:#2A3239}\
        dt,dd{margin:0}</style>\
        <dl><dt>Gate</dt><dd>A1</dd></dl><p style=\"margin:0\">AFTER</p>";
    let (bgs, texts) = collect(&lay(html));
    let band_bottom = bgs
        .iter()
        .map(|b| b.1 + b.3)
        .fold(0.0f64, f64::max);
    assert!(band_bottom > 60.0, "band should include 2x30pt padding: {bgs:?}");
    let after = texts
        .iter()
        .find(|(t, _)| t.contains("AFTER"))
        .expect("AFTER text laid out");
    assert!(
        after.1 >= band_bottom - 0.01,
        "next sibling top {} sits inside the multicol band ending at {band_bottom}",
        after.1
    );
}

#[test]
fn wrap_grow_with_negative_margins_matches_nowrap() {
    // A single-line wrap container must size a flex:1 item exactly as the
    // nowrap path does. Before the fix, wrap counted the -20pt margins in
    // the line's used space and the block path applied them again, so the
    // item grew 40pt wider than its nowrap twin.
    let width = |wrap: &str| {
        let html = format!(
            "<!DOCTYPE html><style>body{{margin:0}}</style>\
            <div style=\"display:flex;flex-wrap:{wrap};margin-left:40pt\">\
            <section style=\"flex:1;margin:0 -20pt;background:#123456;height:10pt\"></section></div>"
        );
        let (bgs, _) = collect(&lay(&html));
        let item = bgs
            .iter()
            .find(|b| (b.3 - 10.0).abs() < 0.5)
            .copied()
            .expect("item background");
        (item.0, item.2)
    };
    let (nx, nw) = width("nowrap");
    let (wx, ww) = width("wrap");
    assert!(
        (ww - nw).abs() < 0.5 && (wx - nx).abs() < 0.5,
        "wrap item (x {wx}, w {ww}) differs from nowrap item (x {nx}, w {nw})"
    );
}
