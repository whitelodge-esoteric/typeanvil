---
title: Font Weight and Style Faces
slug: /specifications/font-weight-style
type: spec
status: draft
owner: elijah
created: 2026-08-19
updated: 2026-08-19
sidebar_position: 12
tags: [engine, css-fonts, typography, pdf, determinism]
spec_id: font-weight-style
issue_id: CORE-80
applies_to: engine 0.x
dependencies: [typography-layer, wpt-conformance-harness]
---

# Font Weight and Style Faces

## Overview

The engine ignores `font-weight` and `font-style` entirely. `engine/src/pdf.rs`
hardcodes one font — `const FONT_PATH: &str = "/System/Library/Fonts/Supplemental/Arial.ttf"`
(regular Arial) — and `engine/src/typography.rs` duplicates the same path for
shaping. No face is ever selected from the computed style: every `h1`/`h2`,
`thead th`, `.total-row`, `.brand`, `.sig .name`, `.abstract`, and `blockquote`
in the demo corpus renders regular-weight, upright Arial.

CORE-79 triage (2026-08-19) measured this as **the single largest visible diff
driver across all six demo docs** (invoice 30.0%, letterhead 14.7%, paper
27.5%, prose 28.5%, report 17.7%, table-stress 33.5%). The verification was
not eyeballed: a minimal repro (`/tmp/boldtest.html`, `.plain` / `.bold` /
`.italic`, Arial 12pt) shows the Typeanvil PDF embeds ONE font
(`/BaseFont …+ArialMT`) while Prince embeds three (`ArialMT`, `Arial-BoldMT`,
`Arial-ItalicMT`), and the bold line measures 257.5pt in Prince where
Typeanvil has no bold variant at all. It also explains why CORE-73 (font
pinning) moved zero scoreboard numbers: the engine used Arial regardless of
the declared stack.

This issue computes `font-weight` / `font-style` from the stylo cascade
(verified present in the computed style, see References) and selects among a
fixed 4-face Arial bundle: regular, bold, italic, bold-italic. The fixed-font
approach stays (fontique discovery and a portable bundled font are later
issues, as the `pdf.rs` comment already notes); the bundle extends it with the
three faces that already exist on this machine.

## Goals / Non-Goals

**Goals**

- `ComputedStyle` carries the computed `font-weight` (numeric) and
  `font-style` (normal | italic; oblique folds into italic).
- A single face-selection rule (`face_for(weight, style)`) resolves every run
  to one of four Arial faces: regular / bold / italic / bold-italic.
- **Shaping and drawing use the same face bytes for a given run** — the
  measured width of a bold run equals its drawn width (this invariant already
  holds for regular; it must hold per face).
- The PDF embeds distinct faces when the document uses them: a doc with
  `font-weight: bold` and `font-style: italic` embeds `ArialMT`,
  `Arial-BoldMT`, `Arial-ItalicMT` (and `Arial-BoldItalicMT` when both apply).
- Determinism: identical input → byte-identical PDF, with a fixed face table
  (no HashMap iteration-order dependence).
- The demo gallery regen drops letterhead/report diff percent toward
  single digits.

**Non-Goals** (deferred; scope stays honest)

- fontique-based face discovery or a portable bundled font (later issue).
- Variable fonts, arbitrary intermediate weights (Arial has 400/700 only:
  500–600 map to the bold face, 100–500 to regular — documented deviation).
- Synthetic bold / synthetic oblique (stroke/oblique) — not needed while the
  four real faces exist on this machine; a future portability issue may add
  them as a fallback.
- `@font-face` / custom font loading; font fallback across families. The
  family is still pinned to Arial regardless of the declared stack (CORE-73
  decision); this issue selects the **face within that family** only.

## Behavior

The engine shall:

1. **Carry weight and style on the computed style.** `ComputedStyle` gains
   `font_weight: f32` (the computed numeric weight: 400 = normal, 700 = bold)
   and `font_style: FontStyle` (engine enum `Normal | Italic`). The cascade
   reads them from stylo via `font.clone_font_weight().value()` and
   `font.clone_font_style()` (see Interfaces for the exact mapping).
2. **Map oblique to italic.** `font-style: oblique` (any angle) computes to
   the `Italic` engine value. Arial has no oblique cut; the italic face is the
   closest available representation. `font-style: normal` maps to `Normal`.
3. **Select the face by one rule.** `fonts::face_for(weight, style)` returns
   `BoldItalic` when weight ≥ 600 and style is italic, `Bold` when weight ≥
   600, `Italic` when style is italic, else `Regular`. The rule lives in one
   place and is the only place a face is chosen.
