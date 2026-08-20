---
title: Typography Layer
slug: /specifications/typography-layer
type: spec
status: draft
owner: elijah
created: 2026-08-16
updated: 2026-08-20
sidebar_position: 4
tags: [typography, line-breaking, knuth-plass, shaping, engine]
spec_id: typography-layer
issue_id: CORE-53
applies_to: engine 0.x
dependencies: [wpt-conformance-harness, fragmentation-core, paged-media-css]
---

# Typography Layer

## Overview

The demo-able differentiator: **nobody in the HTML-to-PDF space ships this —
not even Prince.**

- **Knuth-Plass total-fit line breaking with a real glue model** (per-space
  stretch/shrink). Typst's `linebreak.rs` is the reference implementation but
  has *no glue concept*; ours goes further.
- **Character protrusion** — punctuation hangs into the margin so the optical
  text-block edge looks perfectly straight.
- **Font expansion** — per-line glyph advance adjustment (±1–2%) for better
  spacing.
- **Liang hyphenation** via `hypher`; break opportunities per Unicode line-break
  rules (icu4x segmenter); **shaping via HarfRust**.

This issue replaces the skeleton's greedy first-fit line breaker and its
approximate 0.5-em-per-char width model with a real shaping + total-fit
pipeline. It is the *visible* wedge: uniform spacing, no rivers, fewer hyphens.

**Done:** a side-by-side A4 spread — Typeanvil vs Prince vs Chromium on
identical input — visibly better texture. This is the marketing asset for
typography-sensitive buyers (legal, publishing).

## Goals / Non-Goals

**Goals**

- Real text shaping via **HarfRust** (0.13): per-word glyphs with advances,
  replacing the 0.5-em-per-char approximation everywhere main text is
  measured.
- **Knuth-Plass total-fit line breaking** with a glue model: dynamic
  programming over break opportunities minimizing a demerit function
  (spacing badness + hyphen penalty + adjacent-line compatibility), with
  per-space stretch/shrink glue. Typst's `linebreak.rs` is the reference.
- **Character protrusion**: punctuation at line edges hangs optically into the
  margin (applied at draw time; does not change line breaking).
- **Font expansion**: per-line glyph advance scaling within ±2% (a post-pass
  that makes justified lines fit exactly).
- **Liang hyphenation** via `hypher`, integrated as break opportunities with a
  penalty in the K-P objective.
- **Determinism**: identical input → byte-identical PDF; all shaping, breaking,
  and microtypography passes deterministic.
- **The demo**: a fixture rendered side-by-side (Typeanvil vs Chromium at
  minimum, Prince placeholder when a license is available) with visibly better
  texture.

**Non-Goals** (deferred; scope stays honest)

- Full `microtype` package feature set (kerning expansion classes per script,
  character protrusion per-locale tables beyond a small default set).
- bidi / complex-script reordering beyond what HarfRust does by default for
  the embedded font.
- Variable fonts / font fallback across a font collection (single embedded
  font remains; fontique-based discovery is a later issue).
- Grid/flex text layout, math layout, tables' typographic refinements.
- Changing the fragmentation or paged-media machinery: the fragment tree,
  break tokens, margin boxes, and outline emission are untouched by this
  issue (they consume the improved `TextRun`).

## Behavior

The engine shall:

1. **Shape text with HarfRust**: for each text run, shape the run with the
   embedded font at its font size, producing per-glyph glyph ids and advances.
   Replace the 0.5-em-per-char advance approximation in all measurement and
   drawing paths for main text.
2. **Break lines with Knuth-Plass total fit**: line breaking over a paragraph
   minimizes total demerits (spacing badness from the glue model, hyphen
   penalty, adjacent-line incompatibility) via dynamic programming; the result
   is a set of breakpoints, not a greedy fill-and-move-on.
3. **Model glue**: every inter-word space has a natural width (the font's space
   glyph advance) plus explicit stretch and shrink parameters. Justified lines
   distribute the excess/deficit over the line's glue with bounded stretch.
4. **Justify when asked**: `text-align: justify` (and `text-justify:
   inter-word` default) makes every non-final line's ink width equal the
   content width by stretching/shrinking glue; the final line is not
   justified. Left/right/center alignment remain supported.
5. **Hyphenate via Liang patterns**: `hyphenate` enabled (or `hyphens: auto`)
   offers hyphenation break opportunities from `hypher`'s Knuth-Liang patterns;
   each hyphen opportunity carries the K-P hyphen penalty (default 135, per
   Typst). Hyphenation is off by default.
