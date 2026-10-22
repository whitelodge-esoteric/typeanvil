---
title: Inline element font face and colour
type: spec
status: approved
owner: maintainers
created: 2026-10-20
updated: 2026-10-22
slug: /specifications/inline-style
sidebar_position: 61
tags: [typography, inline, color, font-weight]
spec_id: inline-style
issue_id: CORE-530
applies_to:
  - engine/src/layout.rs
  - engine/src/layout/multicol.rs
  - engine/src/typography.rs
  - engine/src/pdf.rs
  - engine/src/css.rs
dependencies:
  - font-weight-style
---

# Inline element font face and colour

## Overview

An inline element such as `span`, `b`, `strong` or `em` can set its own
`color`, `font-weight` and `font-style`. Before CORE-530, the engine shaped and
painted a whole text run with the parent block's style. The inline element's own
values were computed and then ignored.

## Goals

- The text of an inline element paints in the element's own `color`.
- The text of an inline element uses the font face the element's own
  `font-weight`, `font-style` and `font-family` resolve to.
- `b`, `strong` compute `font-weight: bold`. `i`, `em`, `cite`, `dfn`, `var`
  compute `font-style: italic`.

## Non-Goals

- `font-size` and `line-height` on an inline element. The line box uses the
  block's metrics. Tracked separately.
- `text-decoration` on an inline element.
- Inline border painting, and inline padding in the background box (CORE-543
  paints the background only).

## Behavior

1. The engine shall record, for each inline element whose face or colour
   differs from its parent's, a span of byte offsets in the run text.
2. A span shall nest. The innermost span that covers a byte wins.
3. The line breaker shall shape each piece of a word with the face of the span
   that covers it. A style change inside a word shall not create a break
   opportunity.
4. Each shaped glyph shall carry an optional colour. The PDF writer shall paint
   the glyph in that colour, and in the run colour when none is set.
5. Every path that re-breaks a text run from a source offset (floats, atomic
   lines, multicolumn) shall shift the spans by the same offset.
6. Measurement paths (`measure_block`, intrinsic widths, multicolumn balance)
   shall use the same spans as layout, so measured and laid-out widths agree.

7. (CORE-543) An inline element with its own `background-color` shall record
   it in its style span. Each glyph shall carry the innermost declared
   background. The PDF writer shall fill the glyph run's content area (font
   ascent plus descent) with that colour before it paints the glyphs. A space
   between two words with the same background shall carry it too.

## Acceptance Criteria

1. Given `<p>plain <span style="color:#f00;font-weight:bold">x</span></p>`,
   when laid out, then the glyphs of `x` use the bold face and carry the red
   colour, and the glyphs of `plain` carry neither. Test:
   `core530_inline_style::span_color_and_weight_apply`.
2. Given the same styles from a class, a descendant selector or an inline
   `style` attribute, then the result is the same. Test:
   `core530_inline_style::inline_style_attribute_and_descendant_selector`.
3. Given `<b>`, `<strong>` and `<em>`, then each uses a non-run face. Test:
   `core530_inline_style::b_and_strong_are_bold_and_em_is_italic`.
4. Given a style change inside one word and a nested inline element, then each
   piece uses its own style. Test:
   `core530_inline_style::mid_word_inline_and_nested_inline`.
5. Given `<p>plain <span style="background-color:#000">boxed text</span>
   after</p>`, then the glyphs of `boxed` and `text` carry the black
   background and the others carry none. Test:
   `core543_inline_background::span_background_reaches_its_glyphs_only`.
6. Given `#t p { background-color:#000; display:inline }` on paragraphs in a
   section, then every glyph carries the black background. Test:
   `core543_inline_background::display_inline_paragraphs_paint_background`.

## References

- CORE-530.
- WHATWG HTML section 15.3.1 and 15.3.2 (rendering defaults).
- `docs/specifications/font-weight-style.spec.md`.
