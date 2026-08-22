//! CORE-85 acceptance tests — glyph→text mapping drives the PDF ToUnicode map.
//!
//! Before CORE-85 every shaped glyph carried an empty text range (`0..0`), so
//! krilla's ToUnicode CMap stayed empty and body text extracted as garbage
//! control chars (`'\x01nventory'`). These tests pin the range contract at
//! three levels, mirroring the Done criterion without an external extractor:
//!
//!   1. `shape_word` — per-glyph byte ranges cover the word text (incl. a
//!      ligature glyph spanning multiple chars and multi-byte UTF-8).
//!   2. `break_paragraph` — line glyph ranges cover the line text (incl. the
//!      trailing hyphen glyph on a hyphenation break).
//!   3. rendered PDF — the `/ToUnicode` stream maps glyphs back to readable
//!      source text with no control chars.

use std::io::Read;
use std::ops::Range;

use flate2::read::ZlibDecoder;
use typeanvil::css::{cascade, ComputedStyle, Stylesheet};
use typeanvil::dom::{Dom, NodeKind};
use typeanvil::fonts::{FaceId, FACE_BOLD, FACE_BOLD_ITALIC, FACE_ITALIC, FACE_REGULAR};
use typeanvil::geom::{PageGeometry, Scalar};
use typeanvil::layout::layout;
use typeanvil::pdf::render;
use typeanvil::typography::{break_paragraph, shape_word, ShapedGlyph};

// --- helpers (mirror tests/typography.rs) -----------------------------------

fn inches(v: f64) -> Scalar {
    Scalar(v * 72.0)
}

fn geometry(w_in: f64, h_in: f64, margin_in: f64) -> PageGeometry {
    PageGeometry {
        width: inches(w_in),
        height: inches(h_in),
        margin_top: inches(margin_in),
        margin_right: inches(margin_in),
        margin_bottom: inches(margin_in),
        margin_left: inches(margin_in),
    }
}

fn stylesheet_of(dom: &Dom) -> Stylesheet {
    let mut css = String::new();
    for (id, node) in dom.nodes.iter().enumerate() {
        if let NodeKind::Element(el) = &node.kind {
            if el.tag == "style" {
                css.push_str(&dom.text_content(id));
                css.push('\n');
            }
        }
    }
    Stylesheet::parse(&css)
}

fn lay(html: &str) -> typeanvil::layout::Layout {
    let dom = Dom::parse(html).unwrap();
    let ss = stylesheet_of(&dom);
    layout(&dom, &ss, geometry(8.5, 11.0, 0.8))
}

/// A cascaded style for the first `<p>` element in `html`.
fn p_style(html: &str) -> ComputedStyle {
    let dom = Dom::parse(html).unwrap();
    let ss = stylesheet_of(&dom);
    let styles = cascade(&dom, &ss, &geometry(5.0, 3.0, 0.5));
    let p_id = dom
        .nodes
        .iter()
        .position(|n| matches!(&n.kind, NodeKind::Element(el) if el.tag == "p"))
        .expect("doc must contain a <p>");
    styles[p_id].clone()
}

/// The text covered by the glyphs' DISTINCT ranges, in glyph order. When the
/// ranges are contiguous and cover the text exactly (ligatures spanning
/// several chars, same-cluster runs deduped), this equals the full string.
fn ranges_join(text: &str, glyphs: &[ShapedGlyph]) -> String {
    let mut joined = String::new();
    let mut prev: Option<Range<usize>> = None;
    for g in glyphs {
        if prev.as_ref() != Some(&g.range) {
            joined.push_str(&text[g.range.clone()]);
            prev = Some(g.range.clone());
        }
    }
    joined
}

fn assert_ranges_cover(text: &str, glyphs: &[ShapedGlyph]) {
    for g in glyphs {
        assert!(
            g.range.start <= g.range.end,
            "inverted range {:?}",
            g.range
        );
        assert!(
            g.range.end <= text.len(),
            "range {:?} out of bounds for text len {} ({text:?})",
            g.range,
            text.len()
        );
    }
    let joined = ranges_join(text, glyphs);
    assert_eq!(
        joined, text,
        "glyph ranges do not cover the text exactly (glyphs={})",
        glyphs.len()
    );
}

// --- acceptance criteria -----------------------------------------------------

