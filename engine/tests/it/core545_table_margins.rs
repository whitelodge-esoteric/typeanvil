//! CORE-545: a table box honors its horizontal margins. The
//! weasyprint-invoice `table#total` (`margin: 0 -3cm`, abspos) painted at the
//! content-box left edge instead of 3cm further left, so its fixed width ran
//! to the page edge.

use typeanvil::css::Stylesheet;
use typeanvil::dom::Dom;
use typeanvil::frag::{Fragment, FragmentContent};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::layout;

fn geometry() -> PageGeometry {
    PageGeometry {
        width: Scalar(8.5 * 72.0),
        height: Scalar(11.0 * 72.0),
        margin_top: Scalar(72.0),
        margin_right: Scalar(72.0),
        margin_bottom: Scalar(72.0),
        margin_left: Scalar(72.0),
    }
}

/// Absolute (x, width) of the first `Background` fragment found, summing
/// parent-relative offsets down the tree.
fn first_background(f: &Fragment, base_x: f64) -> Option<(f64, f64)> {
    let x = base_x + f.offset.x.get();
    if matches!(f.content, FragmentContent::Background(_)) {
        return Some((x, f.size.0.get()));
    }
    f.children.iter().find_map(|c| first_background(c, x))
}

fn table_box(css: &str) -> (f64, f64) {
    let html = format!(
        "<style>{css}</style><body><table id=\"t\"><tr><td>CELL</td></tr></table></body>"
    );
    let dom = Dom::parse(&html).expect("parse html");
    let mut sheet_src = String::new();
    for (id, node) in dom.nodes.iter().enumerate() {
        if let typeanvil::dom::NodeKind::Element(el) = &node.kind {
            if el.tag == "style" {
                sheet_src.push_str(&dom.text_content(id));
            }
        }
    }
    let sheet = Stylesheet::parse(&sheet_src);
    let laid = layout(&dom, &sheet, geometry());
    first_background(&laid.pages[0].root, 0.0).expect("table background fragment")
}

#[test]
fn in_flow_table_left_margin_shifts_box() {
    let (x, w) = table_box("body{margin:0} table{background:#eee;width:100pt;margin-left:50pt}");
    assert!((x - 122.0).abs() < 0.5, "x = {x}");
    assert!((w - 100.0).abs() < 0.5, "w = {w}");
}

#[test]
fn negative_horizontal_margins_extend_table_past_content_box() {
    let (x, w) = table_box(
        "body{margin:0} table{background:#eee;width:auto;margin:0 -30pt}",
    );
    assert!((x - 42.0).abs() < 0.5, "x = {x}");
    assert!((w - (468.0 + 60.0)).abs() < 0.5, "w = {w}");
}

#[test]
fn abspos_table_negative_margin_left_shifts_box() {
    let (x, w) = table_box(
        "body{margin:0} table{background:#eee;position:absolute;bottom:0;width:200pt;margin:0 -30pt}",
    );
    assert!((x - 42.0).abs() < 0.5, "x = {x}");
    assert!((w - 200.0).abs() < 0.5, "w = {w}");
}
