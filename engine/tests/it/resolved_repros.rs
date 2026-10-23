//! Regression tests for the former `repros/` fixtures (CORE-559).
//!
//! `repros/` held one HTML fixture per open engine gap. Each gap below is
//! fixed, so its fixture moved here as a layout-seam test. CORE-130,
//! CORE-201 and CORE-210 already had direct tests; they are listed for the
//! record:
//!
//! - CORE-130 page floats: `page_floats::top_float_pins_to_page_top`.
//! - CORE-201 per-side border colors: `core201_per_side_border_colors`.
//! - CORE-210 image in a table cell: `images` (AC 8) and `tables`.
//!
//! All tests use the harness geometry: 5in x 3in page, 0.5in margins, so the
//! page content box is 288pt x 144pt.

use typeanvil::css::{Color, Stylesheet};
use typeanvil::dom::{Dom, NodeId, NodeKind};
use typeanvil::frag::{Fragment, FragmentContent, Fragmentainer, TextRun};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::{layout, Layout};

const EPS: f64 = 0.01;

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

fn doc(css: &str, body: &str) -> (Dom, Layout) {
    let html =
        format!(r#"<!DOCTYPE html><html><head><style>{css}</style></head><body>{body}</body></html>"#);
    let dom = Dom::parse(&html).expect("parse html");
    let mut sheet_css = String::new();
    for (id, node) in dom.nodes.iter().enumerate() {
        if let NodeKind::Element(el) = &node.kind {
            if el.tag == "style" {
                sheet_css.push_str(&dom.text_content(id));
                sheet_css.push('\n');
            }
        }
    }
    let sheet = Stylesheet::parse(&sheet_css);
    let l = layout(&dom, &sheet, geometry());
    (dom, l)
}

fn node_by_id(dom: &Dom, id: &str) -> NodeId {
    dom.nodes
        .iter()
        .position(|n| matches!(&n.kind, NodeKind::Element(el) if el.attr("id") == Some(id)))
        .unwrap_or_else(|| panic!("element #{id} must exist")) as NodeId
}

/// Sizes of every fragment whose `source` is `id` on one page.
fn sizes_of(page: &Fragmentainer, id: NodeId) -> Vec<(f64, f64)> {
    fn walk(f: &Fragment, id: NodeId, out: &mut Vec<(f64, f64)>) {
        if f.source == Some(id) {
            out.push((f.size.0.get(), f.size.1.get()));
        }
        for c in &f.children {
            walk(c, id, out);
        }
    }
    let mut out = Vec::new();
    walk(&page.root, id, &mut out);
    out
}

fn text_runs(f: &Fragment) -> Vec<&TextRun> {
    fn walk<'a>(f: &'a Fragment, out: &mut Vec<&'a TextRun>) {
        if let FragmentContent::Text(r) = &f.content {
            out.push(r);
        }
        for c in &f.children {
            walk(c, out);
        }
    }
    let mut out = Vec::new();
    walk(f, &mut out);
    out
}

fn borders(f: &Fragment) -> Vec<&typeanvil::frag::BorderBox> {
    fn walk<'a>(f: &'a Fragment, out: &mut Vec<&'a typeanvil::frag::BorderBox>) {
        if let FragmentContent::Border(b) = &f.content {
            out.push(b);
        }
        for c in &f.children {
            walk(c, out);
        }
    }
    let mut out = Vec::new();
    walk(f, &mut out);
    out
}

// --- CORE-140: viewport units resolve against the page box -----------------

/// paged-media-css spec Behavior 3a: `vh`/`vw` resolve against the page
/// content box (100vh = content height). Stylo's fixed 1024x768 viewport
/// would make `10vh` 57.6pt instead of 14.4pt.
#[test]
fn core140_vh_vw_band_resolves_against_page_content_box() {
    let (dom, l) = doc(
        "body { margin: 0; } #band { width: 100vw; height: 10vh; background-color: #1d3557; }",
        r#"<div id="band"></div><p>After the band.</p>"#,
    );
    let band = node_by_id(&dom, "band");
    let sizes = sizes_of(&l.pages[0], band);
    assert!(!sizes.is_empty(), "the band lays out on page 1");
    let (w, h) = sizes[0];
    assert!((w - 288.0).abs() < EPS, "100vw = content width 288pt, got {w}");
    assert!((h - 14.4).abs() < EPS, "10vh = 10% of content height 144pt, got {h}");
}

// --- CORE-141: margin boxes honor their own declarations -------------------

/// A margin box's own `font-family`, `font-size`, `color` and
/// `border-bottom` reach its fragment. Before CORE-141 every margin box drew
/// in a fixed 10pt sans face with no border.
#[test]
fn core141_margin_box_uses_its_own_font_color_and_border() {
    let css = r#"
        @page {
          @top-center {
            content: "Per-box styled head";
            font-family: "Courier New", monospace;
            font-size: 12pt;
            color: #1d3557;
            border-bottom: 0.5pt solid #457b9d;
            vertical-align: middle;
          }
        }
        body { font-family: Arial, sans-serif; font-size: 10pt; }
    "#;
    let (_, l) = doc(css, "<p>Body.</p>");
    let page = &l.pages[0];
    let head = page
        .margin_boxes
        .iter()
        .find(|mb| !text_runs(&mb.fragment).is_empty())
        .expect("the top-center margin box carries text");

    let runs = text_runs(&head.fragment);
    let run = runs[0];
    assert!((run.font_size.get() - 12.0).abs() < EPS, "font-size 12pt, got {}", run.font_size.get());
    assert_eq!(run.color, Color { r: 0x1d, g: 0x35, b: 0x57, a: 255 }, "own color");
    let ps = typeanvil::fonts::face_postscript_name(run.font_face);
    assert!(
        ps == "CourierNewPSMT" || ps == "LiberationMono",
        "monospace face expected, got {ps}"
    );

    let b = borders(&head.fragment);
    let b = b.first().expect("the margin box carries its border");
    assert!((b.bottom.get() - 0.5).abs() < EPS, "bottom border 0.5pt, got {}", b.bottom.get());
    assert_eq!(b.top.get(), 0.0, "only the bottom side is declared");
    assert_eq!(b.bottom_color, Color { r: 0x45, g: 0x7b, b: 0x9d, a: 255 });
}

// --- CORE-143: an oversized abspos box is not re-painted per page ----------

/// An absolutely positioned box taller than the page paints its content
/// once. The gap re-painted (or sliced) the box on every continuation page,
/// so its marker text repeated.
#[test]
fn core143_oversized_abspos_content_paints_once() {
    let css = r#"
        body { margin: 0; font-size: 10pt; }
        #big { position: absolute; top: 0; left: 0; width: 200pt; height: 400pt;
               background-color: #e9f0f5; }
    "#;
    let body = r#"<p>In-flow paragraph one.</p>
        <div id="big">OVERSIZED-MARKER</div>
        <p>In-flow paragraph two.</p>"#;
    let (_, l) = doc(css, body);
    assert!(l.pages.len() > 1, "a 400pt box in a 144pt content box must span pages, got {}", l.pages.len());
    let markers: usize = l
        .pages
        .iter()
        .map(|p| {
            text_runs(&p.root)
                .iter()
                .filter(|r| r.text.contains("OVERSIZED-MARKER"))
                .count()
        })
        .sum();
    assert_eq!(markers, 1, "the oversized box's text paints exactly once across {} pages", l.pages.len());
}
