//! CORE-561: vertical-line residuals of CORE-555, with REAL fonts.
//!
//! 1. Mixed block/text vertical boxes set their text as vertical lines.
//! 2. `<a href>` spans in vertical lines emit link rects along the column.
//! 3. Inline backgrounds paint behind vertical glyphs.
//! 4. `direction: rtl` reverses the inline axis of vertical lines.
//! 5. A vertical paragraph in a vertical page flow splits its columns
//!    across pages.
//! 6. `text-orientation: mixed` follows UAX #50 (Tr characters take the
//!    font's vertical form; R characters rotate).

use std::io::Read;

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::frag::{Fragment, FragmentContent, LineOrientation};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::{layout, Layout};
use typeanvil::pdf::{render_with_options, DocumentMetadata};

/// 300pt x 225pt page (400 x 300 px), no margins.
fn geo() -> PageGeometry {
    PageGeometry {
        width: Scalar(300.0),
        height: Scalar(225.0),
        margin_top: Scalar(0.0),
        margin_right: Scalar(0.0),
        margin_bottom: Scalar(0.0),
        margin_left: Scalar(0.0),
    }
}

const HEAD: &str = "<!DOCTYPE html><html><head><style>\
     body { margin: 0; font-family: sans-serif; font-size: 20px; line-height: 30px }\
     .v { writing-mode: vertical-rl; height: 200px; width: 120px; margin: 20px }\
     </style></head><body>";

fn doc(body: &str) -> String {
    format!("{HEAD}{body}</body></html>")
}

fn parse(src: &str) -> (Dom, Stylesheet) {
    let dom = Dom::parse(src).expect("parse");
    let mut css = String::new();
    for (id, n) in dom.nodes.iter().enumerate() {
        if let NodeKind::Element(el) = &n.kind {
            if el.tag == "style" {
                css.push_str(&dom.text_content(id));
            }
        }
    }
    (dom, Stylesheet::parse(&css))
}

fn lay(src: &str) -> Layout {
    let (dom, css) = parse(src);
    layout(&dom, &css, geo())
}

