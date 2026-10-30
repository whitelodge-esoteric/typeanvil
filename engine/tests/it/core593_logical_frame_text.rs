//! CORE-593: text-only blocks inside a logical frame (a vertical root, or
//! the interior frame of a vertical wrapper in a horizontal page) must paint
//! rotated. CORE-555's vertical line path ran inside those frames too; the
//! frame then rotated the columns a second time, so the text came out
//! upright (horizontal). These tests read every text object's `cm` matrix
//! in the PDF: a Latin run in a `vertical-rl` flow paints rotated.

use std::io::Read;

use typeanvil::css::Stylesheet;
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::layout;
use typeanvil::pdf::{render_with_options, DocumentMetadata};

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
    render_with_options(&laid, Some(&dom), &DocumentMetadata::default(), false, false).expect("render")
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

/// The last `cm` matrix before each `BT`, as rounded `(a, b, c, d)`.
fn text_matrices(pdf: &[u8]) -> Vec<(i32, i32, i32, i32)> {
    let mut out = Vec::new();
    for s in streams(pdf) {
        if !s.contains(" Tf") || !s.contains("BT") {
            continue;
        }
        let mut last = None;
        for line in s.lines() {
            if let Some(idx) = line.find(" cm") {
                let nums: Vec<f64> =
                    line[..idx].split_whitespace().filter_map(|t| t.parse::<f64>().ok()).collect();
                if nums.len() >= 6 {
                    let n = &nums[nums.len() - 6..];
                    let r = |v: f64| v.round() as i32;
                    last = Some((r(n[0]), r(n[1]), r(n[2]), r(n[3])));
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

/// True when a text object paints with a quarter-turn (b and c non-zero).
fn rotated(m: &(i32, i32, i32, i32)) -> bool {
    m.0 == 0 && m.3 == 0 && m.1 != 0 && m.2 != 0
}

fn assert_all_rotated(src: &str, what: &str) {
    let ms = text_matrices(&pdf(src));
    assert!(!ms.is_empty(), "{what}: no text objects found");
    assert!(ms.iter().all(rotated), "{what}: every run paints rotated, got {ms:?}");
}

/// page-name-orthogonal-writing-003 shape: a vertical-rl wrapper in a
/// horizontal page holds text-only blocks.
#[test]
fn interior_vertical_wrapper_text_paints_rotated() {
    let src = "<!DOCTYPE html><html style=\"writing-mode: horizontal-tb\"><body>\
               <div style=\"writing-mode: vertical-rl\">\
               <div style=\"page: a\">a</div><div style=\"page: b\">b</div>\
               </div></body></html>";
    assert_all_rotated(src, "interior vertical wrapper");
}

/// page-size-012 shape: a vertical-rl root with text-only blocks.
#[test]
fn vertical_root_text_only_blocks_paint_rotated() {
    let src = "<!DOCTYPE html><html><head><style>\
               :root { writing-mode: vertical-rl } body { margin: 0 }</style></head><body>\
               <div style=\"background: yellow\">first line<br>second line<br></div>\
               <div style=\"break-before: page; background: yellow\">third line<br></div>\
               </body></html>";
    assert_all_rotated(src, "vertical root");
}
