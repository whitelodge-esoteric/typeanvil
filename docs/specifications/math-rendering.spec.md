---
title: Math rendering (MathML Core)
type: spec
status: in-review
owner: maintainers
created: 2026-10-16
updated: 2026-10-16
slug: /specifications/math-rendering
sidebar_position: 70
tags: [math, mathml, layout, typography, tagged-pdf]
spec_id: math-rendering
issue_id: CORE-363
applies_to:
  - engine/src/math.rs
  - engine/src/layout.rs
  - engine/src/css.rs
  - engine/src/tags.rs
dependencies:
  - tagged-pdf
  - typography-layer
---

# Math rendering (MathML Core)

## Overview

The engine shall typeset MathML Core (W3C, as shipped by Chromium and Firefox)
written inline in HTML. A `<math>` element becomes one monolithic box. Its
content is laid out by `engine/src/math.rs` and emitted as ordinary text and
rule fragments, so the PDF text layer, ToUnicode map and tagged-PDF paths work
without new emitter code.

Path chosen: hand-rolled layout in `math.rs`, driven by the DOM. Stylo needs no
new property for the first slice: `math` is a UA-styled element
(`display: inline-block`, or `block` with `display="block"`).

## Goals

- Tokens, `mrow`, scripts, fractions, radicals, under/over and tables lay out
  as in MathML Core.
- Inline math sits on the text baseline. Display math is a centered block.
- Copy and paste yields the math text. Equations are `Formula` structure
  elements with alt text.

## Non-Goals

- TeX input. Decision (2026-10-16, agent): TeX is not parsed by the engine.
  CORE-305 (Markdown input) shall convert `$...$` and `\[...\]` to MathML in
  its own preprocessing step and hand the engine MathML Core. This keeps one
  canonical layout input and no JS runtime (CORE-132 stays parked).
- OpenType MATH table parsing. `read-fonts` 0.43 exposes no MATH API. The
  first slice uses fixed TeX-like constants (below). A later ticket may read
  the table.
- Glyph assembly for stretchy operators. Stretchy delimiters and radicals
  scale the single glyph.
- `mpadded`, `menclose`, `mmultiscripts`, `mglyph`, linebreaking inside math,
  `mathvariant` other than the automatic italic of single-character `mi`.
- Equation fragmentation: an equation is monolithic.

## Behavior

1. **Input.** The engine shall lay out the MathML elements `math mi mn mo mtext
   mspace mrow mfrac msqrt mroot msub msup msubsup munder mover munderover
   mtable mtr mtd mstyle semantics`. Any other element inside `math` shall lay
   out as `mrow`. `annotation` and `annotation-xml` shall be skipped.
2. **Box.** A `math` element with `display="block"` shall be a block box. Its
   content shall be centered in the available width. Any other `math` shall be
   an atomic inline box whose baseline is the math baseline.
3. **Fonts.** The math face shall be the installed `STIX Two Math` face. When
   it is missing, the element's own font face shall be used. License: STIX Two
   is SIL OFL 1.1; the engine reads the system copy and does not bundle it.
4. **Style.** `math-style` and `math-depth` follow MathML Core. Display style
   starts true for `display="block"`. Script size is 0.71 of the parent size
   per level, with a floor of 0.5.
5. **Tokens.** `mn`, `mtext` and multi-character `mi` shall be upright. A
   single-character `mi` shall map letters to the Mathematical Italic block
   (Latin and lowercase Greek). `mo` `-` shall render as U+2212.
6. **Operator spacing.** `mo` shall take its form (prefix, infix, postfix) from
   its position in the parent `mrow`. Relations get 0.2778em on each side,
   binary operators 0.2222em, `,` and `;` get 0.1667em after. `lspace` and
   `rspace` attributes in `em` override.
7. **Scripts.** `msub`, `msup` and `msubsup` shall place scripts at the shifts
   in the constants table, in script size, after the base.
8. **Fractions.** `mfrac` shall stack numerator and denominator on the math
   axis, separated by a rule of the rule thickness (`linethickness` attribute
   `0` removes the rule).
9. **Radicals.** `msqrt` and `mroot` shall draw the radical sign glyph scaled
   to the content height and an overbar rule joined to it. `mroot` shall place
   the index at script-script size left of the sign.
10. **Under/over.** `munder`, `mover` and `munderover` shall center the limits
    over or under the base. Large operators (`∑ ∏ ⋃ ⋂`) shall draw at 1.4 times
    size in display style.
