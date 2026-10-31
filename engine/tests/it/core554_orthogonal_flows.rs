//! CORE-554: nested orthogonal flows (writing-modes-layout spec Behavior 20,
//! 36, 37). Geometry measured against Chromium (probes in the CORE-554
//! evidence folder): 400pt x 300pt page, no margins, 12pt/16pt sans-serif.
//!
//! - vertical-in-horizontal: inline size (physical height) fills the
//!   parent's DEFINITE height, else shrink-to-fit capped at the page.
//! - horizontal-in-vertical: inline size (physical width) is the declared
//!   width, else the vertical parent's definite width, else shrink-to-fit
//!   capped at the ICB width. Its text stays horizontal.
//! - An orthogonal box is monolithic: it moves whole to the next page, and
//!   breaks inside it (forced or page-change) do not fire.

use typeanvil::css::{Color, Stylesheet};
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::frag::{Fragment, FragmentContent, LineOrientation};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::{layout, Layout};

fn geo() -> PageGeometry {
    PageGeometry {
        width: Scalar(400.0),
        height: Scalar(300.0),
        margin_top: Scalar(0.0),
        margin_right: Scalar(0.0),
        margin_bottom: Scalar(0.0),
        margin_left: Scalar(0.0),
    }
}

fn lay(root_style: &str, body: &str) -> Layout {
    let src = format!(
        "<!DOCTYPE html><html style=\"{root_style}\"><head><style>\
         body {{ margin: 0; font-family: sans-serif; font-size: 12pt; line-height: 16pt }}\
         </style></head><body>{body}</body></html>"
    );
    let dom = Dom::parse(&src).expect("parse");
    let mut css = String::new();
    for (id, n) in dom.nodes.iter().enumerate() {
        if let NodeKind::Element(el) = &n.kind {
            if el.tag == "style" {
                css.push_str(&dom.text_content(id));
            }
        }
    }
    layout(&dom, &Stylesheet::parse(&css), geo())
}

/// Absolute rect (x, y, w, h) of every background fill of colour `c` on
/// page `pi`.
fn rects(l: &Layout, pi: usize, c: (u8, u8, u8)) -> Vec<(f64, f64, f64, f64)> {
    fn walk(f: &Fragment, x: f64, y: f64, c: (u8, u8, u8), out: &mut Vec<(f64, f64, f64, f64)>) {
        let (ax, ay) = (x + f.offset.x.get(), y + f.offset.y.get());
        if let FragmentContent::Background(Color { r, g, b, .. }) = f.content {
            if (r, g, b) == c {
                out.push((ax, ay, f.size.0.get(), f.size.1.get()));
            }
        }
        for ch in &f.children {
            walk(ch, ax, ay, c, out);
        }
    }
    let mut out = Vec::new();
    walk(&l.pages[pi].root, 0.0, 0.0, c, &mut out);
    out
}

fn one(l: &Layout, pi: usize, c: (u8, u8, u8)) -> (f64, f64, f64, f64) {
    let r = rects(l, pi, c);
    assert_eq!(r.len(), 1, "one {c:?} box on page {}: {r:?}", pi + 1);
    r[0]
}

/// Text lines per page: (text, orientation, has transform).
fn lines(l: &Layout, pi: usize) -> Vec<(String, LineOrientation, bool)> {
    fn walk(f: &Fragment, out: &mut Vec<(String, LineOrientation, bool)>) {
        if let FragmentContent::Text(run) = &f.content {
            out.push((run.text.clone(), f.orientation, f.transform.is_some()));
        }
        for ch in &f.children {
            walk(ch, out);
        }
    }
    let mut out = Vec::new();
    walk(&l.pages[pi].root, &mut out);
    out
}

const CYAN: (u8, u8, u8) = (0, 255, 255);
const LIME: (u8, u8, u8) = (0, 255, 0);
const MAGENTA: (u8, u8, u8) = (255, 0, 255);
const YELLOW: (u8, u8, u8) = (255, 255, 0);

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.5
}

