//! CORE-171: a `display:inline-block` box with a declared `height` must paint
//! at that height, not at the height of its line box.
//!
//! The bug: `layout_box` (the block path used by the atomic branch) sizes the
//! fragment from its content, and the atomic branch repairs the box height
//! from the resolved margin box. A declared `height` on an inline-block with
//! EMPTY content came out at the surrounding line height (100x19 at 96 DPI for
//! a `100px x 50px` box), which blocked the reference of
//! `css-page/margin-boxes/content-003-print.html`.
//!
//! Invariant asserted here (not the pixel output): the painted background
//! fragment of the inline-block is as tall as its declared height.

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

/// Every background fragment as a page-absolute border box
/// `(x, y, width, height)` in points. A fragment's offset is parent-relative,
/// so the walk accumulates down the tree.
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

/// A tree dump, so a failure shows what the engine actually built.
fn dump(l: &Layout) -> String {
    fn rec(f: &Fragment, depth: usize, out: &mut String) {
        let what = match &f.content {
            FragmentContent::Background(_) => "Background",
            FragmentContent::Border(_) => "Border",
            FragmentContent::Text(run) => {
                out.push_str(&format!("{}Text({:?})\n", "  ".repeat(depth), run.text));
                return;
            }
            FragmentContent::Image(_) => "Image",
            FragmentContent::None => "None",
        };
        out.push_str(&format!(
            "{}{} size=({:.2},{:.2}) offset=({:.2},{:.2})\n",
            "  ".repeat(depth),
            what,
            f.size.0.get(),
            f.size.1.get(),
            f.offset.x.get(),
            f.offset.y.get()
        ));
        for c in &f.children {
            rec(c, depth + 1, out);
        }
    }
    let mut out = String::new();
    for (i, p) in l.pages.iter().enumerate() {
        out.push_str(&format!("page {}:\n", i + 1));
        rec(&p.root, 1, &mut out);
    }
    out
}

const INLINE_BLOCK_HTML: &str = r#"<!DOCTYPE html><html><head><style>
  @page { margin: 0; }
  body { margin: 0; }
</style></head><body>
  <div style="display:inline-block;width:100px;height:50px;background:lime;"></div>
</body></html>"#;

const BLOCK_HTML: &str = r#"<!DOCTYPE html><html><head><style>
  @page { margin: 0; }
  body { margin: 0; }
</style></head><body>
  <div style="width:100px;height:50px;background:lime;"></div>
</body></html>"#;

/// The same box as a block already paints 50px; keep it that way.
#[test]
fn block_declared_height_paints_full_height() {
    let l = lay(BLOCK_HTML);
    let bg = backgrounds(&l);
    assert_eq!(bg.len(), 1, "one background fragment\n{}", dump(&l));
    assert!(
        (bg[0].3 - 37.5).abs() < 0.01,
        "block should be 50px = 37.5pt tall, got {}\n{}",
        bg[0].3,
        dump(&l)
    );
    assert!(bg[0].1.abs() < 0.01, "block starts at the page top\n{}", dump(&l));
}

/// The bug: this asserted 100x19 (the line box) instead of 100x50.
#[test]
fn inline_block_declared_height_paints_full_height() {
    let l = lay(INLINE_BLOCK_HTML);
    let bg = backgrounds(&l);
    assert_eq!(bg.len(), 1, "one background fragment\n{}", dump(&l));
    assert!(
        (bg[0].2 - 75.0).abs() < 0.01,
        "inline-block width should be 100px = 75pt, got {}\n{}",
        bg[0].2,
        dump(&l)
    );
    assert!(
        (bg[0].3 - 37.5).abs() < 0.01,
        "inline-block should paint at its declared 50px = 37.5pt, got {}\n{}",
        bg[0].3,
        dump(&l)
    );
    // The invariant the bug actually broke: the box must be VISIBLE. Aligning
    // a tall inline-block to a short line's baseline shifted it above the
    // page top, so it painted as a 19px slice. Chromium paints it in full
    // from the page top.
    assert!(
        bg[0].1 >= -0.01,
        "inline-block must not start above the page top, got y={}\n{}",
        bg[0].1,
        dump(&l)
    );
}