11. **Tables.** `mtable` shall size columns to the widest cell, rows to the
    tallest cell, with a 0.8em column gap and 0.4em row gap, and center the
    table on the axis. Cells are centered.
12. **Stretchy.** A stretchy `mo` (`( ) [ ] { } |`) in an `mrow` shall scale to
    cover the other children when they exceed 1.2em in height.
13. **PDF.** Token text shall be shaped runs with source ranges, so ToUnicode
    carries the math text. Fraction bars and radical overbars shall be filled
    rectangles in the text color.
14. **Tagged PDF.** With `--tagged`, a `math` element shall map to a `Formula`
    structure element. Alt text is the `alttext` attribute, else the element's
    text content.
15. **Numbering.** Equation numbers use CSS counters on the author's
    containing element. No engine support is needed.

### Constants (em of the current size, STIX Two Math approximations)

| Name | Value |
| --- | --- |
| axis height | 0.25 |
| rule thickness | 0.06 |
| superscript shift up | 0.363 |
| subscript shift down | 0.247 |
| fraction numerator shift (display / text) | 0.677 / 0.394 |
| fraction denominator shift (display / text) | 0.686 / 0.345 |
| fraction gap (display / text) | 3 rule / 1 rule |
| radical gap (display / text) | 0.15 / 0.05 |
| upper/lower limit gap | 0.11 |
| script space after | 0.056 |

## Interfaces

```rust
// engine/src/math.rs
pub fn is_math(dom: &Dom, id: NodeId) -> bool;
pub fn layout_math(dom: &Dom, styles: &[ComputedStyle], id: NodeId) -> MathBox;
pub struct MathBox { pub width: f64, pub ascent: f64, pub descent: f64, pub ops: Vec<MathOp> }
```

## Acceptance Criteria

1. Given the repro `probe/mathml.html`, the PDF text layer contains `E`, `m`,
   `c` and a separate `2` at a baseline above the `c` baseline.
   Test: `engine/tests/core363_mathml.rs::superscript_is_raised`.
2. Given an `mfrac`, the numerator baseline is above and the denominator
   baseline below the fraction rule, and a filled rule rectangle exists.
   Test: `fraction_is_stacked_with_rule`.
3. Given an `msqrt`, a radical sign glyph and an overbar rule exist.
   Test: `radical_has_sign_and_bar`.
4. Given a `<math>` inside a paragraph, the surrounding text is not displaced
   onto the math baseline. Test: `inline_math_sits_on_baseline`.
5. Given `--tagged`, the structure tree holds a `Formula` with alt text.
   Test: `tagged_formula_has_alt`.
6. A demo showcase fixture with real equations renders (AC4):
   `demo/showcase/math.html`, listed in `demo/showcase/manifest.json`.

## Chromium comparison (ticket AC2)

Measured from the PDFs of `probe/mathml.html` (Chromium print-to-PDF vs
TypeAnvil, pypdfium2 char boxes and path bounds). Tolerance: 3.5 pt on
vertical offsets, 0.1 pt on rule thickness.

| Quantity (pt)                                  | Chromium | TypeAnvil | Delta |
|------------------------------------------------|---------:|----------:|------:|
| Superscript bottom above "=" bottom            |     3.83 |      1.87 |  1.96 |
| Fraction rule thickness                        |     0.75 |      0.72 |  0.03 |
| Fraction rule bottom vs display "=" mid        |    -0.83 |     -1.38 |  0.55 |
| Numerator "4" bottom above rule top            |     2.25 |      4.76 |  2.51 |
| Denominator "2" bottom below rule              |     9.75 |     10.88 |  1.13 |
| Radical overbar bottom above fraction rule     |    14.25 |     17.55 |  3.30 |

Every row is inside the tolerance. The full table is in
`~/workspace/typeanvil-evidence/CORE-363/chromium-comparison.md`.

## Known limitations

- The WPT `mathml/` directory is not in the sparse checkout
  (`harness/wpt_fetch.py` `TEST_PATHS`). Selecting and baselining it (ticket
  AC3) is split to CORE-455.
- Text after an inline equation wraps to a new line. The same happens for any
  inline-block on the base binary (CORE-454).

## Edge Cases

- A `math` without children lays out as zero size.
- A missing math font falls back to the element face; glyph fallback
  (CORE-386) covers missing code points.

## References

- MathML Core: https://www.w3.org/TR/mathml-core/
- CORE-363, CORE-305 (Markdown input), CORE-111 (tagged PDF), CORE-113.
