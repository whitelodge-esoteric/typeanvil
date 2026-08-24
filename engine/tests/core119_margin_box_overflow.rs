//! CORE-119 #2 regression: end-aligned margin boxes must not overflow the page.
//!
//! css-page-3 §3.4.2 (margin-box geometry, Prince 16.2 parity): an
//! end-aligned box anchors its right edge at the content edge and spills
//! LEFTWARD into the middle slot when wider than one third. Right-aligning
//! inside the fixed third slot instead pushed wide running heads past the
//! page's right edge (CORE-119 screenshot 2: "Northwind Systems — Q3 2026"
//! clipped at the page boundary).
//!
//! Invariant: every glyph of a `@top-right`/`@bottom-right` margin box ends
//! at or before the content-box right edge.

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::frag::{Fragment, FragmentContent};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::layout;

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
    layout(&dom, &sheet, geometry())
}

/// Rightmost text extent across all pages (page-relative coordinates).
fn max_text_right(l: &typeanvil::layout::Layout) -> f64 {
    fn rec(f: &Fragment, px: f64, out: &mut f64) {
        if let FragmentContent::Text(run) = &f.content {
            // Advance-based right edge: baseline x + shaped width + expansion.
            let w = run.glyphs.iter().map(|g| g.x_advance.get()).sum::<f64>()
                * (1.0 + run.expansion);
            let right = px + run.baseline.x.get() + w;
            if right > *out {
                *out = right;
            }
        }
        for c in &f.children {
            rec(c, px + f.offset.x.get(), out);
        }
    }
    let mut out = f64::MIN;
    for p in &l.pages {
        rec(&p.root, 0.0, &mut out);
    }
    out
}

const WIDE_HEAD_HTML: &str = r#"<!DOCTYPE html><html><head><style>
@page {
  margin: 0.5in;
  @top-right { content: "Northwind Systems — Q3 2026 Quarterly Operating Results"; font-size: 10pt; }
}
body { font-size: 10pt; }
</style></head><body><p>hello</p></body></html>"#;

#[test]
fn wide_end_aligned_margin_box_stays_inside_content_edge() {
    let l = lay(WIDE_HEAD_HTML);
    // Content box: [36, 324] on a 360pt page. The head is ~139pt wide — wider
    // than the third slot (96pt). The old slot-clamp drew it from
    // x=228 to x=367.5, past the PAGE edge (360).
    let right = max_text_right(&l);
    assert!(
        right <= 324.0,
        "end-aligned margin box overflows the content edge: right={right}"
    );
}
