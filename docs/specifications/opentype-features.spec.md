---
title: OpenType Feature Settings — font-feature-settings and font-variant-*
slug: /specifications/opentype-features
type: spec
status: draft
owner: maintainers
created: 2026-08-24
updated: 2026-09-27
sidebar_position: 14
tags: [engine, css-fonts, typography, opentype, determinism]
spec_id: opentype-features
applies_to: engine 0.x
dependencies: [font-resolution, font-weight-style, typography-layer]
---

# OpenType Feature Settings — font-feature-settings and font-variant-*

## Overview

The engine shapes all text through HarfRust with default feature settings.
Authors cannot turn a feature on or off: `font-feature-settings` is parsed by
stylo but never read by the engine, and `font-variant-*` longhands are ignored.
This blocks small caps, oldstyle numerals, discretionary ligature control, and
every other explicit OpenType feature request.

This spec adds feature control to the shaping path. Stylo already computes the
values; HarfRust already accepts per-shape features; the work is the bridge:
read the computed values in `ComputedStyle`, map them to HarfRust `Feature`
lists, and thread that list from style through line breaking into every
`shape_word` call.

**Verified stylo 0.20 surface (generated `properties.rs`, checked 2026-08-24):**

| Property | Accessor | Computed type |
|---|---|---|
| `font-feature-settings` | `font.clone_font_feature_settings()` | `FontSettings<FeatureTagValue<Integer>>` = `Box<[FeatureTagValue]>` |
| `font-variant-ligatures` | `font.clone_font_variant_ligatures()` | bitflags `u16` (`NORMAL=0`, `NONE=1`, `COMMON_LIGATURES`, `NO_COMMON_LIGATURES`, `DISCRETIONARY_LIGATURES`, …) |
| `font-variant-caps` | `font.clone_font_variant_caps()` | keyword enum, servo values `Normal` / `SmallCaps` only (all-/petite-/unicase/titling are gecko-only) |
| `font-variant-numeric` | `font.clone_font_variant_numeric()` | bitflags `u8` (`LINING_NUMS`, `OLDSTYLE_NUMS`, `PROPORTIONAL_NUMS`, `TABULAR_NUMS`, `DIAGONAL_FRACTIONS`, …) |
| `font-variant-east-asian` | `font.clone_font_variant_east_asian()` | bitflags `u16` |

A feature tag inside stylo's list is packed as one byte per character into a
`u32` (`FeatureTagValue { tag: FontTag(u32), value: Integer(i32) }`), matching
HarfRust's `Tag` layout directly — no string round-trip needed.

**Verified HarfRust 0.13 surface (crate source, checked 2026-08-24):**

- `ShapeOptions::features(&[Feature])` — features applied during shaping,
  full-buffer range when built via `Feature::new(tag, value, ..)`.
- `harfrust::Feature { tag: Tag, value: u32, start: u32, end: u32 }`.
- `Tag::new(b"liga")` packs four bytes big-endian; identical packing to
  stylo's `FontTag`.

Defaults parity context (issue scope item 3): Prince enables standard
ligatures + kerning by default. HarfRust's default plan also enables both
(`liga`, `kern`) for Latin text, so the no-author-CSS behavior should match
already; the defaults probe doc proves it before any control ships.

## Goals

1. Read computed `font-feature-settings` in `ComputedStyle` and expose it as
   an owned, cheap-to-clone feature list.
2. Map `font-variant-ligatures`, `font-variant-caps` (small-caps),
   `font-variant-numeric`, and `font-variant-east-asian` to their canonical
   OpenType tags.
3. Thread the resolved feature list through `break_paragraph` →
   `build_items` → `push_word` → `shape_word` so every shaped box honors it.
4. Determinism: shaping stays a pure function of (text, size, face,
   features); identical input renders byte-identical PDFs.

## Non-Goals

- `font-variant-position` (subscript/superscript synthesis) — separate
  synthesis problem, not a simple tag mapping.
- Petit/unicase/titling caps — not available in stylo's servo build
  (`extra_gecko_values`); recorded here so nobody chases them.
- `font-variation-settings` (variable-font axes) — different mechanism;
  follow-up if a variable face lands in the corpus.
- `@font-face` font-feature-descriptors — author-side defaults per face;
  deferred until a fixture demands them.
- Per-character feature ranges (`Feature.start/end`) — we always apply
  whole-word features.

## Behavior

1. `ComputedStyle` SHALL carry `feature_settings: Vec<(u32, i32)>` (packed
   tag, value) copied from `clone_font_feature_settings()`, plus
   `variant_caps: CapsMode` where `CapsMode ∈ { Normal, SmallCaps }`,
   and bitflag-derived tag lists for ligatures / numeric / east-asian.
