//! CORE-119 #4 regression: single-line cells must stretch to the row height.
//!
//! css-tables-3: the row box is as tall as its tallest cell, and every
//! sibling cell fills that height. The engine laid each cell at its own
//! content height, so single-line cells stopped short of the row's bottom
//! border (visible gaps in the bordered grid — CORE-119 screenshot 4).
//! These tests pin the invariant: sibling cell fragments share one height.

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

/// (x offset, height) of every bordered cell fragment across all pages.
fn bordered_cells(l: &Layout) -> Vec<(f64, f64)> {
    fn rec(f: &Fragment, px: f64, out: &mut Vec<(f64, f64)>) {
        if matches!(f.content, FragmentContent::Border(_)) {
            out.push((px + f.offset.x.get(), f.size.1.get()));
        }
        for c in &f.children {
            rec(c, px + f.offset.x.get(), out);
        }
    }
    let mut out = Vec::new();
    for p in &l.pages {
        // Fragmentainer roots are page-relative; x chain starts at 0.
        for c in &p.root.children {
            rec(c, 0.0, &mut out);
        }
    }
    out
}

const WRAP_ROW_HTML: &str = r#"<!DOCTYPE html><html><head><style>
table { border-collapse: collapse; width: 100%; }
td { border: 0.5pt solid #bbb; padding: 2pt 4pt; font-size: 10pt;
     background-color: #eef; }
</style></head><body>
<table>
<tr><td>short</td><td>anvil standard one hundred fifty pound with a long description that wraps to several lines in a narrow column for sure</td></tr>
</table></body></html>"#;

#[test]
fn single_line_cell_stretches_to_row_height() {
    let l = lay(WRAP_ROW_HTML);
    let cells = bordered_cells(&l);
    assert!(
        cells.len() >= 2,
        "expected two bordered cells, found {}",
        cells.len()
    );
    // First two bordered fragments sit at different column offsets but MUST
    // share the row height.
    let (x0, h0) = cells[0];
    let (x1, h1) = cells[1];
    assert!(
        (x1 - x0).abs() > 1.0,
        "cells should sit at different columns, x0={x0} x1={x1}"
    );
    assert_eq!(
        h0, h1,
        "sibling cells must share the row height (CORE-119 #4)"
    );
}

/// The border CHILD inside a stretched cell must reach the row's bottom edge
/// too. When the cell carries a background (CORE-100), layout_table_cell
/// attaches the border as a child fragment sized to the PRE-stretch cell
/// height; stretching only the parent left the border floating ~one text
/// line above the row bottom (CORE-119 follow-up: header cells with wrapped
/// two-line siblings showed short verticals + an inset bottom border).
#[test]
fn border_child_reaches_row_bottom_in_mixed_height_row() {
    let l = lay(WRAP_ROW_HTML);
    fn check(f: &Fragment, py: f64, failures: &mut Vec<String>) {
        // A cell fragment with a background carries its border as a child of
        // the same size. After the stretch, the child must equal the parent.
        if matches!(f.content, FragmentContent::Background(_)) {
            for c in &f.children {
                if matches!(c.content, FragmentContent::Border(_)) {
                    let ph = f.size.1.get();
                    let ch = c.size.1.get();
                    if (ph - ch).abs() > 0.5 {
                        failures.push(format!(
                            "border child height {ch} != cell height {ph}"
                        ));
                    }
                }
            }
        }
        for c in &f.children {
            check(c, py + f.offset.y.get(), failures);
        }
    }

    let mut failures: Vec<String> = Vec::new();
    for p in &l.pages {
        check(&p.root, 0.0, &mut failures);
    }
    assert!(
        failures.is_empty(),
        "stretched cells must stretch their border children: {failures:?}"
    );
}
