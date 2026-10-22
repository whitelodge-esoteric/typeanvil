//! CORE-541: the collapsed space after an inline `<math>` survives layout.
//!
//! `build_items` drops white space that opens a text run. A run that
//! continues the line of an inline atomic box (`<math>`) lost its leading
//! space, so "mass m at speed" drew and extracted as "mass mat speed".

use typeanvil::css::Stylesheet;
use typeanvil::dom::Dom;
use typeanvil::frag::{Fragment, FragmentContent};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout;

fn geometry() -> PageGeometry {
    PageGeometry {
        width: Scalar(612.0),
        height: Scalar(792.0),
        margin_top: Scalar(36.0),
        margin_right: Scalar(36.0),
        margin_bottom: Scalar(36.0),
        margin_left: Scalar(36.0),
    }
}

fn walk(f: &Fragment, px: f64, out: &mut Vec<(f64, String)>) {
    if let FragmentContent::Text(run) = &f.content {
        out.push((px + run.baseline.x.get(), run.text.clone()));
    }
    let ax = px + f.offset.x.get();
    for c in &f.children {
        walk(c, ax, out);
    }
}

/// X of the first text run that starts with "at".
fn at_x(body: &str) -> f64 {
    let html = format!("<html><body>{body}</body></html>");
    let dom = Dom::parse(&html).unwrap();
    let laid = layout::layout(&dom, &Stylesheet::parse(""), geometry());
    let mut runs = Vec::new();
    walk(&laid.pages[0].root, 0.0, &mut runs);
    runs.iter()
        .find(|(_, t)| t.starts_with("at"))
        .unwrap_or_else(|| panic!("no run starting with \"at\" in {runs:?}"))
        .0
}

#[test]
fn space_after_inline_math_advances_the_next_word() {
    let glued = at_x("<p>mass <math><mi>m</mi></math>at speed</p>");
    let spaced = at_x("<p>mass <math><mi>m</mi></math> at speed</p>");
    // One space of the 12pt-class body font is about 3pt.
    assert!(
        spaced - glued > 2.0,
        "space after </math> missing: glued x={glued}, spaced x={spaced}"
    );
}