2. The engine SHALL resolve these fields into a single ordered
   `Vec<harfrust::Feature>` before shaping: variant-derived tags first
   (deterministic order: ligatures, caps, numeric, east-asian), then
   explicit `font-feature-settings` entries last (css-fonts-4 §10.3:
   low-level settings win).
3. `shape_word` SHALL accept the feature list and pass it via
   `ShapeOptions::features`. All existing call sites pass the paragraph's
   resolved list (words, hyphen glyph, space glue, leader fills).
4. Mapping table (bit → tag, value):
   - ligatures `NONE` → disable `liga`, `clig`, `dlig`, `hlig`, `calt`;
     `NO_COMMON_LIGATURES` → `liga=0 clig=0`; `DISCRETIONARY_LIGATURES` →
     `dlig=1`; plain `COMMON_LIGATURES`/`NORMAL` adds nothing (default-on).
   - caps `SmallCaps` → `smcp=1` (synthesis fallback is out of scope: if the
     face lacks `smcp`, HarfRust ignores the tag, matching HarfBuzz).
   - numeric `LINING_NUMS`→`lnum=1`; `OLDSTYLE_NUMS`→`onum=1`;
     `PROPORTIONAL_NUMS`→`pnum=1`; `TABULAR_NUMS`→`tnum=1`;
     `DIAGONAL_FRACTIONS`→`frac=1`; stacked fractions excluded (gecko-only).
   - east-asian: `JIS78..83/90` → `jp78/jp83/jp90` variants; simplified/
     traditional forms map to `smpl/trad`. Full coverage optional at first
     land; the mapping function is total over the flag bits.
5. Duplicate tags: the LAST entry for a tag wins (HarfBuzz semantics);
   resolution dedupes deterministically by tag with last-wins order.
6. Feature application MUST NOT depend on HashMap iteration or wall time;
   the resolved list order is fully determined by rule 2.

## Interfaces

```rust
// css.rs — ComputedStyle additions
pub feature_settings: Vec<(u32, i32)>,   // packed tag, value (from stylo)
pub ot_features: Vec<(u32, u32)>,        // RESOLVED tag/value pairs incl. font-variant-* maps

// typography.rs
pub fn shape_word(word: &str, font_size: Scalar, face: FaceId,
                  features: &[(u32, u32)]) -> ShapeRun;
// existing 3-arg callers updated; break_paragraph resolves the list once
// from &ComputedStyle and threads it through build_items/push_word.
```

## Acceptance Criteria

- **AC1 — parsing:** a stylesheet with `font-feature-settings: "smcp" 1, "liga" 0`
  yields exactly those tags in `ComputedStyle.feature_settings` with values
  1 and 0. Given/When/Then: render such a doc → inspect computed style via a
  unit test on `cascade` output.
- **AC2 — shaping width changes with liga off:** a word containing an
  fi/fl ligature opportunity measures WIDER with `"liga" 0` than default on
  a face that has ligatures (bundled Arial has `liga`). Test:
  `engine/tests/typography.rs::feature_settings_toggle_changes_width`.
- **AC3 — variant mapping:** `font-variant-numeric: oldstyle-nums` produces
  `onum=1` in the resolved list; `small-caps` produces `smcp=1`. Unit test
  on the resolver.
- **AC4 — determinism:** two renders of the same feature-controlled doc are
  byte-identical. Existing determinism test extended with a feature doc.
- **AC5 — WPT gate:** harness run shows 0 fixed / 0 regressed on the full
  suite (features default off; no test/ref changes).

## Edge Cases

- Empty feature list (the common case) must take the exact current code
  path — zero-cost default, byte-identical output for all existing docs.
- A tag with value 0 disables a default-on feature; value ≥ 2 selects
  alternate index (aalt-style). Pass through verbatim; faces without the
  feature ignore it.
- `font-feature-settings: normal` parses to an empty list in stylo
  (`if_empty = "normal"`); no special casing needed.
- Ligature-off + justification: K-P widths come from the same shaped boxes,
  so glue math automatically uses the wider un-ligated width. No separate
  handling.
- Small-caps text extraction: lowercase source letters map to smcp glyphs;
  ToUnicode still carries the original codepoints via cluster ranges
  (unchanged mechanism).

## References

- Blocked-by relationship satisfied by `font-resolution.spec.md`
  (custom fonts landed).
- css-fonts-4 §10 (font-feature-settings) and §6 (font-variant-*).
- HarfRust 0.13 `ShapeOptions::features` API (verified in crate source).
- stylo 0.20 generated accessors (verified in this worktree's
  `target/debug/build/stylo-*/out/properties.rs`).
