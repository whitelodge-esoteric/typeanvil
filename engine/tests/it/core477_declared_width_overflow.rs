//! CORE-477: a block with a declared `width` wider than its containing block
//! keeps that width and overflows (css-sizing-3 §5.1); it is not clamped to
//! the container (monolithic-overflow-031 page 1).

use typeanvil::css::Stylesheet;
use typeanvil::dom::Dom;
use typeanvil::frag::{Fragment, FragmentContent};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::layout;

fn geo() -> PageGeometry {
    // Harness geometry: 5in x 3in, 0.5in margins -> 288pt content width.
    PageGeometry {
        width: Scalar(360.0),
        height: Scalar(216.0),
        margin_top: Scalar(36.0),
        margin_right: Scalar(36.0),
        margin_bottom: Scalar(36.0),
        margin_left: Scalar(36.0),
    }
}

fn baselines(f: &Fragment, oy: f64, out: &mut Vec<f64>) {
    let y = oy + f.offset.y.get();
    if let FragmentContent::Text(run) = &f.content {
        out.push((oy + run.baseline.y.get()).round());
    }
    for c in &f.children {
        baselines(c, y, out);
    }
}

fn line_count(html: &str) -> usize {
    let dom = Dom::parse(html).expect("parse html");
    let sheet = Stylesheet::parse("body{margin:0}");
    let l = layout(&dom, &sheet, geo());
    let mut v = Vec::new();
    baselines(&l.pages[0].root, 0.0, &mut v);
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v.dedup();
    v.len()
}

const TEXT: &str = "lorem ipsum dolor sit amet consectetur";

#[test]
fn declared_width_wider_than_container_is_not_clamped() {
    // 50% of 288pt = 144pt; 350px = 262.5pt. The text fits one line at 262pt.
    let html = format!(
        "<!DOCTYPE html><div style=\"width:50%\"><div style=\"width:350px\">{TEXT}</div></div>"
    );
    assert_eq!(line_count(&html), 1, "text must wrap at 350px, not 144pt");
}

#[test]
fn declared_width_narrower_than_container_still_wraps_at_declared() {
    let html = format!(
        "<!DOCTYPE html><div style=\"width:50%\"><div style=\"width:100px\">{TEXT}</div></div>"
    );
    assert!(line_count(&html) > 1);
}