4. **Shape with the selected face.** The typography layer's `break_paragraph`
   resolves the face from the run's `ComputedStyle` and shapes every word with
   that face's bytes at the run's font size. `shape_word` takes the face
   explicitly.
5. **Draw with the selected face.** Every `TextRun` carries the resolved
   `font_face: FontFace`; the PDF backend loads the matching krilla `Font` for
   both the simple `draw_text` path (generated content, margin boxes) and the
   shaped `draw_glyphs` path. No run is ever drawn with a face different from
   the one it was shaped with.
6. **Share the face table.** `typography.rs` and `pdf.rs` both resolve faces
   through `fonts.rs` — one path table, one byte source per face, so measured
   widths match drawn widths for every face (the existing invariant for
   regular Arial, extended per face).
7. **Embed every used face once.** A document that uses bold, italic, and
   bold-italic embeds all four subset faces; an unused face is not embedded.
   Face identity is deterministic (fixed paths, fixed order).
8. **Stay deterministic.** Face table order is fixed; no hash-order
   dependence; identical input yields byte-identical PDF.
9. **Keep every existing suite green.** Typography, tables, multicol,
   paged-media, and fragmentation acceptance tests pass unchanged (they use
   regular text and must not observe any width change).

## Interfaces

**New module** `engine/src/fonts.rs` — the single source of truth:

```text
FontFace          // the four Arial faces
  Regular | Bold | Italic | BoldItalic

pub fn face_for(weight: f32, style: FontStyle) -> FontFace
pub fn face_path(face: FontFace) -> &'static str
// Regular:    /System/Library/Fonts/Supplemental/Arial.ttf
// Bold:       /System/Library/Fonts/Supplemental/Arial Bold.ttf
// Italic:     /System/Library/Fonts/Supplemental/Arial Italic.ttf
// BoldItalic: /System/Library/Fonts/Supplemental/Arial Bold Italic.ttf
```

`FontFace` derives `Clone, Copy, Debug, PartialEq, Eq` and is usable as an
array index (4 faces, fixed order). It lives in its own leaf module so both
`typography.rs` and `pdf.rs` (and `frag.rs`, for `TextRun`) can depend on it
without cycles.

**`engine/src/css.rs`** — `ComputedStyle` gains:

```text
font_weight: f32,        // computed font-weight; 400 = normal, 700 = bold
font_style: FontStyle,   // engine-owned: Normal | Italic (oblique folds in)
```

`FontStyle` is defined next to the other engine style enums (`Float`,
`Position`, `TextAlign`). In `convert`:

- `font_weight = font.clone_font_weight().value()` (stylo `FontWeight` wraps a
  fixed-point f32; `.value()` returns the float — verified in stylo 0.20
  `values/computed/font.rs`).
- `font_style = match font.clone_font_style() { s if s == StyloFontStyle::NORMAL => Normal, _ => Italic }`
  — import stylo's type as `StyloFontStyle` to avoid the name clash. Stylo
  encodes italic as a distinct fixed-point value (`ITALIC`), `oblique` as an
  angle; anything not `NORMAL` maps to `Italic`.
- `initial()` defaults: `font_weight: 400.0`, `font_style: FontStyle::Normal`.

**`engine/src/typography.rs`** — replace the single `FONT_PATH` / `FONT_BYTES`
/ `SHAPER` statics with a per-face table (e.g. `[LazyLock<FaceData>; 4]`,
`FaceData { bytes: Vec<u8>, font: FontRef<'static>, shaper: ShaperData }`,
indexed by `FontFace as usize`):

```text
pub fn shape_word(word: &str, font_size: Scalar, face: FontFace) -> ShapeRun
```

`break_paragraph(text, max_width, style, hyphenate, justify)` keeps its
signature and derives `face_for(style.font_weight, style.font_style)`
internally; every shaping call in the K-P pipeline uses the derived face.
`ShapeRun` may gain a `face: FontFace` field (informational; the glyphs are
already face-specific) — optional, not required by the acceptance criteria.

**`engine/src/frag.rs`** — `TextRun` gains:

```text
font_face: FontFace,   // resolved at construction; pdf.rs picks the font from it
```

**`engine/src/layout.rs`** (and any other `TextRun` constructor — generated
content in `paged.rs`, table cells, margin boxes): every constructor sets
`font_face: face_for(style.font_weight, style.font_style)` from the owning
element's computed style. Grep for `TextRun {` and cover all sites.

**`engine/src/pdf.rs`** — replace `load_font()` with a per-face lazy loader
(`fn font_for(face: FontFace) -> &'static Font`, four `LazyLock<Font>`
statics). Both the `draw_text` simple path and the `draw_glyphs` shaped path
use `font_for(t.font_face)` instead of the single shared `font`. The unused
`FONT_PATH` const is removed.