6. **Take break opportunities from Unicode line-break rules**: break
   opportunities come from an icu4x segmenter (`icu_segmenter`); where the
   segmenter's data provider is impractical in this build, a pure-Rust UAX #14
   implementation (`unicode-linebreak`) may stand in behind one function —
   documented deviation, same observable behavior for Latin text.
7. **Protrude punctuation**: a punctuation glyph (`. , ; : ! ? - ' " ) ] }`
   and friends, default table) at the start/end of a line shifts horizontally
   into the margin by a per-character protrusion factor. Protrusion is
   optical only — applied at draw time, does not affect line breaking or
   measured width.
8. **Expand fonts per line**: within a justified paragraph, the line's glyph
   advances may be scaled by a per-line factor in [−2%, +2%] so the line fits
   the content width exactly (a deterministic post-pass on top of glue
   stretching; expansion is applied only when it reduces residual error).
9. **Breakpoint glue is consumed by the break (CORE-94)**: the glue at a
   line's break point produces no space glyph and contributes neither its
   natural width nor its stretch/shrink to the line. The DP's
   `adjustment_ratio` measures `items[start..end]` (exclusive of the break
   item) and the materializer must match: counting the breakpoint glue
   inflated the line's natural width, over-stretched it, then shrank it via
   negative expansion — leaving justified lines ~4pt short of the measure
   and diverging from Prince's line counts. Verified 2026-08-20: prose
   justified lines now reach the content edge (spec §Behavior 4), matching
   Prince to <0.5pt.
10. **Draw glyphs, not strings**: the PDF backend draws main-text lines via
   krilla's `draw_glyphs` (glyph ids + advances), not the high-level
   `draw_text` string path, so shaped widths, protrusion offsets, and
   expansion factors reach the output exactly as laid out.
11. **Map every glyph back to its source text**: each shaped glyph carries the
    byte range of its cluster in the run's text (`ShapedGlyph.range`); the
    PDF backend passes those ranges to krilla, which slices the text by them
    to build the font's `/ToUnicode` CMap. Body text must therefore extract,
    copy, and search as the source text (readable strings, no control
    chars) in every emitted PDF (CORE-85). Empty ranges are prohibited — a
    glyph with no text mapping yields an empty CMap entry and garbage
    extraction.
12. **Shape every run**: `TextRun` carries the shaped glyphs and resolved
    text; `Fragment`/`Fragmentainer`/break-token structure is unchanged.
    Margin boxes and generated content (single short lines) are shaped with
    `shape_word` like body text — no run uses the raw `draw_text` string
    path, so non-ASCII (em dash, curly quotes, `·`) always renders as a real
    glyph with a ToUnicode mapping (CORE-83).
13. **Stay deterministic**: shaping (fixed font bytes + size), K-P (pure
    function of widths/opportunities), protrusion, and expansion are all
    deterministic; no hash-order dependence; identical input yields
    byte-identical PDF.
14. **Keep everything else green**: fragmentation (CORE-51) and paged-media
    (CORE-52) acceptance tests pass unchanged.
15. **Ship the demo**: a fixture (`typography-demo.html`) rendering a
    typography-sensitive paragraph set; a script produces the side-by-side
    spread (Typeanvil + Chromium now; Prince slot reserved).

## Interfaces

**New module** `engine/src/typography.rs`:

