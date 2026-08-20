//! Typography: real text shaping + Knuth-Plass total-fit line breaking.
//!
//! This module replaces the skeleton's greedy first-fit breaker and its
//! 0.5-em-per-char width approximation with a real pipeline:
//!
//! - **Shaping** (HarfRust 0.13): each word is shaped against the embedded
//!   font face at its font size, producing per-glyph ids and advances in points.
//! - **Break opportunities** (UAX #14 via `unicode-linebreak` 0.1.5): candidate
//!   break points between words. The spec's Interfaces name `icu_segmenter` but
//!   allow a pure-Rust UAX #14 fallback behind one function
//!   ([`line_break_opportunities`]) — documented deviation, same observable
//!   behavior for Latin text.
//! - **Hyphenation** (`hypher` 0.1.7, Knuth-Liang patterns): extra break
//!   opportunities inside long words, each carrying the K-P hyphen penalty.
//! - **Knuth-Plass total fit**: dynamic programming over break opportunities
//!   minimizing total demerits (spacing badness from the glue model + hyphen
//!   penalty + adjacent-line incompatibility), with per-space stretch/shrink
//!   glue. Not a first-fit wrapper.
//! - **Protrusion** and **font expansion**: optical microtypography applied at
//!   draw time (protrusion) or as a per-line advance scale (expansion).
//!
//! Everything here is deterministic: shaping is a pure function of (font bytes,
//! size, text); the K-P engine is a pure function of widths/opportunities;
//! protrusion and expansion are pure functions of their inputs. Identical input
//! therefore yields byte-identical PDF.
//!
//! ## HarfRust API notes (0.13)
//!
//! Shaping goes through `read_fonts::FontRef` → `harfrust::ShaperData::new` →
//! `data.shaper(&font).build()` → `shaper.shape(buffer, ShapeOptions)`. HarfRust
//! has *no font-size property* (shaping is always in font design units); we
//! scale advances by `font_size / units_per_em` ourselves. Glyph ids come from
//! `GlyphInfo::glyph_id`; advances from `GlyphPosition::x_advance` (i32 design
//! units).

use std::ops::Range;
use std::sync::LazyLock;

use harfrust::{Direction, FontRef, ShaperData, ShapeOptions, UnicodeBuffer};
use hypher::Lang;
use read_fonts::TableProvider;
use unicode_linebreak::{linebreaks, BreakOpportunity as UaxBreak};

use crate::css::ComputedStyle;
use crate::fonts::{face_path, FontFace};
use crate::geom::Scalar;

/// The K-P hyphen penalty (Typst's default, from Knuth-Plass §hyphenation).
const HYPHEN_PENALTY: f64 = 135.0;

/// The embedded font bytes, loaded once per face. `'static` so a [`FontRef`]
/// can borrow them for the whole process. The initializer is fixed, hence
/// `LazyLock`.
static FACE_BYTES: [LazyLock<Vec<u8>>; 4] = [
    LazyLock::new(|| {
        std::fs::read(face_path(FontFace::Regular))
            .expect("reading embedded font for shaping")
    }),
    LazyLock::new(|| {
        std::fs::read(face_path(FontFace::Bold)).expect("reading embedded font for shaping")
    }),
    LazyLock::new(|| {
        std::fs::read(face_path(FontFace::Italic))
            .expect("reading embedded font for shaping")
    }),
    LazyLock::new(|| {
        std::fs::read(face_path(FontFace::BoldItalic))
            .expect("reading embedded font for shaping")
    }),
];