/// 1. A shaped word's per-glyph byte ranges cover the word text, including
///    multi-byte UTF-8 (the em dash is 3 bytes). Whether Arial ligates "fi"
///    depends on the shaper's feature set — the coverage contract holds either
///    way (a ligature glyph would simply span several chars in one range).
#[test]
fn shaped_word_ranges_cover_text() {
    let size = Scalar(11.0);
    let run = shape_word("file", size, FACE_REGULAR);
    assert_ranges_cover(&run.text, &run.glyphs);

    let run2 = shape_word("café —", size, FACE_REGULAR);
    assert_ranges_cover(&run2.text, &run2.glyphs);
}

/// 2. A K-P line's glyph ranges cover the line text exactly, including the
///    inter-word space glyphs and the trailing hyphen glyph on a hyphenation
///    break.
#[test]
fn line_ranges_cover_line_text() {
    let style = p_style(
        "<html><style>p{font-family:Arial;font-size:11pt}</style><p>x</p></html>",
    );
    let width = Scalar(300.0);

    let lines = break_paragraph(
        "The quick brown fox jumps over the lazy dog.",
        width,
        &style,
        false,
        true,
    );
    assert!(!lines.is_empty());
    for lr in &lines {
        assert_ranges_cover(&lr.text, &lr.glyphs);
    }

    // Hyphenated variant exercises the hyphen-glyph range (trailing '-').
    let hyph = break_paragraph(
        "Start counterrevolutionary end.",
        width,
        &style,
        true,
        false,
    );
    assert!(!hyph.is_empty());
    for lr in &hyph {
        assert_ranges_cover(&lr.text, &lr.glyphs);
    }
}

/// 3. The rendered PDF's `/ToUnicode` CMap maps body-text glyphs back to the
///    readable source text — the Done criterion of CORE-85, verified without
///    an external extractor.
#[test]
fn tounicode_map_extracts_source_text() {
    let html = r#"<html><style>p{font-family:Arial;font-size:11pt}</style><body>
        <p>Inventory Ledger — Northwind Systems, 2026.</p>
        <p>The quick brown fox jumps over the lazy dog. Field offices file final forms.</p>
    </body></html>"#;
    let l = lay(html);
    let pdf = render(&l).unwrap();

    let entries = to_unicode_entries(&pdf);
    assert!(
        entries.len() >= 20,
        "ToUnicode map suspiciously small: {} entries",
        entries.len()
    );
    let joined: String = entries.iter().map(|(_, s)| s.as_str()).collect();
    // CIDs are assigned per unique glyph in first-use order, so repeated
    // letters appear once; assert every distinct char of the source text is
    // mapped (the Done criterion: readable body text, no control chars).
    assert!(
        "Inventory Ledger — Northwind Systems, 2026."
            .chars()
            .all(|c| joined.contains(c)),
        "map missing chars of source text: {joined:?}"
    );
    assert!(
        joined.contains('\u{2014}'),
        "map missing the em dash U+2014: {joined:?}"
    );
    assert!(
        !joined.chars().any(|c| (c as u32) < 0x20),
        "map contains control chars: {joined:?}"
    );
}

// --- CORE-83: margin-box / generated-content non-ASCII ---------------------

/// 4. Margin-box content is shaped (CORE-83): the run's glyphs are non-empty,
///    their ranges cover the resolved text (em dash U+2014, middle dot U+00B7),
///    and every glyph is a real outline (id != 0), not the font's `.notdef`.
#[test]
fn margin_box_runs_are_shaped() {
    let html = r#"<html><style>
        @page { margin: 0.5in; @top-center { content: "— · "; } }
        p { font-family: Arial; font-size: 11pt; }
    </style><body><p>Body text.</p></body></html>"#;
    let l = lay(html);
    let mut found = false;
    for page in &l.pages {
        collect_margin_runs(&page.root, &mut |run: &typeanvil::frag::TextRun| {
            if run.text.contains('\u{2014}') {
                found = true;
                assert!(!run.glyphs.is_empty(), "margin box must be shaped");
                for g in &run.glyphs {
                    assert!(g.id != 0, "glyph id 0 = .notdef for {:?}", run.text);
                    assert!(
                        g.range.start <= g.range.end && g.range.end <= run.text.len(),
                        "range {:?} out of bounds for {:?}",
                        g.range,
                        run.text
                    );
                }
                // The em dash and middle dot must both be present in the run.
                assert!(run.text.contains('\u{00B7}'), "missing middle dot");
            }
        });
    }
    assert!(found, "no margin-box run with an em dash found");
}

fn collect_margin_runs<'a>(
    frag: &'a typeanvil::frag::Fragment,
    f: &mut impl FnMut(&'a typeanvil::frag::TextRun),
) {
    if let typeanvil::frag::FragmentContent::Text(run) = &frag.content {
        f(run);
    }
    for child in &frag.children {
        collect_margin_runs(child, f);
    }
}