/// Vertical box in a horizontal flow, parent with a definite height: the
/// box's inline size (physical height) fills that height (Chromium: 100pt),
/// and its columns wrap inside it.
#[test]
fn vertical_in_horizontal_fills_definite_parent_height() {
    let l = lay(
        "",
        "<div style=\"height:100pt;background:yellow\">\
         <div style=\"writing-mode:vertical-rl;background:cyan\">alpha beta gamma delta epsilon zeta eta theta</div>\
         </div>",
    );
    let (x, y, w, h) = one(&l, 0, CYAN);
    assert!(close(h, 100.0), "inline size fills the 100pt parent: h={h}");
    assert!(close(x, 0.0) && close(y, 0.0), "box at the parent origin: ({x},{y})");
    assert!(close(w, 48.0), "three 16pt columns: w={w}");
    let ls = lines(&l, 0);
    assert_eq!(ls.len(), 3, "three vertical lines: {ls:?}");
    assert!(ls.iter().all(|(_, o, _)| *o == LineOrientation::VerticalDown));
}

/// Vertical box in a horizontal flow, indefinite parent height: the inline
/// size is shrink-to-fit (one column, as tall as its text), not the page.
#[test]
fn vertical_in_horizontal_indefinite_is_shrink_to_fit() {
    let l = lay("", "<div style=\"writing-mode:vertical-rl;background:cyan\">one two three</div>");
    let (_, _, w, h) = one(&l, 0, CYAN);
    assert!(close(w, 16.0), "one column: w={w}");
    assert!(h > 40.0 && h < 100.0, "inline size is the text length, not the page: h={h}");
}

/// Horizontal box in a vertical root, indefinite: its width is its
/// max-content (shrink-to-fit), it stacks right-to-left with its siblings,
/// and its text stays horizontal (Chromium: x 252..384, one 16pt line).
#[test]
fn horizontal_in_vertical_root_indefinite_is_shrink_to_fit_and_horizontal() {
    let l = lay(
        "writing-mode:vertical-rl",
        "<div style=\"background:yellow\">alpha</div>\
         <div style=\"writing-mode:horizontal-tb;background:lime\">inner horizontal text here</div>\
         <div style=\"background:cyan\">omega</div>",
    );
    assert_eq!(l.pages.len(), 1);
    let (yx, _, yw, _) = one(&l, 0, YELLOW);
    let (lx, ly, lw, lh) = one(&l, 0, LIME);
    let (cx, _, cw, _) = one(&l, 0, CYAN);
    assert!(close(yx, 384.0) && close(yw, 16.0), "first column at the right: {yx} {yw}");
    assert!(close(lh, 16.0), "one horizontal line tall: lh={lh}");
    assert!(lw > 120.0 && lw < 140.0, "max-content width: lw={lw}");
    assert!(close(lx + lw, 384.0) && close(ly, 0.0), "stacks left of the first column: {lx} {ly}");
    assert!(close(cx + cw, lx), "next column stacks left of it: {cx} {cw}");
    let ls = lines(&l, 0);
    let inner = ls.iter().find(|(t, ..)| t.contains("inner")).expect("inner line");
    assert_eq!(inner.1, LineOrientation::Horizontal, "{inner:?}");
    assert!(!inner.2, "no rotation on the horizontal line: {inner:?}");
}

/// Horizontal box in a vertical root with a declared width wraps at it.
#[test]
fn horizontal_in_vertical_root_declared_width_wraps() {
    let l = lay(
        "writing-mode:vertical-rl",
        "<div style=\"writing-mode:horizontal-tb;width:100pt;background:magenta\">definite width box with wrapping words</div>",
    );
    let (x, y, w, h) = one(&l, 0, MAGENTA);
    assert!(close(w, 100.0), "declared width: {w}");
    assert!(close(h, 48.0), "three lines: {h}");
    assert!(close(x, 300.0) && close(y, 0.0), "block-start edge is the right: ({x},{y})");
}

/// Horizontal box whose vertical parent has a definite width fills it.
#[test]
fn horizontal_in_vertical_fills_definite_parent_width() {
    let l = lay(
        "writing-mode:vertical-rl",
        "<div style=\"width:120pt;background:yellow\">\
         <div style=\"writing-mode:horizontal-tb;background:lime\">alpha beta gamma delta epsilon zeta eta theta</div>\
         </div>",
    );
    let (x, y, w, h) = one(&l, 0, LIME);
    assert!(close(w, 120.0), "fills the parent's 120pt block size: {w}");
    assert!(close(h, 48.0), "three lines: {h}");
    assert!(close(x, 280.0) && close(y, 0.0), "at the parent's block-start: ({x},{y})");
}