/// Cached shaper data (OpenType tables) per face, built once. Borrows the
/// corresponding entry in [`FACE_BYTES`]; both live for the process, so
/// shaping never re-reads or re-parses the font.
static FACE_SHAPERS: [LazyLock<(FontRef<'static>, ShaperData)>; 4] = [
    LazyLock::new(|| {
        let bytes = &FACE_BYTES[FontFace::Regular as usize];
        let font = FontRef::new(bytes).expect("parsing embedded font for shaping");
        let data = ShaperData::new(&font);
        (font, data)
    }),
    LazyLock::new(|| {
        let bytes = &FACE_BYTES[FontFace::Bold as usize];
        let font = FontRef::new(bytes).expect("parsing embedded font for shaping");
        let data = ShaperData::new(&font);
        (font, data)
    }),
    LazyLock::new(|| {
        let bytes = &FACE_BYTES[FontFace::Italic as usize];
        let font = FontRef::new(bytes).expect("parsing embedded font for shaping");
        let data = ShaperData::new(&font);
        (font, data)
    }),
    LazyLock::new(|| {
        let bytes = &FACE_BYTES[FontFace::BoldItalic as usize];
        let font = FontRef::new(bytes).expect("parsing embedded font for shaping");
        let data = ShaperData::new(&font);
        (font, data)
    }),
];

fn face_shaper(face: FontFace) -> &'static (FontRef<'static>, ShaperData) {
    &FACE_SHAPERS[face as usize]
}

/// The vertical metrics of a face, in fractions of the em square: (ascent,
/// descent). Read from the font's hhea table (the CSS2.1 §10.8.1 font
/// leading baseline: half the leading is split around ascent + descent).
/// All bundled Arial faces share one hhea, but this reads per-face so a
/// future font swap stays correct.
pub fn font_metrics(face: FontFace) -> (f64, f64) {
    let (font, _) = face_shaper(face);
    let hhea = font.hhea().expect("hhea table for font metrics");
    let upem = font.head().expect("head table").units_per_em() as f64;
    let asc = hhea.ascender().to_i16() as f64 / upem;
    let desc = hhea.descender().to_i16() as f64 / upem; // negative
    (asc, desc)
}

/// The CSS2.1 §10.8.1 baseline offset: the distance from a line box's top to
/// its text baseline. `ascent + half-leading`, where leading = line-height −
/// (ascent + descent) and ascent/descent are the font's hhea metrics scaled
/// to `font_size`. TypeAnvil historically placed the baseline at `font_size`
/// (ascent assumed = font size, no half-leading); Prince uses this formula,
/// which is what CORE-90 proved. Line box HEIGHT is unchanged (still
/// `line_height`); only the glyph baseline inside the box moves.
pub fn baseline_offset(font_size: Scalar, line_height: Scalar, face: FontFace) -> Scalar {
    let (asc_em, desc_em) = font_metrics(face);
    let ascent = font_size.get() * asc_em;
    let descent = font_size.get() * -desc_em;
    let half_leading = (line_height.get() - ascent - descent) * 0.5;
    Scalar(ascent + half_leading)
}

/// One shaped glyph.
#[derive(Clone, Debug)]
pub struct ShapedGlyph {
    /// Glyph id in the selected face.
    pub id: u32,
    /// Advance in points at the run's font size.
    pub x_advance: Scalar,
    /// Positioning offset in points (kerning etc.).
    pub x_offset: Scalar,
    /// Byte range of the glyph's cluster in the paired text string (the
    /// `ShapeRun.text` for shaped words, the `LineResult.text` for line
    /// glyphs). krilla slices the text by these ranges to build the PDF
    /// ToUnicode map, so every glyph's range must be non-empty and in-bounds
    /// (CORE-85).
    pub range: Range<usize>,
}

/// A shaped word or segment.
#[derive(Clone, Debug)]
pub struct ShapeRun {
    /// Original text (for the PDF `text` argument).
    pub text: String,
    /// The shaped glyphs, in visual order.
    pub glyphs: Vec<ShapedGlyph>,
    /// Sum of glyph advances, in points.
    pub width: Scalar,
    /// Font size the run was shaped at, in points.
    pub font_size: Scalar,
}

/// An inter-word space with stretch/shrink parameters (the glue model).
#[derive(Clone, Copy, Debug)]
pub struct Glue {
    /// Natural width (the font's space-glyph advance), in points.
    pub width: Scalar,
    /// How much the glue may grow when justifying, in points.
    pub stretch: Scalar,
    /// How much the glue may shrink when justifying, in points.
    pub shrink: Scalar,
}

