//! CORE-173: a tall inline-block moves the line's baseline down.
//!
//! css2 §10.8.1: a replaced inline box with no in-flow line boxes takes its
//! bottom margin edge as its baseline, so the line box grows to the tallest
//! ascent and the BASELINE moves down to that edge. The text on the line rides
//! the shift. The engine used to leave the text at the line top while the box
//! filled the line, so text and a tall inline-block were drawn in different
//! bands (Chromium on `css-page/margin-boxes/content-003`'s reference puts the
//! text ink at y 39..53 with the box at y 0..49; the engine drew it at 3..18).
//!
//! Invariant asserted here: the text's baseline is the box's bottom edge.

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

/// The single background box as `(x, y, w, h)`, page-absolute.
fn background_box(l: &Layout) -> (f64, f64, f64, f64) {
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
    assert_eq!(out.len(), 1, "one background box: {out:?}");
    out[0]
}

/// The single text run's page-absolute baseline y.
fn text_baseline(l: &Layout) -> f64 {
    fn rec(f: &Fragment, px: f64, py: f64, out: &mut Vec<f64>) {
        let ax = px + f.offset.x.get();
        let ay = py + f.offset.y.get();
        if let FragmentContent::Text(run) = &f.content {
            // A run's baseline is parent-relative (the emitter's rule).
            out.push(py + run.baseline.y.get());
        }
        for c in &f.children {
            rec(c, ax, ay, out);
        }
    }
    let mut out = Vec::new();
    for p in &l.pages {
        rec(&p.root, 0.0, 0.0, &mut out);
    }
    assert_eq!(out.len(), 1, "one text run: {out:?}");
    out[0]
}

const TALL_BOX: &str = r#"<!DOCTYPE html><html><head><style>
  @page { margin: 0; }
  body { margin: 0; }
</style></head><body>
  Hello
  <div style="display:inline-block;width:100px;height:50px;background:lime;"></div>
</body></html>"#;

#[test]
fn tall_inline_block_moves_the_line_baseline_down() {
    let l = lay(TALL_BOX);
    let (_, box_y, _, box_h) = background_box(&l);
    let baseline = text_baseline(&l);

    assert!(
        (box_h - 37.5).abs() < 0.01,
        "box should be 50px = 37.5pt tall, got {box_h}"
    );
    // The box's bottom margin edge is the line's baseline.
    assert!(
        (baseline - (box_y + box_h)).abs() < 0.01,
        "text baseline {baseline} must be the box's bottom edge {}",
        box_y + box_h
    );
    // ...and that is BELOW the strut's own baseline (16px text ≈ 11pt), which
    // is what the bug got wrong: the text stayed at the line top.
    assert!(
        baseline > 20.0,
        "baseline must move down with the box, got {baseline}"
    );
}
