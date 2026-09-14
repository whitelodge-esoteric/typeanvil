//! CORE-174: the white space between a text run and a following inline-level
//! atomic survives, and the atomic starts one space further right.
//!
//! css-text-3 §4.1.1 removes white space at a LINE BREAK. A space that precedes
//! an atomic which continues the same line is not at a break, so the atomic has
//! to clear it.
//!
//! The bug: `build_items` leaves the word loop as soon as the white-space run
//! reaches the end of the item's text (`if i >= text.len() { break; }`), before
//! pushing the inter-word glue. The space therefore reached no line's width and
//! a following atomic restarted at the text's INK end. Chromium keeps the space
//! (`Hello` + a 100x50 inline-block starts the box at x=40, not x=36).
//!
//! This test states the rule as an A/B inside one layout call pair: a document
//! with no white space before the atomic puts the box exactly at the text's
//! advance end, and adding a collapsible run moves the box by exactly one
//! inter-word space.

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::frag::{Fragment, FragmentContent};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::{layout, Layout};

fn inches(v: f64) -> Scalar {
    Scalar(v * 72.0)
}

fn geometry() -> PageGeometry {
    PageGeometry {
        width: inches(5.0),
        height: inches(3.0),
        margin_top: inches(0.5),
        margin_right: inches(0.5),
        margin_bottom: inches(0.5),
        margin_left: inches(0.5),
    }
}

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
    let sheet = Stylesheet::parse(&css);
    layout(&dom, &sheet, geometry())
}

/// Background fragments as page-absolute border boxes `(x, y, w, h)`.
fn backgrounds(l: &Layout) -> Vec<(f64, f64, f64, f64)> {
    fn rec(f: &Fragment, px: f64, py: f64, out: &mut Vec<(f64, f64, f64, f64)>) {
        let ax = px + f.offset.x.get();
        let ay = py + f.offset.y.get();
        if let FragmentContent::Background(_) = &f.content {
            out.push((ax, ay, f.size.0.get(), f.size.1.get()));
        }
        for c in &f.children {
            rec(c, ax, ay, out);
        }
    }
    let mut out = Vec::new();
    for p in &l.pages {
        rec(&p.root, 0.0, 0.0, &mut out);
    }
    out
}

/// Every text run's page-absolute right edge.
fn text_rights(l: &Layout) -> Vec<f64> {
    fn rec(f: &Fragment, px: f64, py: f64, out: &mut Vec<f64>) {
        let ax = px + f.offset.x.get();
        let ay = py + f.offset.y.get();
        if let FragmentContent::Text(run) = &f.content {
            let w: f64 =
                run.glyphs.iter().map(|g| g.x_advance.get()).sum::<f64>() * (1.0 + run.expansion);
            out.push(px + run.baseline.x.get() + w);
        }
        for c in &f.children {
            rec(c, ax, ay, out);
        }
    }
    let mut out = Vec::new();
    for p in &l.pages {
        rec(&p.root, 0.0, 0.0, &mut out);
    }
    out
}

const BOX: &str =
    r#"<div style="display:inline-block;width:100px;height:50px;background:lime;"></div>"#;
const HEAD: &str = r#"<!DOCTYPE html><html><head><style>
  @page { margin: 0; }
  body { margin: 0; }
</style></head><body>
  "#;
const TAIL: &str = r#"
</body></html>"#;

/// No white space between the text and the box: the box starts at the text's
/// advance end exactly.
#[test]
fn box_without_preceding_space_starts_at_the_text_end() {
    let html = format!("{HEAD}Hello{BOX}{TAIL}");
    let l = lay(&html);
    let bg = backgrounds(&l);
    assert_eq!(bg.len(), 1, "one background fragment: {bg:?}");
    let (x, _, w, _) = bg[0];
    assert!((w - 75.0).abs() < 0.01, "width 100px = 75pt, got {w}");
    let rights = text_rights(&l);
    assert_eq!(rights.len(), 1, "one text run: {rights:?}");
    assert!(
        (x - rights[0]).abs() < 0.05,
        "without white space the box starts at the text's advance end: \
         box x={x}, text right edge={}",
        rights[0]
    );
}

/// A collapsible white-space run before the box moves it by one inter-word
/// space — the bug left this case identical to the one above.
#[test]
fn collapsible_space_before_box_shifts_it_by_one_space() {
    let tight = lay(&format!("{HEAD}Hello{BOX}{TAIL}"));
    let spaced = lay(&format!("{HEAD}Hello\n  {BOX}{TAIL}"));
    let tight_bg = backgrounds(&tight);
    let spaced_bg = backgrounds(&spaced);
    assert_eq!(tight_bg.len(), 1, "tight: {tight_bg:?}");
    assert_eq!(spaced_bg.len(), 1, "spaced: {spaced_bg:?}");
    let gap = spaced_bg[0].0 - tight_bg[0].0;
    // One space at 16px: ~4.4pt. Bounded loosely so a font change does not
    // break the test, but tight enough to fail when the space disappears.
    assert!(
        gap > 1.0 && gap < 8.0,
        "a collapsible run before the box must add exactly one space of \
         advance (got gap={gap}pt, tight x={}, spaced x={})",
        tight_bg[0].0,
        spaced_bg[0].0
    );
    // The text itself must not move: only the atomic's pen gained the space.
    assert_eq!(
        text_rights(&tight)[0].round(),
        text_rights(&spaced)[0].round(),
        "the text's own advance end is unchanged by the fix"
    );
}