/// The kind of a break opportunity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BreakKind {
    /// A break at an inter-word space (natural, zero penalty).
    Space,
    /// A hyphenation break inside a word (carries the hyphen penalty + glyph).
    Hyphen,
    /// A UAX #14 mandatory or allowed break at a non-space boundary.
    Rule,
}

/// A candidate break in the paragraph's item stream.
#[derive(Clone, Copy, Debug)]
pub struct BreakOpportunity {
    /// What kind of break this is.
    pub kind: BreakKind,
    /// K-P penalty: 0 for a space, [`HYPHEN_PENALTY`] for a hyphen.
    pub penalty: f64,
    /// Extra width the break introduces (a hyphen glyph), in points.
    pub width: Scalar,
}

/// One line produced by the K-P engine.
#[derive(Clone, Debug)]
pub struct LineResult {
    /// Resolved text of the line (incl. a trailing hyphen when hyphenated).
    pub text: String,
    /// The shaped glyphs of the line, in visual order.
    pub glyphs: Vec<ShapedGlyph>,
    /// Natural ink width (sum of word + glue natural widths), in points.
    pub natural_width: Scalar,
    /// Glue stretch applied on this line (justified lines), in points.
    pub stretch_used: Scalar,
    /// Glue shrink applied on this line (justified lines), in points.
    pub shrink_used: Scalar,
    /// Per-line glyph advance scale in [-0.02, 0.02] (font expansion).
    pub expansion: f64,
    /// Optical hang for the first glyph (punctuation), in points.
    pub protrude_left: Scalar,
    /// Optical hang for the last glyph (punctuation), in points.
    pub protrude_right: Scalar,
}

/// Shape a single word against the selected face at `font_size`.
///
/// Returns a [`ShapeRun`] whose glyph advances are in points. Missing glyphs
/// resolve to the font's `.notdef` deterministically (no crash).
pub fn shape_word(word: &str, font_size: Scalar, face: FontFace) -> ShapeRun {
    let (font, data) = face_shaper(face);
    let shaper = data.shaper(font).build();
    let upem = shaper.units_per_em() as f64;
    let scale = font_size.get() / upem;

    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(word);
    // Latin, left-to-right. harfrust guesses script from the codepoints but
    // NOT direction (it passes the buffer's direction into the shape plan
    // verbatim), so set it explicitly — a buffer without a direction panics.
    buffer.set_direction(Direction::LeftToRight);
    let glyph_buffer = shaper.shape(buffer, ShapeOptions::default());

    let infos = glyph_buffer.glyph_infos();
    let positions = glyph_buffer.glyph_positions();
    // HarfRust's `push_str` assigns cluster = byte offset of each char into
    // the word (see `UnicodeBuffer::push_str` — `char_indices`), so a shaped
    // glyph's `cluster` is the byte offset of its grapheme cluster in the
    // word. For LTR Latin (single script, no reordering) the clusters are
    // non-decreasing; a ligature glyph keeps the cluster of its first char
    // and therefore spans multiple source bytes.
    let clusters: Vec<u32> = infos.iter().map(|i| i.cluster).collect();
    let byte_len = word.len();
    let mut glyphs = Vec::with_capacity(infos.len());
    let mut width = 0.0f64;
    for (i, (info, pos)) in infos.iter().zip(positions.iter()).enumerate() {
        let adv = pos.x_advance as f64 * scale;
        let off = pos.x_offset as f64 * scale;
        width += adv;
        let start = clusters[i] as usize;
        // End at the next DISTINCT cluster, or the word's end. Glyphs that
        // share a cluster (e.g. base + combining marks) get the SAME range;
        // krilla assigns codepoints only to the first glyph of a run of equal
        // ranges, so the cluster's text is not duplicated in the ToUnicode
        // map. Every range here is non-empty (a cluster never equals the word
        // length).
        let end = clusters[i + 1..]
            .iter()
            .find(|&&c| c > clusters[i])
            .map(|&c| c as usize)
            .unwrap_or(byte_len);
        glyphs.push(ShapedGlyph {
            id: info.glyph_id,
            x_advance: Scalar(adv),
            x_offset: Scalar(off),
            range: start..end,
        });
    }

    ShapeRun {
        text: word.to_string(),
        glyphs,
        width: Scalar(width),
        font_size,
    }
}