```text
ShapedGlyph         // one shaped glyph
  id: u32                     // glyph id in the embedded font
  x_advance: Scalar           // advance in points at the run's font size
  x_offset: Scalar            // optional positioning offset (kerning etc.)
  range: Range<usize>         // byte range of the glyph's cluster in the
                              // paired text (word text for ShapeRun glyphs,
                              // line text for LineResult glyphs); feeds the
                              // PDF /ToUnicode map (CORE-85)

ShapeRun            // a shaped word/segment
  text: String                // original text (for the PDF text arg)
  glyphs: Vec<ShapedGlyph>
  width: Scalar               // sum of advances
  font_size: Scalar

Glue                // inter-word space
  width: Scalar               // natural width (space glyph advance)
  stretch: Scalar             // how much it may grow
  shrink: Scalar              // how much it may shrink

BreakOpportunity    // a candidate break
  kind: Space | Hyphen | Rule   // space, hyphenation, or UAX#14 rule
  penalty: f64                 // 0 for space, hyphen penalty (135) for hyphen
  width: Scalar                // cost of the break (hyphen glyph etc.)

LineResult          // one K-P line
  items: Vec<LineItem>         // shaped words + glue + optional hyphen glyph
  natural_width: Scalar
  stretch_used: Scalar         // glue stretch applied (justified lines)
  shrink_used: Scalar
  expansion: f64               // per-line glyph advance scale in [-0.02, 0.02]
  protrude_left: Scalar        // optical hang for first glyph (punctuation)
  protrude_right: Scalar       // optical hang for last glyph
  text: String                 // resolved text incl. hyphens

pub fn break_paragraph(
    text: &str,
    max_width: Scalar,
    style: &ComputedStyle,
    hyphenate: bool,
    justify: bool,
) -> Vec<LineResult>

pub fn shape_word(word: &str, font_size: Scalar) -> ShapeRun
pub fn line_break_opportunities(text: &str) -> Vec<usize>   // UAX #14
pub fn protrusion_for(ch: char) -> (Scalar, Scalar)          // left, right hang
```

**`engine/src/frag.rs`** — `TextRun` gains shaped glyphs:

```text
TextRun {
  text: String,
  baseline: Point,
  font_size: Scalar,
  color: Color,
  font_family: String,
  glyphs: Vec<ShapedGlyph>,   // all runs carry shaped glyphs (CORE-83)
  expansion: f64,             // NEW — per-line advance scale (default 0.0)
  protrude_left: Scalar,      // NEW
  protrude_right: Scalar,     // NEW
}
```

`ShapedGlyph` is re-exported from `crate::typography`.

**`engine/src/layout.rs`** — the greedy `break_lines` is replaced by
`break_paragraph` from `typography.rs` for main text. `measure_block` (the
`break-inside: avoid` heuristic) uses the same breaker so measured heights
match laid-out heights. Text runs produced by generated content (TOC entries,
margin boxes) are shaped single-line via `shape_word` at their resolved font
size/face, so non-ASCII maps to a glyph + ToUnicode range (CORE-83); no
justification/protrusion is applied to them.

**`engine/src/pdf.rs`** — `Line` fragments with non-empty `glyphs` draw via
`surface.draw_glyphs(start, glyphs, font, text, font_size, outlined=false)`.
Every run (body text, generated content, margin boxes) carries shaped glyphs
(CORE-83); the `draw_text` fallback remains only for degenerate empty runs.
Protrusion offsets and expansion factors are applied to glyph
positions/advances at draw time.

**`engine/Cargo.toml`** — add `harfrust` (0.13), `hypher` (0.1), and one of
`icu_segmenter` (with a data provider that builds cleanly) or
`unicode-linebreak` (pure Rust UAX #14 fallback — documented deviation).
No C FFI; all pure Rust per the architecture brief.

**CLI**: unchanged.

## Acceptance Criteria

Given/When/Then, each mapping to a real test in `engine/tests/typography.rs`:

1. **Real shaping widths** — Given the text `"WWWW"` and `"iiii"` at the same
   font/size, when shaped, then their widths differ (real glyph metrics, not
   character count) (`typography.rs::shaping_real_widths`).
2. **Glue model present** — Given a justified paragraph, when broken, then
   each non-final line's ink width equals the content width within epsilon
   (glue stretch/shrink applied) (`typography.rs::justified_lines_fill_width`).
3. **K-P beats greedy** — Given a paragraph with several possible breakpoints,
   when broken with total fit vs greedy first-fit, then the total-fit result
   has lower total demerit (measured spacing deviation) — the paragraph is
   not just first-fit in disguise (`typography.rs::total_fit_beats_greedy`).
4. **Hyphenation** — Given `hyphens: auto` and a long word that cannot fit on
   a line, when broken, then the word breaks with a hyphen glyph; without
   hyphenation the same word does not break (`typography.rs::hyphenation_breaks`).
5. **Protrusion** — Given a justified paragraph ending a line with `.` or `,`,
   when laid out, then the trailing punctuation's drawn x exceeds the content
   box right edge by the protrusion amount (`typography.rs::protrusion_hangs`).
6. **Expansion** — Given a justified paragraph whose lines cannot fill exactly
   with glue alone, when laid out with expansion enabled, then at least one
   line uses a nonzero expansion factor within ±2%, and residual error is
   smaller than without expansion (`typography.rs::expansion_applied`).
7. **Break opportunities** — Given text with a punctuation cluster, when
   `line_break_opportunities` runs, then breaks are allowed at Unicode line
   break boundaries and not inside words (UAX #14 semantics)
   (`typography.rs::uax14_opportunities`).
8. **Determinism** — Given the same typography fixture rendered twice, then the
   PDF bytes are identical (`typography.rs::determinism_typography`).
9. **Regression: fragmentation + paged-media** — the existing CORE-51/CORE-52
   tests pass unchanged (they consume `TextRun`).
10. **ToUnicode map** — Given a doc whose body text contains repeated letters,
    ligature-friendly words, and a non-ASCII char (em dash), when rendered,
    then the PDF's `/ToUnicode` CMap maps every distinct source char back to
    readable text with no control chars
    (`tounicode.rs::tounicode_map_extracts_source_text`), and every shaped
    word's and line's glyph ranges cover their text exactly
    (`tounicode.rs::shaped_word_ranges_cover_text`,
    `tounicode.rs::line_ranges_cover_line_text`).
11. **Justified lines fill the measure (CORE-94)** — Given a justified
    paragraph with no hyphenation, when broken at content width W, then every
    non-final line's ink width equals W within 0.5pt (the breakpoint glue is
    consumed by the break, so the line is not left ~4pt short). Verified
    against Prince 16.2: prose justified lines reach the content edge to
    <0.5pt, matching Prince's measure.
12. **Demo fixture** — Given `engine/tests/fixtures/typography-demo.html`, when
    rendered, then it is multi-page, deterministic, and a side-by-side spread
    script produces a PNG with Typeanvil's render (`typography.rs::demo_fixture`,
    plus `scripts/render-typography-demo.sh` for the comparison image).