**`engine/Cargo.toml`** — no new dependencies.

**CLI**: unchanged.

## Acceptance Criteria

Given/When/Then, each mapping to a real test in `engine/tests/fonts.rs`:

1. **Bold embeds a distinct face** — Given a doc with `.bold { font-weight:
   bold }` and `.plain` text, when rendered, then the PDF bytes contain both
   `ArialMT` and `Arial-BoldMT` (subset tags may prefix the names; substring
   match on the PostScript base name) (`fonts.rs::bold_embeds_bold_face`).
2. **Italic embeds a distinct face** — Given `font-style: italic`, when
   rendered, then the PDF bytes contain `Arial-ItalicMT`
   (`fonts.rs::italic_embeds_italic_face`).
3. **Bold-italic embeds the combined face** — Given `font-weight: bold;
   font-style: italic`, when rendered, then the PDF bytes contain
   `Arial-BoldItalicMT` (`fonts.rs::bold_italic_embeds_combined_face`).
4. **Weight threshold** — Given `font-weight: 400` and `font-weight: 600` on
   equal text, when laid out, then the 600 run renders with the bold face and
   the 400 run with the regular face (`fonts.rs::weight_threshold_at_600`).
5. **Bold measures wider than regular** — Given the same text at the same size
   in `.plain` and `.bold`, when laid out, then the bold run's drawn width
   exceeds the regular run's (real bold glyph metrics, not a synthetic
   stroke) (`fonts.rs::bold_width_exceeds_regular`).
6. **Oblique maps to italic** — Given `font-style: oblique`, when rendered,
   then the PDF bytes contain `Arial-ItalicMT` (`fonts.rs::oblique_uses_italic_face`).
7. **Determinism** — Given a doc using all four faces, when rendered twice,
   then the PDF bytes are identical (`fonts.rs::determinism_four_faces`).
8. **Regression: typography + layout suites** — the existing
   `engine/tests/typography.rs`, `tables*.rs`, `multicol.rs`, and
   fragmentation/paged-media tests pass unchanged (regular runs must measure
   exactly as before) (`fonts.rs::regression_existing_suites` — a thin test
   that renders a known fixture and asserts byte-identity with a pinned
   baseline, plus the full `cargo test` run in CI).

## Edge Cases

- **`font-weight` numeric values between 400 and 700** (500, 600): 600+ maps
  to bold, below maps to regular (Arial has no intermediate cuts).
- **`font-weight: normal` (400) / `bold` (700) keywords** — the computed
  values are 400/700; the rule above covers them. `bolder`/`lighter` resolve
  relative to the inherited weight inside stylo before `.value()` is read.
- **`font-style: oblique 14deg`** — folds into the italic face (documented
  deviation; no oblique cut in the bundle).
- **`font-style: normal` with any weight** — upright face selected by weight.
- **Face file missing on another machine** — same behavior as today's
  single-font `load_font` error: rendering fails with a clear path context
  (the portability/bundling issue will remove this constraint).
- **Text runs that span pages** — the run's `font_face` is carried on the
  `TextRun`, which the break-token resume path re-fragments from source
  offsets; a resumed run shapes with the same face on every page.
- **Mixed faces in one paragraph** (e.g. a bold word inside a regular
  paragraph) — inline styles already produce separate runs per element;
  each run selects its own face. Inter-run kerning across a face change is
  not modeled (consistent with today's per-run shaping).

## References

- CORE-79 triage: `docs/research/` notes + `git log` for CORE-79 commit
  (`docs(demo): second triage — structural diff drivers found; font
  weight/style ignored`). Verification technique (PDF font-list grep,
  line-width measurement): `references/pdf-verification-techniques.md` in the
  `typeanvil-project` skill.
- Stylo 0.20 computed values (verified in the vendored crate):
  `values/computed/font.rs` — `FontWeight(FontWeightFixedPoint)` with
  `.value() -> f32`; `FontStyle(FontStyleFixedPoint)` with `NORMAL`, `ITALIC`,
  `OBLIQUE` constants. Accessors `clone_font_weight()` / `clone_font_style()`
  confirmed in the generated `properties.rs` of the debug build.
- Face files verified present on this machine:
  `/System/Library/Fonts/Supplemental/Arial{,. Bold,. Italic,. Bold Italic}.ttf`.
- Typography-layer spec (CORE-53): shaping/measurement pipeline this issue
  extends; its Non-Goals defer "fontique-based discovery" to a later issue.
- CSS Fonts Module Level 4 (css-fonts-4): `font-weight` numeric mapping,
  `font-style` computed values.