/// UAX #14 break opportunities: byte offsets into `text` *after* which a line
/// may break. Excludes the trivial end-of-text opportunity.
pub fn line_break_opportunities(text: &str) -> Vec<usize> {
    // `unicode-linebreak` yields (byte offset *after* the break, kind). The
    // final offset (text.len()) is the trivial end-of-text opportunity; drop
    // it. Both Mandatory and Allowed breaks are opportunities here.
    linebreaks(text)
        .filter(|&(i, kind)| i < text.len() && matches!(kind, UaxBreak::Allowed | UaxBreak::Mandatory))
        .map(|(i, _)| i)
        .collect()
}

/// The optical protrusion (left hang, right hang) for a character, in em
/// fractions. Zero for non-protruding characters.
pub fn protrusion_for(ch: char) -> (Scalar, Scalar) {
    // Default protrusion table, em fractions (right hang is what matters at a
    // justified line's end; left hang for an opening mark at line start). Values
    // modeled on microtype's defaults for a proportional serif/sans face: small
    // marks hang more, wider marks less. The hyphen is intentionally excluded
    // from the right-hang set here (spec edge case: hyphen glyph does not
    // protrude unless configured).
    let (l, r) = match ch {
        '.' | ',' => (0.0, 0.7),
        ';' | ':' => (0.0, 0.5),
        '!' | '?' => (0.0, 0.2),
        '\'' | '"' | '\u{2019}' | '\u{201d}' => (0.3, 0.3),
        '\u{2018}' | '\u{201c}' => (0.5, 0.0),
        ')' | ']' | '}' => (0.0, 0.2),
        '(' | '[' | '{' => (0.2, 0.0),
        '\u{2013}' | '\u{2014}' => (0.0, 0.3),
        _ => (0.0, 0.0),
    };
    (Scalar(l), Scalar(r))
}

// --- Knuth-Plass internals ------------------------------------------------

/// An item in the K-P item stream: a box (shaped word/fragment), glue (space),
/// or a penalty (candidate break with a cost).
#[derive(Clone, Debug)]
enum Item {
    /// A shaped box that occupies width and never breaks internally.
    Box(ShapeRun),
    /// Inter-word glue with stretch/shrink.
    Glue(Glue),
    /// A candidate breakpoint. `hyphen` carries the trailing hyphen glyph run
    /// to append when the line breaks here.
    Penalty {
        penalty: f64,
        /// Width added to the line if it breaks here (hyphen glyph); 0 if not.
        width: Scalar,
        /// The glyph run for the hyphen, appended to the broken line's text.
        hyphen: Option<ShapeRun>,
        /// Whether breaking here is mandatory (forced newline). Reserved for
        /// future newline support: `build_items` never emits one today.
        #[allow(dead_code)]
        forced: bool,
    },
}

/// Space glue for the selected face at `font_size`: natural width is the space
/// glyph advance; stretch/shrink are the classic TeX fractions of the space
/// (1/2 stretch, 1/3 shrink) for a comfortable justified texture.
fn space_glue(font_size: Scalar, face: FontFace) -> Glue {
    let space = shape_word(" ", font_size, face).width;
    Glue {
        width: space,
        stretch: space * 0.5,
        shrink: space * (1.0 / 3.0),
    }
}