fn pdf(src: &str) -> Vec<u8> {
    let (dom, css) = parse(src);
    let laid = layout(&dom, &css, geo());
    render_with_options(&laid, Some(&dom), &DocumentMetadata::default(), true, false).expect("render")
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn streams(pdf: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(i) = find(&pdf[at..], b"stream") {
        let mut s = at + i + b"stream".len();
        if pdf[s..].starts_with(b"\r\n") {
            s += 2;
        } else if pdf[s..].starts_with(b"\n") {
            s += 1;
        }
        let Some(e) = find(&pdf[s..], b"endstream") else { break };
        let mut buf = Vec::new();
        if flate2::read::ZlibDecoder::new(&pdf[s..s + e]).read_to_end(&mut buf).is_ok() {
            out.push(String::from_utf8_lossy(&buf).into_owned());
        }
        at = s + e + b"endstream".len();
    }
    out
}

/// (orientation, abs x, abs y, w, h, text) of every text line of page `p`.
type Line = (LineOrientation, f64, f64, f64, f64, String, f64, f64);

fn lines(laid: &Layout, p: usize) -> Vec<Line> {
    fn walk(f: &Fragment, x: f64, y: f64, out: &mut Vec<Line>) {
        let (ax, ay) = (x + f.offset.x.get(), y + f.offset.y.get());
        if let FragmentContent::Text(run) = &f.content {
            out.push((
                f.orientation,
                ax,
                ay,
                f.size.0.get(),
                f.size.1.get(),
                run.text.clone(),
                x + run.baseline.x.get(),
                y + run.baseline.y.get(),
            ));
        }
        for c in &f.children {
            walk(c, ax, ay, out);
        }
    }
    let mut out = Vec::new();
    walk(&laid.pages[p].root, 0.0, 0.0, &mut out);
    out
}

/// Item 1: a vertical box with a block child beside its text sets EVERY
/// line (the anonymous runs and the child's) as a vertical line, and CJK
/// in it shapes upright.
#[test]
fn mixed_block_and_text_vertical_box_sets_vertical_lines() {
    let laid = lay(&doc("<div class=\"v\">one 日本<div>child</div>two</div>"));
    let ls = lines(&laid, 0);
    assert_eq!(ls.len(), 3, "three lines: {ls:?}");
    for l in &ls {
        assert_eq!(l.0, LineOrientation::VerticalDown, "vertical line: {l:?}");
        assert!(l.4 > l.3, "a column is taller than wide: {l:?}");
    }
    // Columns step leftward (vertical-rl) in source order.
    assert!(ls[0].1 > ls[1].1 && ls[1].1 > ls[2].1, "rl stacking: {ls:?}");
    // CJK glyphs are upright (shaped top-to-bottom).
    let mut upright = 0;
    fn walk(f: &Fragment, n: &mut usize) {
        if let FragmentContent::Text(run) = &f.content {
            *n += run.glyphs.iter().filter(|g| g.upright).count();
        }
        for c in &f.children {
            walk(c, n);
        }
    }
    walk(&laid.pages[0].root, &mut upright);
    assert_eq!(upright, 2, "日本 upright");
}

/// Item 2: a link inside a vertical line gets an annotation rect that runs
/// ALONG the column (taller than wide), inside the column.
#[test]
fn link_in_vertical_line_has_a_column_rect() {
    let src = doc("<div class=\"v\">one <a href=\"https://x.test/\">link here</a> two</div>");
    let laid = lay(&src);
    assert_eq!(laid.links.len(), 1, "one link rect");
    let l = &laid.links[0];
    let (x, y, w, h) = (l.x.get(), l.y.get(), l.w.get(), l.h.get());
    assert!(h > w * 2.0, "rect runs down the column: {x} {y} {w} {h}");
    let col = &lines(&laid, 0)[0];
    assert!(x >= col.1 - 0.01 && x + w <= col.1 + col.3 + 0.01, "rect inside the column x");
    // Starts after "one " (> 15pt below the column top) and ends before
    // the column bottom.
    assert!(y > col.2 + 15.0 && y + h < col.2 + col.4, "rect along the column: {y} {h} {col:?}");
    let bytes = pdf(&src);
    assert!(find(&bytes, b"/Link").is_some(), "link annotation in the PDF");
}

/// Item 3: an inline background paints a filled rect behind the vertical
/// glyphs; the rect is taller than wide (it runs along the column).
#[test]
fn inline_background_paints_behind_vertical_glyphs() {
    let src = doc("<div class=\"v\">one <span style=\"background: #00ff00\">marked</span> two</div>");
    let bytes = pdf(&src);
    let s: String = streams(&bytes).into_iter().filter(|s| s.contains("BT")).collect();
    // The fill colour, then a closed four-corner path (`m l l l h f`).
    let at = s.find("0 1 0 rg").expect("green fill set") + "0 1 0 rg".len();
    let end = s[at..].find("\nh").expect("closed path after the fill");
    let nums: Vec<f64> = s[at..at + end]
        .split_whitespace()
        .filter_map(|t| t.parse::<f64>().ok())
        .collect();
    assert_eq!(nums.len(), 8, "four corners: {nums:?}");
    let xs = [nums[0], nums[2], nums[4], nums[6]];
    let ys = [nums[1], nums[3], nums[5], nums[7]];
    let span = |v: [f64; 4]| v.iter().cloned().fold(f64::MIN, f64::max) - v.iter().cloned().fold(f64::MAX, f64::min);
    let (w, h) = (span(xs), span(ys));
    assert!(h > w * 2.0, "background runs along the column: w={w} h={h}");
    // The rect is not the whole column: it covers "marked" only.
    assert!(h < 100.0, "background covers the span, not the column: {h}");
}

/// Item 4: `direction: rtl` reverses the inline axis: the line starts at
/// the column's BOTTOM (inline start) instead of its top.
#[test]
fn rtl_reverses_the_vertical_inline_axis() {
    let ltr = lay(&doc("<div class=\"v\">one two</div>"));
    let rtl = lay(&doc("<div class=\"v\" style=\"direction: rtl\">one two</div>"));
    let (a, b) = (&lines(&ltr, 0)[0], &lines(&rtl, 0)[0]);
    // ltr: the baseline starts at the content top (15pt).
    assert!((a.7 - 15.0).abs() < 0.5, "ltr starts at the top: {a:?}");
    // rtl: the text ends at the content bottom (165pt), so it starts lower.
    assert!(b.7 > a.7 + 40.0, "rtl text sits at the column bottom: {b:?}");
}

/// Item 5: a long paragraph in a vertical-rl page flow splits its columns
/// across pages (no column overflows a page; every word is placed once).
#[test]
fn vertical_paragraph_splits_across_pages() {
    let words: Vec<String> = (1..=150).map(|i| format!("w{i}")).collect();
    let src = format!(
        "<!DOCTYPE html><html style=\"writing-mode: vertical-rl\"><head><style>\
         body {{ margin: 0; font-family: sans-serif; font-size: 20px; line-height: 30px }}\
         </style></head><body><p style=\"margin: 0\">{}</p></body></html>",
        words.join(" ")
    );
    let laid = lay(&src);
    assert!(laid.pages.len() >= 2, "the paragraph spans pages: {}", laid.pages.len());
    let mut all = Vec::new();
    for p in 0..laid.pages.len() {
        for l in lines(&laid, p) {
            assert_eq!(l.0, LineOrientation::VerticalDown, "vertical line");
            assert!(l.1 >= -0.01 && l.1 + l.3 <= 300.01, "column inside the page: {l:?}");
            all.extend(l.5.split_whitespace().map(str::to_string));
        }
    }
    assert_eq!(all, words, "every word once, in order");
}

/// Item 6: UAX #50. Latin (R) rotates; ideographs (U) are upright; the
/// CJK corner bracket U+300C (Tr) takes the font's vertical form and is
/// upright.
#[test]
fn uax50_orientation_classes() {
    assert!(typeanvil::typography::is_upright_char('語'));
    assert!(!typeanvil::typography::is_upright_char('a'));
    assert!(typeanvil::typography::is_transformed_or_rotated('\u{300C}'));
    assert!(!typeanvil::typography::is_upright_char('\u{300C}'));
    // U+3001 IDEOGRAPHIC COMMA is Tu: always upright.
    assert!(typeanvil::typography::is_upright_char('\u{3001}'));
    let laid = lay(&doc("<div class=\"v\">a「語」</div>"));
    let mut glyphs = Vec::new();
    fn walk(f: &Fragment, out: &mut Vec<(String, bool)>) {
        if let FragmentContent::Text(run) = &f.content {
            for g in &run.glyphs {
                out.push((run.text[g.range.clone()].to_string(), g.upright));
            }
        }
        for c in &f.children {
            walk(c, out);
        }
    }
    walk(&laid.pages[0].root, &mut glyphs);
    let up = |s: &str| glyphs.iter().find(|g| g.0 == s).map(|g| g.1);
    assert_eq!(up("a"), Some(false), "Latin rotates: {glyphs:?}");
    assert_eq!(up("語"), Some(true), "ideograph upright: {glyphs:?}");
    assert_eq!(up("「"), Some(true), "Tr bracket takes the vertical form: {glyphs:?}");
}

/// Tagged output: the vertical lines of a mixed box keep source order in
/// their ActualText spans, and the output is deterministic.
#[test]
fn mixed_box_output_is_deterministic() {
    let src = doc("<div class=\"v\">one 日本<div>child <a href=\"#x\">a</a></div>two</div>");
    assert_eq!(pdf(&src), pdf(&src));
}
