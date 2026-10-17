//! CORE-239: a `<br>` right after a tall inline-block ends the box's own
//! line. It must not open an extra empty line below the box, so the text
//! after it lands at the same y as the text after a plain-block box.

use typeanvil::css::Stylesheet;
use typeanvil::dom::Dom;
use typeanvil::frag::{Fragment, FragmentContent};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::layout;

fn geo() -> PageGeometry {
    PageGeometry {
        width: Scalar(360.0),
        height: Scalar(216.0),
        margin_top: Scalar(36.0),
        margin_right: Scalar(36.0),
        margin_bottom: Scalar(36.0),
        margin_left: Scalar(36.0),
    }
}

fn lay(html: &str) -> typeanvil::layout::Layout {
    let dom = Dom::parse(html).expect("parse html");
    let mut css = String::new();
    for (id, node) in dom.nodes.iter().enumerate() {
        if let typeanvil::dom::NodeKind::Element(el) = &node.kind {
            if el.tag == "style" {
                css.push_str(&dom.text_content(id));
                css.push('\n');
            }
        }
    }
    let sheet = Stylesheet::parse(&css);
    layout(&dom, &sheet, geo())
}

/// Absolute baseline y of every text run on a fragment tree.
fn baselines(f: &Fragment, oy: f64, out: &mut Vec<f64>) {
    let y = oy + f.offset.y.get();
    if let FragmentContent::Text(run) = &f.content {
        out.push(oy + run.baseline.y.get());
    }
    for c in &f.children {
        baselines(c, y, out);
    }
}

fn last_page_baseline(html: &str) -> (usize, f64) {
    let l = lay(html);
    let n = l.pages.len();
    let mut v = Vec::new();
    baselines(&l.pages[n - 1].root, 0.0, &mut v);
    assert!(!v.is_empty(), "text on last page");
    eprintln!("all baselines: {v:?}");
    (n, *v.last().unwrap())
}

#[test]
fn br_after_tall_inline_block_adds_no_empty_line() {
    let plain = "<!DOCTYPE html><style>:root{print-color-adjust:exact}body{margin:0}</style>\
        <div style=\"width:50px;height:350vh;background:hotpink\"></div>\
        This text should be at the middle of the fourth page.";
    let atomic = "<!DOCTYPE html><style>:root{print-color-adjust:exact}body{margin:0}</style>\
        <div style=\"display:inline-block;vertical-align:top;width:50px;height:350vh;\
        background:hotpink\"></div><br>\nThis text should be at the middle of the fourth page.";
    let (pn, py) = last_page_baseline(plain);
    let (an, ay) = last_page_baseline(atomic);
    eprintln!("plain pages={pn} y={py}; atomic pages={an} y={ay}");
    assert_eq!(pn, an, "same page count");
    assert!((py - ay).abs() < 0.5, "text baseline plain={py} atomic={ay}");
}