/// Tokenize a paragraph into the K-P item stream. Words are shaped; spaces
/// become glue; `hyphenate` adds intra-word hyphen penalties from `hypher`.
/// UAX #14 boundaries confirm where inter-word breaks are legal.
fn build_items(text: &str, font_size: Scalar, face: FontFace, hyphenate: bool) -> Vec<Item> {
    let glue = space_glue(font_size, face);
    let hyphen_run = shape_word("-", font_size, face);
    let mut items: Vec<Item> = Vec::new();
    let words: Vec<&str> = text.split_whitespace().collect();
    for (wi, word) in words.iter().enumerate() {
        if wi > 0 {
            // Inter-word glue is a legal (zero-penalty) breakpoint.
            items.push(Item::Glue(glue));
        }
        push_word(&mut items, word, font_size, hyphenate, &hyphen_run, face);
    }
    items
}

/// Push one word into the item stream. With `hyphenate`, split the word into
/// Liang syllables and emit a hyphen penalty between each; otherwise emit the
/// whole word as a single box. Trailing/leading punctuation stays attached to
/// its syllable box (protrusion is a draw-time concern).
fn push_word(
    items: &mut Vec<Item>,
    word: &str,
    font_size: Scalar,
    hyphenate: bool,
    hyphen_run: &ShapeRun,
    face: FontFace,
) {
    if !hyphenate || word.chars().count() < 5 {
        items.push(Item::Box(shape_word(word, font_size, face)));
        return;
    }
    // hypher hyphenates only the alphabetic core; shape each syllable as a box
    // and place a hyphen penalty between consecutive syllables.
    let syllables: Vec<&str> = hypher::hyphenate(word, Lang::English).collect();
    if syllables.len() <= 1 {
        items.push(Item::Box(shape_word(word, font_size, face)));
        return;
    }
    for (si, syl) in syllables.iter().enumerate() {
        if si > 0 {
            // Candidate break with a hyphen glyph.
            items.push(Item::Penalty {
                penalty: HYPHEN_PENALTY,
                width: hyphen_run.width,
                hyphen: Some(hyphen_run.clone()),
                forced: false,
            });
        }
        items.push(Item::Box(shape_word(syl, font_size, face)));
    }
}

/// A legal breakpoint the DP considers: index into the item stream where a
/// line may end. `is_break_point(i)` is true for a glue at `i` (break by
/// skipping the glue) or a penalty at `i` with finite penalty.
fn is_break_point(items: &[Item], i: usize) -> bool {
    match &items[i] {
        // A glue is a breakpoint only if preceded by a box (TeX rule).
        Item::Glue(_) => i > 0 && matches!(items[i - 1], Item::Box(_)),
        Item::Penalty { penalty, .. } => *penalty < f64::INFINITY,
        Item::Box(_) => false,
    }
}

/// The adjustment ratio for a line spanning items `start..=end` at content
/// width `max_width`: how much each unit of stretch (ratio>0) or shrink
/// (ratio<0) is used to make the line fit. `None` when the line cannot shrink
/// enough to fit (overfull beyond available shrink → infinitely bad).
fn adjustment_ratio(items: &[Item], start: usize, end: usize, max_width: f64) -> Option<f64> {
    let (mut natural, mut stretch, mut shrink) = (0.0f64, 0.0f64, 0.0f64);
    for it in &items[start..end] {
        match it {
            Item::Box(b) => natural += b.width.get(),
            Item::Glue(g) => {
                natural += g.width.get();
                stretch += g.stretch.get();
                shrink += g.shrink.get();
            }
            Item::Penalty { .. } => {}
        }
    }
    // The paragraph's final line ends at the last box; include it, exactly as
    // a hyphen penalty's width is included at a hyphenation break — otherwise
    // the DP under-measures the final line by a whole word and can choose an
    // overfull one when a better split exists.
    if end + 1 == items.len() {
        if let Item::Box(b) = &items[end] {
            natural += b.width.get();
        }
    }
    let diff = max_width - natural;
    if diff > 0.0 {
        if stretch > 0.0 {
            Some(diff / stretch)
        } else {
            // No glue to stretch: a justified line cannot fill, so the ratio is
            // unbounded (TeX badness 10000). Ragged-right lines ignore this
            // (they never need to fill); the final line is exempt by the DP.
            Some(f64::INFINITY)
        }
    } else if diff < 0.0 {
        if shrink > 0.0 && -diff <= shrink {
            Some(diff / shrink)
        } else {
            None
        }
    } else {
        Some(0.0)
    }
}