/// Horizontal box nested in an INTERIOR vertical box in a horizontal page:
/// laid out physically inside the vertical frame (Chromium: x 16..149).
#[test]
fn horizontal_in_interior_vertical() {
    let l = lay(
        "",
        "<div style=\"writing-mode:vertical-rl;background:cyan;height:200pt\">\
         <div style=\"background:yellow\">alpha</div>\
         <div style=\"writing-mode:horizontal-tb;background:lime\">inner horizontal text here</div>\
         <div style=\"background:magenta\">omega</div></div>",
    );
    assert_eq!(l.pages.len(), 1);
    let (yx, _, _, yh) = one(&l, 0, YELLOW);
    let (lx, ly, lw, lh) = one(&l, 0, LIME);
    let (mx, _, _, _) = one(&l, 0, MAGENTA);
    assert!(close(yh, 200.0), "vertical child fills the 200pt inline size: {yh}");
    assert!(close(lx + lw, yx) && close(ly, 0.0) && close(lh, 16.0), "lime left of yellow: {lx} {lw} {yx}");
    assert!(close(mx + 16.0, lx), "magenta left of lime: {mx} {lx}");
    let ls = lines(&l, 0);
    let inner = ls.iter().find(|(t, ..)| t.contains("inner")).expect("inner line");
    assert_eq!(inner.1, LineOrientation::Horizontal, "{inner:?}");
    assert!(!inner.2, "{inner:?}");
}

/// Continuation across a mode change: an orthogonal box that does not fit
/// the room left moves whole to the next page; every word is placed once,
/// and the flow after it continues (Chromium: 3 pages).
#[test]
fn orthogonal_box_moves_whole_to_next_page() {
    let long = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon phi chi psi omega";
    let l = lay(
        "writing-mode:vertical-rl",
        &format!(
            "<div style=\"width:120pt;background:yellow\">\
             <div style=\"writing-mode:horizontal-tb;background:lime\">alpha beta gamma delta epsilon zeta eta theta</div></div>\
             <div style=\"writing-mode:horizontal-tb;background:magenta\">{long}</div>\
             <div style=\"background:cyan\">end</div>"
        ),
    );
    assert_eq!(l.pages.len(), 3, "box deferred whole, then the trailing column");
    assert!(rects(&l, 0, MAGENTA).is_empty(), "nothing of the box on page 1");
    let (mx, my, mw, mh) = one(&l, 1, MAGENTA);
    assert!(close(mw, 400.0) && close(mh, 32.0) && close(mx, 0.0) && close(my, 0.0), "{mx} {my} {mw} {mh}");
    let words: Vec<String> = (0..3)
        .flat_map(|p| lines(&l, p))
        .flat_map(|(t, ..)| t.split_whitespace().map(str::to_string).collect::<Vec<_>>())
        .collect();
    assert_eq!(words.iter().filter(|w| *w == "omicron").count(), 1, "{words:?}");
    assert_eq!(words.iter().filter(|w| *w == "end").count(), 1);
    assert!(!rects(&l, 2, CYAN).is_empty(), "the flow continues on page 3");
}

/// A forced break inside an orthogonal box does not fire: the box is
/// monolithic in the page flow (Behavior 37).
#[test]
fn forced_break_inside_orthogonal_box_is_ignored() {
    let l = lay(
        "writing-mode:vertical-rl",
        "<div style=\"writing-mode:horizontal-tb\">\
         <div style=\"break-after:page\">a</div><div>b</div></div>",
    );
    assert_eq!(l.pages.len(), 1, "no page break inside the orthogonal box");
    let ls = lines(&l, 0);
    assert_eq!(ls.len(), 2, "{ls:?}");
}

/// page-name-orthogonal-writing-001/003 shapes stay one page.
#[test]
fn page_change_inside_orthogonal_box_stays_one_page() {
    let a = lay(
        "writing-mode:vertical-rl",
        "<div style=\"writing-mode:horizontal-tb\"><div style=\"page:a\">a</div><div style=\"page:b\">b</div></div>",
    );
    assert_eq!(a.pages.len(), 1, "001 shape");
    let b = lay(
        "",
        "<div style=\"writing-mode:vertical-rl\"><div style=\"page:a\">a</div><div style=\"page:b\">b</div></div>",
    );
    assert_eq!(b.pages.len(), 1, "003 shape");
}