12. **Margin-box / generated-content non-ASCII** — Given a margin box with
    `content: "Northwind — · 2026"` and an element with a non-ASCII
    `content` string, when rendered, then the margin-box run's glyphs are
    non-empty, their ranges cover the resolved text, and the PDF's
    `/ToUnicode` CMap maps the em dash (U+2014) and middle dot (U+00B7) back
    to readable text with no control chars and no raw-byte mojibake
    (`tounicode.rs::margin_box_runs_are_shaped`,
    `tounicode.rs::margin_box_tounicode_map`). This requires the stylesheet
    comment stripper to be char-safe (CORE-83 — the old byte-wise stripper
    mangled every multi-byte UTF-8 char in the stylesheet source).

## Edge Cases

- **Single word wider than the line** → overflow like a monolithic line
  (consistent with fragmentation's last-resort placement).
- **Very long unbreakable word, hyphenation off** → overflows; never split.
- **Hyphenation on but no dictionary patterns for the word** → falls back to
  overflow.
- **Justify + short final line** → final line not justified (natural width).
- **Expansion beyond ±2% would be needed** → clamp at the bound; residual
  error remains (deterministic).
- **Protrusion on a line that is already at the content edge** → still hangs
  (optical only); never changes line breaking.
- **Empty text run** → no lines.
- **Text with only spaces** → no lines.
- **Mixed punctuation at line start and end** → both edges may protrude.
- **Hyphen + protrusion on the same line** → independent; hyphen glyph does
  not protrude (default table excludes it unless configured).
- **Font glyph missing** (e.g. a char absent from Arial) → `.notdef` glyph
  from the font; deterministic; no crash.

## References

- Research brief: `docs/research/rust-ecosystem/typeanvil-rust-typesetting-research.md`
  (wrap-vs-build: HarfRust, hypher, icu4x, krilla; Typst's linebreak.rs as the
  K-P reference).
- Vault: `brain/Projects/Typeanvil/Project Overview.md` — "Why LaTeX Output
  Looks Crisper" section (K-P, protrusion, expansion, glue, determinism).
- Knuth & Plass, "Breaking Paragraphs into Lines" (1981); Typst
  `typst-layout/src/inline/linebreak.rs` (costs: hyphen 135, runt 100).
- CORE-51 spec (fragment contract), CORE-52 spec (TextRun consumers, margin
  boxes, generated content).
- W3C: css-text-3 (`text-align`, `text-justify`, `hyphens`, `overflow-wrap`),
  UAX #14 (Unicode line breaking).
- Crates: harfrust 0.13 (complete HarfBuzz port), hypher 0.1.7 (Knuth-Liang),
  krilla 0.8 `draw_glyphs` (surface.rs), icu_segmenter / unicode-linebreak.