/// Badness of an adjustment ratio: 100·|r|³, clamped, per Knuth-Plass. Infinite
fn badness(ratio: f64) -> f64 {
    // 100·|r|³, clamped at 10000 (TeX's maximum badness for |r| > 1: the line
    // would need more stretch/shrink than its glue provides).
    100.0 * ratio.abs().powi(3).min(100.0)
}

/// Run the Knuth-Plass total-fit DP over the item stream. Returns the chosen
/// break item indices (each is where a line ends), in order. This is real
/// dynamic programming minimizing total demerits, not a greedy first-fit pass:
/// each candidate line's demerit combines spacing badness, the breakpoint
/// penalty, and an adjacent-line-incompatibility term. `justify` exempts
/// ragged-right lines from needing to fill the width (their underfullness is
/// free; only the final line is exempt when justifying).
fn knuth_plass(items: &[Item], max_width: f64, justify: bool) -> Vec<usize> {
    let n = items.len();
    if n == 0 {
        return Vec::new();
    }
    // Candidate break positions: every legal breakpoint, plus the paragraph end
    // (n-1 forced as the final line's end). Node 0 is the virtual paragraph
    // start (content begins at item 0).
    let mut nodes: Vec<usize> = (0..n).filter(|&i| is_break_point(items, i)).collect();
    // The final line always ends at the last item.
    if nodes.last() != Some(&(n - 1)) {
        nodes.push(n - 1);
    }

    // DP over nodes. `best[k]` = min total demerit for a paragraph ending with a
    // break at nodes[k]; `prev[k]` = the node index it came from (usize::MAX for
    // the paragraph start). `fit[k]` = fitness class of the last line.
    let m = nodes.len();
    let mut best = vec![f64::INFINITY; m];
    let mut prev = vec![usize::MAX; m];
    let mut fit = vec![1i32; m];

    for k in 0..m {
        let brk = nodes[k];
        let is_last = brk == n - 1;
        let penalty = match &items[brk] {
            Item::Penalty { penalty, .. } => *penalty,
            _ => 0.0,
        };
        // Predecessor = paragraph start (content 0..brk) or an earlier break j
        // (content nodes[j]+1..brk, skipping the break item at nodes[j]).
        for pred in 0..=k {
            let (line_start, base_demerit, base_fit) = if pred == k {
                (0usize, 0.0f64, 1i32) // from paragraph start
            } else {
                let j = pred; // an earlier taken break
                if best[j].is_infinite() {
                    continue;
                }
                (nodes[j] + 1, best[j], fit[j])
            };
            if line_start > brk {
                continue;
            }
            let ratio = match adjustment_ratio(items, line_start, brk, max_width) {
                Some(r) => r,
                None => continue,
            };
            // A justified intermediate line pays the full spacing badness; the
            // final line and (with `justify` off) every underfull line are set
            // at natural width, free.
            let b = if (is_last || !justify) && ratio > 0.0 {
                0.0
            } else {
                badness(ratio)
            };
            let cls = fitness_class(ratio);
            let mut demerit = {
                let base = 1.0 + b;
                if penalty >= 0.0 {
                    base * base + penalty * penalty
                } else {
                    base * base - penalty * penalty
                }
            };
            if (cls - base_fit).abs() > 1 {
                demerit += 100.0;
            }
            let total = base_demerit + demerit;
            if total < best[k] {
                best[k] = total;
                prev[k] = if pred == k { usize::MAX } else { pred };
                fit[k] = cls;
            }
        }
    }

    // The paragraph ends at the last node (brk == n-1); it is the last element.
    let end_k = m - 1;
    if best[end_k].is_infinite() {
        // No viable set of breaks (e.g. an unbreakable overfull word). Fall back
        // to a single line spanning everything (monolithic overflow).
        return vec![n - 1];
    }
    let mut breaks = Vec::new();
    let mut cur = end_k;
    loop {
        breaks.push(nodes[cur]);
        match prev[cur] {
            usize::MAX => break,
            p => cur = p,
        }
    }
    breaks.reverse();
    breaks
}

