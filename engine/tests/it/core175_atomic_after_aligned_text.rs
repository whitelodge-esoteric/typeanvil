//! CORE-175: an inline-level atomic that follows text on an ALIGNED line must
//! start at the text's aligned advance end, not the line origin.
//!
//! The bug: the full-width bare-text path (no floats, fresh line) computed
//! each line's aligned x but never wrote the atomic pen state, so the first
//! atomic after that text restarted at `inner_left`. For `text-align: right`
//! the box landed a full content width behind the text — the line origin,
//! overlapping nothing else only because the line was otherwise empty.
//!
//! Invariant asserted here: the gap between the text's advance end and the
//! box's left edge is the SAME for `text-align: left` and `text-align: right`
//! (css2 §16.2 — alignment moves the whole line's content; the atomic is part
//! of that line).

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
            // A run's baseline is parent-relative (the emitter's rule).
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

fn text_then_box(align: &str) -> String {
    format!(
        r#"<!DOCTYPE html><html><head><style>
  @page {{ margin: 0; }}
  body {{ margin: 0; }}
  p {{ margin: 0; text-align: {align}; }}
</style></head><body>
  <p>Hello
  <span style="display:inline-block;width:100px;height:50px;background:lime;"></span></p>
</body></html>"#
    )
}

/// The gap between the text's advance end and the box's left edge, for one
/// alignment. The box never overlaps the text under any alignment.
fn gap_for(align: &str) -> (f64, f64) {
    let l = lay(&text_then_box(align));
    let bg = backgrounds(&l);
    assert_eq!(bg.len(), 1, "one background fragment ({align}): {bg:?}");
    let rights = text_rights(&l);
    assert_eq!(rights.len(), 1, "one text run ({align}): {rights:?}");
    (bg[0].0 - rights[0], bg[0].0)
}

/// The bug: with `text-align: right` the box sat at the content box's left
/// edge (-360pt gap), a full content width behind the text.
#[test]
fn atomic_after_right_aligned_text_follows_it() {
    let (gap_left, _x_left) = gap_for("left");
    let (gap_right, x_right) = gap_for("right");
    let (gap_center, x_center) = gap_for("center");

    // Left alignment: the known-good CORE-172 behaviour — one space (~3.33pt).
    assert!(
        gap_left > 0.0 && gap_left < 6.0,
        "left-aligned gap should be one space, got {gap_left}"
    );
    // Right/center: the box must still start AT OR AFTER the text's end.
    assert!(
        gap_right >= -0.01,
        "right-aligned box must not sit behind the text: gap={gap_right} (box x={x_right})"
    );
    assert!(
        (gap_right - gap_left).abs() < 1.0,
        "alignment must not change the text→box gap: left={gap_left}, right={gap_right}"
    );
    assert!(
        gap_center >= -0.01,
        "centered box must not sit behind the text: gap={gap_center} (box x={x_center})"
    );
    assert!(
        (gap_center - gap_left).abs() < 1.0,
        "alignment must not change the text→box gap: left={gap_left}, center={gap_center}"
    );
}
