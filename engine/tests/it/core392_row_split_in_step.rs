//! CORE-392 — a table row split across fragmentainers continues ALL cells in
//! step (css-break-3 parallel flows). Before the fix the row stopped laying
//! cells after the first one that broke, so cell 2 started a fragment late.

use typeanvil::css::Stylesheet;
use typeanvil::dom::Dom;
use typeanvil::frag::{Fragment, FragmentContent};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::layout;

fn inches(v: f64) -> Scalar {
    Scalar(v * 72.0)
}

/// Distinct absolute x origins (rounded) of Background fragments on a page.
fn bg_xs(f: &Fragment, ox: f64, out: &mut Vec<i64>) {
    let x = ox + f.offset.x.get();
    if matches!(f.content, FragmentContent::Background(_)) && f.size.1.get() > 0.0 {
        let r = x.round() as i64;
        if !out.contains(&r) {
            out.push(r);
        }
    }
    for c in &f.children {
        bg_xs(c, x, out);
    }
}

#[test]
fn split_row_cells_start_on_same_fragment() {
    // 5in x 3in, 0.5in margins: 144pt fragmentainer. Two 250pt cells.
    let html = r#"<!DOCTYPE html><html><head><style>body{margin:0}</style></head><body>
<table style="width:100px;border-spacing:0"><tr>
<td style="background:red;padding:0"><div style="height:250pt;background:blue"></div></td>
<td style="background:red;padding:0"><div style="height:250pt;background:blue"></div></td>
</tr></table></body></html>"#;
    let dom = Dom::parse(html).expect("parse");
    let geo = PageGeometry {
        width: inches(5.0),
        height: inches(3.0),
        margin_top: inches(0.5),
        margin_right: inches(0.5),
        margin_bottom: inches(0.5),
        margin_left: inches(0.5),
    };
    let l = layout(&dom, &Stylesheet::parse(""), geo);
    assert_eq!(l.pages.len(), 2, "250pt cells in 144pt pages take 2 pages");
    for (i, p) in l.pages.iter().enumerate() {
        let mut xs = Vec::new();
        bg_xs(&p.root, 0.0, &mut xs);
        assert!(xs.len() >= 2, "page {} must hold both cells, got x origins {:?}", i + 1, xs);
    }
}