/// Fitness class of a line by its adjustment ratio (Knuth-Plass): tight,
/// normal, loose, very loose.
fn fitness_class(ratio: f64) -> i32 {
    if ratio < -0.5 {
        0
    } else if ratio <= 0.5 {
        1
    } else if ratio <= 1.0 {
        2
    } else {
        3
    }
}
impl LineResult {
    /// The line's drawn ink width: natural width plus applied glue stretch /
    /// minus shrink, scaled by the expansion factor. What layout measures for
    /// `text-align` offsets and what the PDF backend draws must agree.
    pub fn drawn_width(&self) -> Scalar {
        (self.natural_width + self.stretch_used - self.shrink_used) * (1.0 + self.expansion)
    }
}

/// Materialize one K-P line spanning items `start..=end` into a [`LineResult`].
///
/// Break semantics mirror the DP's [`adjustment_ratio`] measurement: a line
/// covers `items[start..end]` plus the break item at `end` — a penalty's
/// hyphen glyph for a hyphenation break, or a trailing box for the
/// paragraph's final line (the DP always breaks there and measures without
/// the last box so the final line is never spuriously rejected; we include it
/// so no text is lost). A glue at `end` is the breakpoint itself and is
/// consumed by the break. Justified lines distribute the excess/deficit over
/// their inter-word glue (bounded at full stretch/shrink) and then let font
/// expansion absorb the residual, clamped to ±2%.
#[allow(clippy::too_many_arguments)]
fn materialize_line(
    items: &[Item],
    start: usize,
    end: usize,
    max_width: Scalar,
    space_run: &ShapeRun,
    justify_line: bool,
    font_size: Scalar,
) -> LineResult {
    let mut text = String::new();
    let mut glyphs: Vec<ShapedGlyph> = Vec::new();
    // (glyph index, natural width, stretch, shrink) of each inter-word space.
    let mut spaces: Vec<(usize, f64, f64, f64)> = Vec::new();
    let mut natural = 0.0f64;
    let mut total_stretch = 0.0f64;
    let mut total_shrink = 0.0f64;
    let mut pending_glue: Option<Glue> = None;

    for it in &items[start..=end] {
        match it {
            Item::Box(b) => {
                if let Some(g) = pending_glue.take() {
                    // The glue that preceded this box materializes as a space
                    // glyph between the words (advance patched below).
                    if let Some(sg) = space_run.glyphs.first() {
                        let sp = text.len();
                        text.push(' ');
                        let gi = glyphs.len();
                        // Default advance = the glue's natural width so
                        // non-justified lines (final line, ragged-right,
                        // monolithic fallback) keep real spaces. The justify
                        // branch below patches this to stretched/shrunk.
                        glyphs.push(ShapedGlyph {
                            id: sg.id,
                            x_advance: Scalar(g.width.get()),
                            x_offset: Scalar::ZERO,
                            // The single ' ' just appended to the line text
                            // (CORE-85).
                            range: sp..sp + 1,
                        });
                        spaces.push((gi, g.width.get(), g.stretch.get(), g.shrink.get()));
                    }
                }
                let base = text.len();
                text.push_str(&b.text);
                natural += b.width.get();
                // Rebase the word's glyph ranges (word-relative cluster byte
                // offsets) onto the line text (CORE-85).
                glyphs.extend(b.glyphs.iter().map(|g| ShapedGlyph {
                    id: g.id,
                    x_advance: g.x_advance,
                    x_offset: g.x_offset,
                    range: (base + g.range.start)..(base + g.range.end),
                }));
            }
            Item::Glue(g) => {
                pending_glue = Some(*g);
                natural += g.width.get();
                total_stretch += g.stretch.get();
                total_shrink += g.shrink.get();
            }
            Item::Penalty { .. } => {}
        }
    }

    // A hyphenation break appends the hyphen glyph (and its width) to the line.
    if let Item::Penalty {
        width, hyphen: Some(h), ..
    } = &items[end]
    {
        natural += width.get();
        let hp = text.len();
        text.push('-');
        // The hyphen glyph(s) cover the trailing '-' in the line text
        // (CORE-85).
        glyphs.extend(h.glyphs.iter().map(|g| ShapedGlyph {
            id: g.id,
            x_advance: g.x_advance,
            x_offset: g.x_offset,
            range: hp..hp + 1,
        }));
    }

    let (mut stretch_used, mut shrink_used) = (0.0f64, 0.0f64);
    let mut expansion = 0.0f64;
    if justify_line {
        // Glue absorbs up to its full stretch/shrink (bounded glue); the
        // residual is left for font expansion. Overfull beyond shrink applies
        // maximum shrink (the DP rejects such lines, so this is defensive).
        let ratio = adjustment_ratio(items, start, end, max_width.get())
            .unwrap_or(-1.0)
            .clamp(-1.0, 1.0);
        if ratio >= 0.0 {
            stretch_used = ratio * total_stretch;
            for &(gi, w, s, _) in &spaces {
                glyphs[gi].x_advance = Scalar(w + ratio * s);
            }
        } else {
            shrink_used = -ratio * total_shrink;
            for &(gi, w, _, k) in &spaces {
                glyphs[gi].x_advance = Scalar(w + ratio * k);
            }
        }
        let adjusted = natural + stretch_used - shrink_used;
        if adjusted > 0.0 {
            // Per-line glyph-advance scale in [-2%, +2%] making the line fit
            // exactly; nonzero exactly when the glue could not absorb the
            // whole adjustment (spec Behavior §8).
            expansion = ((max_width.get() - adjusted) / adjusted).clamp(-0.02, 0.02);
        }
    }

    // Optical protrusion: em-fraction hang of the line's edge punctuation,
    // in points. Applied at draw time; never affects breaking or measurement.
    let first = text.chars().next().unwrap_or(' ');
    let last = text.chars().next_back().unwrap_or(' ');
    let (pl, _) = protrusion_for(first);
    let (_, rr) = protrusion_for(last);

    LineResult {
        text,
        glyphs,
        natural_width: Scalar(natural),
        stretch_used: Scalar(stretch_used),
        shrink_used: Scalar(shrink_used),
        expansion,
        protrude_left: pl * font_size.get(),
        protrude_right: rr * font_size.get(),
    }
}

