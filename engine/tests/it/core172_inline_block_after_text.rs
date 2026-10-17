//! CORE-172: an inline-block that follows bare text on the same line must start
//! after that text, not at the line origin.
//!
//! The bug: the bare-text path placed its lines and never advanced the atomic
//! pen state, so the first `Item::Atomic` after text restarted at the content
//! box's left edge and painted over the preceding characters
//! (`Hello` + a 100x50 inline-block put the box at x 0..99 — Chromium puts it
//! at 40..139 for the same input).
//!
//! Invariant asserted here: the box's left edge is at least the preceding
//! text's advance width.

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
            let w: f64 = run.glyphs.iter().map(|g| g.x_advance.get()).sum::<f64>()
                * (1.0 + run.expansion);
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

const TEXT_THEN_BOX: &str = r#"<!DOCTYPE html><html><head><style>
  @page { margin: 0; }
  body { margin: 0; }
</style></head><body>
  Hello
  <div style="display:inline-block;width:100px;height:50px;background:lime;"></div>
</body></html>"#;

/// The bug: the box started at x=0, on top of "Hello".
#[test]
fn inline_block_after_text_starts_after_it() {
    let l = lay(TEXT_THEN_BOX);
    let bg = backgrounds(&l);
    assert_eq!(bg.len(), 1, "one background fragment: {bg:?}");
    let (x, y, w, h) = bg[0];
    assert!((w - 75.0).abs() < 0.01, "width 100px = 75pt, got {w}");
    assert!((h - 37.5).abs() < 0.01, "height 50px = 37.5pt, got {h}");
    assert!(y >= -0.01, "box must not hang above the page, got y={y}");

    let rights = text_rights(&l);
    assert_eq!(rights.len(), 1, "one text run: {rights:?}");
    // The box starts at the text's advance width, so it never overlaps.
    assert!(
        x >= rights[0] - 0.01,
        "inline-block must start after the text: box x={x}, text right edge={}",
        rights[0]
    );
    // "Hello" at 16px is ~30pt; the box is nowhere near the line origin.
    assert!(x > 20.0, "box should follow the text, got x={x}");
}
