//! Dashed and dotted borders (CORE-531).
//!
//! css-backgrounds-3 §4.2: `dashed` and `dotted` draw a series of dashes or
//! dots, never a solid line. The border fragment carries each side's line
//! style, and the PDF painter splits a dashed/dotted side into separate
//! rectangles (on-runs) with gaps (off-runs) between them.

use typeanvil::css::{BorderLineStyle, Stylesheet};
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::frag::{BorderBox, Fragment, FragmentContent};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::{layout, Layout};
use typeanvil::pdf::dash_rects;

fn inches(v: f64) -> Scalar {
    Scalar(v * 72.0)
}

fn lay(body: &str) -> Layout {
    let html = format!(
        r#"<!DOCTYPE html><html><head><style>body{{margin:0}}</style></head><body>{body}</body></html>"#
    );
    let dom = Dom::parse(&html).expect("parse html");
    let mut css = String::new();
    for (id, node) in dom.nodes.iter().enumerate() {
        if let NodeKind::Element(el) = &node.kind {
            if el.tag == "style" {
                css.push_str(&dom.text_content(id));
            }
        }
    }
    let sheet = Stylesheet::parse(&css);
    layout(
        &dom,
        &sheet,
        PageGeometry {
            width: inches(4.0),
            height: inches(2.0),
            margin_top: Scalar::ZERO,
            margin_right: Scalar::ZERO,
            margin_bottom: Scalar::ZERO,
            margin_left: Scalar::ZERO,
        },
    )
}

fn first_border(l: &Layout) -> BorderBox {
    fn walk(f: &Fragment) -> Option<BorderBox> {
        if let FragmentContent::Border(b) = &f.content {
            return Some(b.clone());
        }
        f.children.iter().find_map(walk)
    }
    walk(&l.pages[0].root).expect("a border fragment")
}

#[test]
fn layout_carries_dashed_and_dotted_per_side() {
    let l = lay(r#"<div style="border-top:4px dashed #000;border-right:4px dotted #000;border-bottom:4px solid #000;border-left:4px dashed #000;height:30px">x</div>"#);
    let b = first_border(&l);
    assert_eq!(
        b.styles,
        [
            BorderLineStyle::Dashed,
            BorderLineStyle::Dotted,
            BorderLineStyle::Solid,
            BorderLineStyle::Dashed
        ]
    );
}

#[test]
fn zero_width_box_keeps_dashed_left_border() {
    let l = lay(r#"<div style="position:absolute;top:72px;left:14px;width:0;height:60px;border-left:4px dashed #000"></div>"#);
    let b = first_border(&l);
    assert_eq!(b.styles[3], BorderLineStyle::Dashed);
}

/// Count the on-runs of a horizontal side: more than one, separated by gaps,
/// the first flush with the start and the last flush with the end.
#[test]
fn dashed_horizontal_side_has_gaps() {
    let r = dash_rects(BorderLineStyle::Dashed, 10.0, 20.0, 200.0, 3.0, true);
    assert!(r.len() > 3, "expected several dashes, got {}", r.len());
    assert!((r[0].0 - 10.0).abs() < 1e-3);
    let last = r.last().unwrap();
    assert!((last.0 + last.2 - 210.0).abs() < 1e-2);
    for w in r.windows(2) {
        let gap = w[1].0 - (w[0].0 + w[0].2);
        assert!(gap > 0.5, "dashes must be separated by a gap, got {gap}");
    }
}

#[test]
fn dashed_vertical_side_has_gaps() {
    let r = dash_rects(BorderLineStyle::Dashed, 5.0, 0.0, 3.0, 100.0, false);
    assert!(r.len() > 3);
    for w in r.windows(2) {
        assert!(w[1].1 - (w[0].1 + w[0].3) > 0.5);
    }
}

#[test]
fn dotted_side_has_square_dots() {
    let r = dash_rects(BorderLineStyle::Dotted, 0.0, 0.0, 120.0, 4.0, true);
    assert!(r.len() > 5);
    for d in &r {
        assert!((d.2 - 4.0).abs() < 1e-3, "dot length equals thickness");
    }
    for w in r.windows(2) {
        assert!(w[1].0 - (w[0].0 + w[0].2) > 0.5);
    }
}

#[test]
fn solid_side_is_one_rect() {
    let r = dash_rects(BorderLineStyle::Solid, 0.0, 0.0, 120.0, 4.0, true);
    assert_eq!(r.len(), 1);
}
