//! CORE-368: inline-blocks that hold BLOCK children share one line box.
//!
//! The poster's `#date` puts two `display: inline-block` articles, each with
//! two `<p>`s, side by side. Four defects in the atomic placement path broke
//! that (css2 §9.2.2 / §10.8: atomic inline-level boxes sit on a shared line
//! box, and the line is as tall as its tallest box):
//!
//! 1. The atomic's baseline shift moved every DESCENDANT as well as the box,
//!    though descendants are parent-relative, so text inside a shifted box
//!    landed above the page.
//! 2. The second atomic on a line was laid from the previous box's bottom,
//!    so the line grew to the SUM of its boxes.
//! 3. A wrapped atomic, or one after an in-flow block, kept the old line's
//!    top and pen.
//! 4. The box's own margins were applied twice (`layout_box` applies them).
//!
//! Every test lays out through the production `layout` entry point and reads
//! page-absolute geometry from the fragment tree.

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
        width: inches(8.5),
        height: inches(11.0),
        margin_top: inches(0.5),
        margin_right: inches(0.5),
        margin_bottom: inches(0.5),
        margin_left: inches(0.5),
    }
}

const LEFT: f64 = 36.0;
const TOP: f64 = 36.0;

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
    layout(&dom, &Stylesheet::parse(&css), geometry())
}

/// Background fragments as page-absolute `(x, y, w, h)`, page 1 only.
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
    rec(&l.pages[0].root, 0.0, 0.0, &mut out);
    out
}

/// Page-absolute `(text, baseline x, baseline y)` for every run on page 1.
fn texts(l: &Layout) -> Vec<(String, f64, f64)> {
    fn rec(f: &Fragment, px: f64, py: f64, out: &mut Vec<(String, f64, f64)>) {
        let ax = px + f.offset.x.get();
        let ay = py + f.offset.y.get();
        if let FragmentContent::Text(run) = &f.content {
            // A run's baseline is parent-relative (the emitter's rule).
            out.push((run.text.trim().to_string(), px + run.baseline.x.get(), py + run.baseline.y.get()));
        }
        for c in &f.children {
            rec(c, ax, ay, out);
        }
    }
    let mut out = Vec::new();
    rec(&l.pages[0].root, 0.0, 0.0, &mut out);
    out
}

fn text(l: &Layout, s: &str) -> (f64, f64) {
    texts(l)
        .into_iter()
        .find(|(t, _, _)| t == s)
        .map(|(_, x, y)| (x, y))
        .unwrap_or_else(|| panic!("no text {s:?} in {:?}", texts(l)))
}

const STYLE: &str = "@page { margin: 0.5in } body { margin: 0; font-size: 16px } \
    p { margin: 0 } .b { display: inline-block; width: 1in; background: #ccc }";

fn doc(body: &str) -> String {
    format!("<html><head><style>{STYLE}</style></head><body>{body}</body></html>")
}

/// Defects 1 + 2: the second box sits on the SAME line as the first (equal
/// tops), its text is inside its own box, and the following block starts
/// below the TALLER box — not below the sum of both.
#[test]
fn two_inline_blocks_with_block_children_share_one_line() {
    let l = lay(&doc(
        "<div><span class=\"b\"><p>A1</p><p>A2</p><p>A3</p></span> \
         <span class=\"b\"><p>B1</p></span></div><p>MARKER</p>",
    ));
    let bg = backgrounds(&l);
    assert_eq!(bg.len(), 2, "two box backgrounds: {bg:?}");
    let (ax, ay, _, ah) = bg[0];
    let (bx, by, _, bh) = bg[1];
    assert!(bx > ax + 70.0, "B packs to the right of A: {bg:?}");
    assert!(by >= ay - 0.01, "B is not above the line: A y={ay}, B y={by}");
    assert!(by + bh <= ay + ah + 0.01, "B fits inside the line A sets: {bg:?}");

    let (_, b1) = text(&l, "B1");
    assert!(b1 > by && b1 < by + bh + 0.01, "B1 baseline {b1} inside B's box {by}..{}", by + bh);
    let (_, a3) = text(&l, "A3");
    assert!(a3 > ay && a3 < ay + ah + 0.01, "A3 baseline {a3} inside A's box");

    let (_, marker) = text(&l, "MARKER");
    let line_bottom = (ay + ah).max(by + bh);
    assert!(
        marker < line_bottom + 25.0,
        "MARKER baseline {marker} follows the line bottom {line_bottom}, not the SUM of both boxes ({})",
        TOP + ah + bh
    );
}