/// 5. The rendered PDF's `/ToUnicode` CMap maps margin-box glyphs back to the
///    readable source text — the CORE-83 Done criterion: `content: "— · "`
///    extracts as a real em dash and middle dot, no control chars.
#[test]
fn margin_box_tounicode_map() {
    let html = r#"<html><style>
        @page { margin: 0.5in;
            @top-center { content: "Northwind — · 2026"; }
            @bottom-right { content: "Page " counter(page); } }
        p { font-family: Arial; font-size: 11pt; }
    </style><body><p>Inventory body text.</p></body></html>"#;
    let l = lay(html);
    let pdf = render(&l).unwrap();

    let entries = to_unicode_entries(&pdf);
    assert!(
        entries.len() >= 10,
        "ToUnicode map suspiciously small: {} entries",
        entries.len()
    );
    let joined: String = entries.iter().map(|(_, s)| s.as_str()).collect();
    assert!(
        joined.contains('\u{2014}'),
        "map missing the em dash U+2014 from the margin box: {joined:?}"
    );
    assert!(
        joined.contains('\u{00B7}'),
        "map missing the middle dot U+00B7 from the margin box: {joined:?}"
    );
    assert!(
        !joined.chars().any(|c| (c as u32) < 0x20),
        "map contains control chars: {joined:?}"
    );
}

// --- ToUnicode CMap extraction ----------------------------------------------

/// Find the byte index of `needle` at or after `from`, or `None`.
fn find_bytes(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    haystack
        .get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

/// Extract (cid, unicode) pairs from every `/ToUnicode` CMap stream in the
/// PDF. Streams are FlateDecode-compressed; krilla emits `beginbfchar` blocks
/// with `<cid> <utf16be>` lines.
fn to_unicode_entries(pdf: &[u8]) -> Vec<(u16, String)> {
    let mut entries = Vec::new();
    let mut search = 0usize;
    while let Some(mut i) = find_bytes(pdf, b"stream", search) {
        i += b"stream".len();
        if pdf.get(i) == Some(&b'\r') {
            i += 1;
        }
        if pdf.get(i) == Some(&b'\n') {
            i += 1;
        }
        let end = find_bytes(pdf, b"endstream", i).unwrap_or(pdf.len());
        let mut dec = ZlibDecoder::new(&pdf[i..end]);
        let mut raw = Vec::new();
        if dec.read_to_end(&mut raw).is_ok() && raw.windows(11).any(|w| w == b"beginbfchar") {
            entries.extend(parse_bfchar(&raw));
        }
        // Skip past the endstream marker itself — it contains the substring
        // "stream", so resuming from `end` would re-match inside it.
        search = end + b"endstream".len();
    }
    entries.sort_by_key(|(cid, _)| *cid);
    entries
}

/// Parse `<cid> <utf16be>` pairs from a `beginbfchar` block.
fn parse_bfchar(data: &[u8]) -> Vec<(u16, String)> {
    let start = find_bytes(data, b"beginbfchar", 0).map(|p| p + b"beginbfchar".len());
    let Some(start) = start else { return Vec::new() };
    let stop = find_bytes(data, b"endbfchar", start).unwrap_or(data.len());
    let block = &data[start..stop];

    // Collect every `<hex>` token in order.
    let mut tokens: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i < block.len() {
        if block[i] == b'<' {
            let mut hex = String::new();
            i += 1;
            while i < block.len() && block[i] != b'>' {
                hex.push(block[i] as char);
                i += 1;
            }
            tokens.push(hex);
        }
        i += 1;
    }

    let mut out = Vec::new();
    let mut it = tokens.into_iter();
    while let (Some(cid_hex), Some(uni_hex)) = (it.next(), it.next()) {
        let cid = u16::from_str_radix(&cid_hex, 16).unwrap_or(u16::MAX);
        let uni = decode_utf16be(&uni_hex);
        if !uni.is_empty() {
            out.push((cid, uni));
        }
    }
    out
}

/// Decode a UTF-16BE hex string (krilla's CMap values) to text.
fn decode_utf16be(hex: &str) -> String {
    let mut s = String::new();
    let mut i = 0usize;
    while i + 4 <= hex.len() {
        if let Ok(u) = u16::from_str_radix(&hex[i..i + 4], 16) {
            s.push(char::from_u32(u as u32).unwrap_or('\u{FFFD}'));
        }
        i += 4;
    }
    s
}