/// Break a paragraph into lines with Knuth-Plass total fit.
///
/// `max_width` is the content width in points. `hyphenate` offers Liang
/// hyphenation breaks; `justify` distributes glue so non-final lines fill the
/// content width. Deterministic pure function of its inputs.
pub fn break_paragraph(
    text: &str,
    max_width: Scalar,
    style: &ComputedStyle,
    hyphenate: bool,
    justify: bool,
) -> Vec<LineResult> {
    // Spec edge cases: empty runs and whitespace-only runs produce no lines.
    if text.trim().is_empty() {
        return Vec::new();
    }
    let font_size = style.font_size;
    let face = crate::fonts::face_for(style.font_weight, style.font_style);
    let items = build_items(text, font_size, face, hyphenate);
    if items.is_empty() {
        return Vec::new();
    }
    let breaks = knuth_plass(&items, max_width.get(), justify);
    // The inter-word space glyph, shaped once per paragraph.
    let space_run = shape_word(" ", font_size, face);
    let mut lines = Vec::with_capacity(breaks.len());
    let mut start = 0usize;
    for (bi, &end) in breaks.iter().enumerate() {
        // The final line is never justified (spec Behavior §4).
        let justify_line = justify && bi + 1 < breaks.len();
        lines.push(materialize_line(
            &items,
            start,
            end,
            max_width,
            &space_run,
            justify_line,
            font_size,
        ));
        start = end + 1;
    }
    lines
}