/// Defect 3 (wrap): the third box wraps to a NEW line whose top is the first
/// line's bottom, and its text is inside it.
#[test]
fn wrapped_inline_block_opens_a_new_line() {
    let html = format!(
        "<html><head><style>{STYLE} .w {{ width: 3.5in }}</style></head><body>\
         <div><span class=\"b w\"><p>WA1</p><p>WA2</p></span> \
         <span class=\"b w\"><p>WB1</p></span> \
         <span class=\"b w\"><p>WC1</p></span></div></body></html>"
    );
    let l = lay(&html);
    let bg = backgrounds(&l);
    assert_eq!(bg.len(), 3, "{bg:?}");
    let first_bottom = bg[0].1 + bg[0].3;
    let (cx, cy, _, ch) = bg[2];
    assert!((cx - LEFT).abs() < 0.5, "C wraps to the line start, x={cx}");
    assert!(cy >= first_bottom - 0.01, "C top {cy} is below the first line bottom {first_bottom}");
    assert!(cy < first_bottom + 20.0, "C opens the NEXT line, not a later one: {cy}");
    let (_, c1) = text(&l, "WC1");
    assert!(c1 > cy && c1 < cy + ch + 0.01, "WC1 baseline {c1} inside C's box {cy}..{}", cy + ch);
}

/// Defect 3 (block): an inline-block after an in-flow block starts a fresh
/// line at the block's bottom and at the content-box left edge.
#[test]
fn inline_block_after_block_starts_fresh_line() {
    let l = lay(&doc(
        "<div><span class=\"b\"><p>KA1</p></span><p>KBLOCK</p>\
         <span class=\"b\"><p>KB1</p></span></div>",
    ));
    let bg = backgrounds(&l);
    assert_eq!(bg.len(), 2, "{bg:?}");
    let (bx, by, _, bh) = bg[1];
    let (_, block) = text(&l, "KBLOCK");
    assert!((bx - LEFT).abs() < 0.5, "second box starts at the line start, x={bx}");
    assert!(by > block, "second box sits below the block (block baseline {block}, box top {by})");
    let (_, kb1) = text(&l, "KB1");
    assert!(kb1 > by && kb1 < by + bh + 0.01, "KB1 baseline {kb1} inside its box");
}

/// Defect 4: `margin-left` indents the box ONCE.
#[test]
fn inline_block_margin_left_applied_once() {
    let html = "<html><head><style>@page { margin: 0.5in } body { margin: 0 } \
                .b { display: inline-block; width: 1in; margin-left: 40px; background: #ccc }\
                </style></head><body><div><span class=\"b\">x</span></div></body></html>";
    let bg = backgrounds(&lay(html));
    assert_eq!(bg.len(), 1, "{bg:?}");
    assert!((bg[0].0 - (LEFT + 30.0)).abs() < 0.5, "40px = 30pt indent once, got x={}", bg[0].0);
}

/// Guard: a lone inline-block with block children keeps its place.
#[test]
fn single_inline_block_unchanged() {
    let l = lay(&doc("<div><span class=\"b\"><p>S1</p><p>S2</p></span></div>"));
    let bg = backgrounds(&l);
    assert_eq!(bg.len(), 1);
    assert!((bg[0].0 - LEFT).abs() < 0.5 && (bg[0].1 - TOP).abs() < 0.5, "{bg:?}");
    let (_, s2) = text(&l, "S2");
    assert!(s2 > bg[0].1 && s2 < bg[0].1 + bg[0].3 + 0.01);
}
