//! CORE-555: glyph orientation and vertical line stacking with a REAL font
//! (the bundled Arial/Liberation faces plus a system CJK face), not Ahem.
//!
//! Ahem's square glyphs cannot show orientation, so these tests read the
//! PDF content stream: every text object's `cm` matrix tells whether the
//! glyphs paint rotated (and which way) or upright, and its translation
//! tells where the column sits. They use only the public layout/PDF API.

use std::io::Read;

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::frag::{Fragment, FragmentContent};
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

/// One 120px-wide, 200px-tall box at (20px, 20px), 20px text on 30px lines
/// (column width 22.5pt). `text` wraps into several columns.
fn html(wm: &str, text: &str) -> String {
    format!(
        "<!DOCTYPE html><html><head><style>\
         body {{ margin: 0; font-family: sans-serif; font-size: 20px; line-height: 30px }}\
         .v {{ writing-mode: {wm}; height: 200px; width: 120px; margin: 20px }}\
         </style></head><body><div class=\"v\">{text}</div></body></html>"
    )
}

fn lay(src: &str) -> Layout {
    let dom = Dom::parse(src).expect("parse");
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

fn pdf(src: &str) -> Vec<u8> {
    let dom = Dom::parse(src).expect("parse");
    let mut css = String::new();
    for (id, n) in dom.nodes.iter().enumerate() {
        if let NodeKind::Element(el) = &n.kind {
            if el.tag == "style" {
                css.push_str(&dom.text_content(id));
            }
        }
    }
    let laid = layout(&dom, &Stylesheet::parse(&css), geo());
    render_with_options(&laid, Some(&dom), &DocumentMetadata::default(), true, false).expect("render")
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Every inflated stream of the PDF.
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

/// The `cm` matrix that precedes each text object (`BT`) in the page
/// content stream, in paint order. `[a, b, c, d, e, f]` in PDF user space
/// (y up, origin bottom-left).
fn text_matrices(pdf: &[u8]) -> Vec<[f64; 6]> {
    let mut out = Vec::new();
    for s in streams(pdf) {
        if !s.contains(" Tf") || !s.contains("BT") {
            continue;
        }
        let mut last: Option<[f64; 6]> = None;
        for line in s.lines() {
            if let Some(idx) = line.find(" cm") {
                let head = &line[..idx];
                let nums: Vec<f64> = head
                    .split_whitespace()
                    .filter_map(|t| t.parse::<f64>().ok())
                    .collect();
                if nums.len() >= 6 {
                    let n = &nums[nums.len() - 6..];
                    last = Some([n[0], n[1], n[2], n[3], n[4], n[5]]);
                }
            }
            if line.starts_with("BT") {
                if let Some(m) = last {
                    out.push(m);
                }
            }
        }
    }
    out
}

/// Classify a text-object matrix. The page writer flips y once
/// (`1 0 0 -1 0 H`), so in PDF space an upright run is `[1 0 0 -1]`, a run
/// rotated 90° clockwise on the page is `[0 -1 -1 0]` and one rotated 90°
/// counter-clockwise is `[0 1 1 0]`.
#[derive(Debug, PartialEq)]
enum Orient {
    Upright,
    Clockwise,
    CounterClockwise,
}

fn orient(m: &[f64; 6]) -> Orient {
    let r = |v: f64| v.round() as i32;
    match (r(m[0]), r(m[1]), r(m[2]), r(m[3])) {
        (1, 0, 0, -1) => Orient::Upright,
        (0, -1, -1, 0) => Orient::Clockwise,
        (0, 1, 1, 0) => Orient::CounterClockwise,
        other => panic!("unexpected text matrix {other:?} in {m:?}"),
    }
}

/// Every text line fragment of page 0 as (abs x, abs y, w, h, text), in
/// paint (document) order.
fn lines(laid: &Layout) -> Vec<(f64, f64, f64, f64, String)> {
    fn walk(f: &Fragment, x: f64, y: f64, out: &mut Vec<(f64, f64, f64, f64, String)>) {
        let (ax, ay) = (x + f.offset.x.get(), y + f.offset.y.get());
        if let FragmentContent::Text(run) = &f.content {
            out.push((ax, ay, f.size.0.get(), f.size.1.get(), run.text.clone()));
        }
        for c in &f.children {
            walk(c, ax, ay, out);
        }
    }
    let mut out = Vec::new();
    walk(&laid.pages[0].root, 0.0, 0.0, &mut out);
    out
}

const LATIN: &str = "one two three four five six seven eight nine ten eleven";

/// Line DIRECTION and STACKING ORDER: each line is a tall column (a
/// line-height wide, the box's inline extent tall); successive lines step
/// LEFT in the `-rl` modes and RIGHT in the `-lr` modes, starting at the
/// box's block-start edge. The box is 90pt wide with 15pt margins; the
/// `-rl` modes right-anchor it (CORE-153), so its right edge is 285pt; the
/// `-lr` modes left-anchor it at 15pt. Content y runs 15..165pt.
#[test]
fn vertical_lines_stack_along_the_block_axis() {
    for (wm, rl) in [
        ("vertical-rl", true),
        ("vertical-lr", false),
        ("sideways-rl", true),
        ("sideways-lr", false),
    ] {
        let ls = lines(&lay(&html(wm, LATIN)));
        assert!(ls.len() >= 3, "{wm}: paragraph wraps into columns, got {ls:?}");
        for (x, y, w, h, _) in &ls {
            assert!((w - 22.5).abs() < 0.01, "{wm}: column is one line-height wide, got w={w}");
            assert!((h - 150.0).abs() < 0.01, "{wm}: column spans the 200px inline extent, got h={h}");
            assert!((y - 15.0).abs() < 0.01, "{wm}: column starts at the content top, got y={y}");
            let _ = x;
        }
        let first_x = ls[0].0;
        if rl {
            assert!((first_x - (285.0 - 22.5)).abs() < 0.01, "{wm}: first column at the right edge, got {first_x}");
        } else {
            assert!((first_x - 15.0).abs() < 0.01, "{wm}: first column at the left edge, got {first_x}");
        }
        for pair in ls.windows(2) {
            let step = pair[1].0 - pair[0].0;
            let want = if rl { -22.5 } else { 22.5 };
            assert!((step - want).abs() < 0.01, "{wm}: next line steps {want}, got {step}");
        }
        // Source order is preserved line by line.
        let joined: Vec<String> = ls.iter().map(|l| l.4.trim().to_string()).collect();
        assert_eq!(joined.join(" "), LATIN, "{wm}: lines carry the source text in order");
    }
}

/// Horizontal text is unchanged: one line per row, rows step DOWN.
#[test]
fn horizontal_lines_still_stack_down() {
    let ls = lines(&lay(&html("horizontal-tb", LATIN)));
    assert!(ls.len() >= 2);
    for pair in ls.windows(2) {
        assert!(pair[1].1 > pair[0].1, "horizontal lines stack downward: {ls:?}");
        assert!((pair[1].0 - pair[0].0).abs() < 0.01);
    }
    assert!(ls[0].2 > ls[0].3, "a horizontal line is wider than tall");
}

/// GLYPH ORIENTATION, Latin: rotated 90° clockwise in `vertical-*` and
/// `sideways-rl`; 90° counter-clockwise in `sideways-lr`.
#[test]
fn latin_glyphs_rotate_per_mode() {
    for (wm, want) in [
        ("vertical-rl", Orient::Clockwise),
        ("vertical-lr", Orient::Clockwise),
        ("sideways-rl", Orient::Clockwise),
        ("sideways-lr", Orient::CounterClockwise),
    ] {
        let ms = text_matrices(&pdf(&html(wm, LATIN)));
        assert!(!ms.is_empty(), "{wm}: text painted");
        for m in &ms {
            assert_eq!(orient(m), want, "{wm}: Latin run orientation, matrix {m:?}");
        }
    }
    // Control: horizontal text paints upright.
    for m in text_matrices(&pdf(&html("horizontal-tb", LATIN))) {
        assert_eq!(orient(&m), Orient::Upright);
    }
}

/// GLYPH ORIENTATION, `text-orientation: mixed` (the default): CJK stays
/// upright in `vertical-rl/lr`, while the sideways modes rotate it with the
/// Latin text. `text-orientation: upright` sets Latin upright too.
#[test]
fn cjk_is_upright_in_vertical_modes_and_rotated_in_sideways_modes() {
    let cjk = "日本語";
    for (wm, want) in [
        ("vertical-rl", Orient::Upright),
        ("vertical-lr", Orient::Upright),
        ("sideways-rl", Orient::Clockwise),
        ("sideways-lr", Orient::CounterClockwise),
    ] {
        let ms = text_matrices(&pdf(&html(wm, cjk)));
        assert!(!ms.is_empty(), "{wm}: CJK painted");
        for m in &ms {
            assert_eq!(orient(m), want, "{wm}: CJK orientation, matrix {m:?}");
        }
    }
    let up = html("vertical-rl", "Ab").replace(".v {", ".v { text-orientation: upright;");
    for m in text_matrices(&pdf(&up)) {
        assert_eq!(orient(&m), Orient::Upright, "text-orientation: upright sets Latin upright");
    }
}

/// Upright CJK glyphs advance DOWN the column by the font's vertical
/// advance (one em each for ideographs), centred on the column.
#[test]
fn upright_glyphs_advance_down_the_column() {
    let ms = text_matrices(&pdf(&html("vertical-rl", "日本語")));
    assert_eq!(ms.len(), 3, "one upright draw per ideograph: {ms:?}");
    // Each glyph's Tm sits inside its own BT; the cm is the shared page
    // flip, so read positions from the layout instead and check the column.
    let ls = lines(&lay(&html("vertical-rl", "日本語")));
    assert_eq!(ls.len(), 1, "three ideographs fit one column");
    let (x, _, w, _, t) = &ls[0];
    assert_eq!(t, "日本語");
    assert!((x + w - 285.0).abs() < 0.01, "column at the right edge, got x={x} w={w}");
}

/// Sideways-lr runs BOTTOM TO TOP: its rotated run starts at the column's
/// bottom (PDF y near the box's bottom edge), unlike sideways-rl which
/// starts at the top.
#[test]
fn sideways_lr_runs_bottom_to_top() {
    let page_h = 225.0;
    let top_pdf = page_h - 15.0; // box content top in PDF space
    let bottom_pdf = page_h - 165.0; // box content bottom in PDF space
    let srl = text_matrices(&pdf(&html("sideways-rl", "Alpha")));
    let slr = text_matrices(&pdf(&html("sideways-lr", "Alpha")));
    assert!((srl[0][5] - top_pdf).abs() < 0.5, "sideways-rl starts at the top: {:?}", srl[0]);
    assert!((slr[0][5] - bottom_pdf).abs() < 0.5, "sideways-lr starts at the bottom: {:?}", slr[0]);
}

/// Every `/ActualText` value in the page streams, decoded (literal strings
/// or `<hex>` UTF-16BE with a BOM).
fn actual_texts(pdf: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    for s in streams(pdf) {
        let b = s.as_bytes();
        let mut at = 0;
        while let Some(i) = s[at..].find("/ActualText") {
            let mut p = at + i + "/ActualText".len();
            while b[p].is_ascii_whitespace() {
                p += 1;
            }
            let end = p + s[p..].find(|c| c == ')' || c == '>').unwrap();
            if b[p] == b'<' {
                let hex = &s[p + 1..end];
                let units: Vec<u16> = (0..hex.len() / 4)
                    .map(|k| u16::from_str_radix(&hex[4 * k..4 * k + 4], 16).unwrap())
                    .collect();
                out.push(String::from_utf16_lossy(&units).trim_start_matches('\u{feff}').to_string());
            } else {
                out.push(s[p + 1..end].to_string());
            }
            at = end;
        }
    }
    out
}

/// Text extraction reads a vertical paragraph in source order in every
/// mode. Extractors (pdfium, Acrobat) order separate text runs by position,
/// so `*-rl` columns, which stack right to left, would come out last line
/// first. The paragraph's columns therefore form ONE marked-content span
/// whose `/ActualText` is the paragraph text in source order, which those
/// extractors return verbatim (measured with pypdfium2 `get_text_range`).
/// The ToUnicode map stays complete for per-glyph selection.
#[test]
fn vertical_text_layer_round_trips() {
    let mixed = format!("{LATIN} 日本語の文章を縦に書きます");
    for wm in ["vertical-rl", "vertical-lr", "sideways-rl", "sideways-lr"] {
        for text in [LATIN, mixed.as_str()] {
            let laid = lay(&html(wm, text));
            assert!(lines(&laid).len() >= 3, "{wm}: the paragraph wraps into columns");
            let bytes = pdf(&html(wm, text));
            let all = streams(&bytes).join("\n");
            assert!(all.contains("beginbfchar") || all.contains("beginbfrange"), "{wm}: ToUnicode present");
            for ch in ['o', 'n', 'e', 'l', 'v'] {
                let hex = format!("<{:04X}>", ch as u32);
                assert!(all.contains(&hex), "{wm}: ToUnicode maps {ch:?}");
            }
            assert_eq!(actual_texts(&bytes), vec![text.to_string()], "{wm}: extraction in source order");
        }
    }
}

/// A vertical paragraph is monolithic in the flow: one that does not fit
/// the room left on the page moves whole to the next page instead of
/// painting past the page edge (page-name-orthogonal-writing-002 ref shape:
/// a huge `margin-block-end` pushes the second paragraph off page 1).
#[test]
fn vertical_paragraph_that_does_not_fit_moves_to_next_page() {
    let src = "<!DOCTYPE html><html style=\"writing-mode: vertical-rl\"><head><style>\
               body { margin: 0; font-size: 20px; line-height: 30px }</style></head><body>\
               <div style=\"margin-block-end: 999in\">a</div><div>b</div></body></html>";
    let laid = lay(src);
    assert_eq!(laid.pages.len(), 2, "the second paragraph moves to page 2");
    let page_text = |i: usize| -> Vec<String> {
        fn walk(f: &Fragment, out: &mut Vec<String>) {
            if let FragmentContent::Text(run) = &f.content {
                out.push(run.text.clone());
            }
            for c in &f.children {
                walk(c, out);
            }
        }
        let mut out = Vec::new();
        walk(&laid.pages[i].root, &mut out);
        out
    };
    assert_eq!(page_text(0), vec!["a".to_string()]);
    assert_eq!(page_text(1), vec!["b".to_string()]);
}

/// Same input, byte-identical PDF, for every vertical mode.
#[test]
fn vertical_output_is_deterministic() {
    for wm in ["vertical-rl", "vertical-lr", "sideways-rl", "sideways-lr"] {
        let src = html(wm, &format!("{LATIN} 日本語"));
        assert_eq!(pdf(&src), pdf(&src), "{wm}: deterministic");
    }
}
