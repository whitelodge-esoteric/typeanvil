---
title: List Markers
type: spec
status: approved
owner: maintainers
created: 2026-10-16
updated: 2026-10-16
slug: /specifications/list-markers
sidebar_position: 60
tags: [layout, css-lists, markers]
spec_id: list-markers
issue_id: CORE-385
applies_to:
  - engine/src/layout/lists.rs
  - engine/src/layout.rs
  - engine/src/css.rs
dependencies:
  - tagged-pdf
---

# List Markers

## Overview

Before CORE-385 every `<li>` rendered without a bullet or number. This spec
defines the marker the engine paints for list items (css-lists-3).

Stylo path (verified 2026-10-16): the servo build of stylo 0.20 has no
`list-style-type` longhand. The hand-rolled paged pass in `css.rs` parses
`list-style-type` and the type keyword inside `list-style`, and records it in
`ComputedStyle::list_style_type`. Stylo still computes `display` for `li`
(UA rule `li { display: block }`), so `display: list-item` is not needed.

## Goals / Non-Goals

Goals: `list-style-type` (disc, circle, square, decimal,
decimal-leading-zero, lower/upper-alpha, lower/upper-latin,
lower/upper-roman, none); `<ol start>`, `<ol reversed>`, `<ol type>`,
`<li value>`; outside marker position; marker in the text layer.

Non-Goals: `list-style-position: inside`, `list-style-image`, `::marker`
styling, `counter(list-item)`. These stay unsupported and are tracked as
follow-ups.

## Behavior

1. An `li` with `display: block` shall paint a marker on the first fragment
   of its box.
2. The effective type shall be the element's own declaration, else the
   nearest ancestor's declaration, else the UA default: `decimal` in an `ol`
   (`type` attribute `a A i I` maps to alpha/roman), else `disc`, `circle`,
   `square` by `ul`/`ol` nesting depth 1, 2, 3+.
3. The ordinal shall follow HTML 4.9.8: `start` (default 1, or the item
   count when `reversed`), step +1 (-1 when `reversed`), and `li value`
   resets the counter.
4. Marker text shall be `<number>. ` for numeric and alphabetic types, and a
   glyph followed by a space for `disc` (U+2022), `circle` (U+25E6) and
   `square` (U+25AA). `none` paints nothing.
5. The marker is an outside marker: its right edge shall sit at the item's
   content-box left edge, and it shall not change any line width.
6. The marker shall be a shaped text run, so it reaches the PDF text layer
   with a ToUnicode map. It shares the first text baseline of the item.
7. The marker uses the item's font face, size and colour.

## Acceptance Criteria

Tests in `engine/tests/core385_list_markers.rs`.

- AC-1: `<ol><li>one</li><li>two</li></ol><ul><li>a</li></ul>` paints runs
  `1. `, `2. ` and `• ` (`ol_and_ul_paint_markers`).
- AC-2: `start`, `reversed` and `value` set the ordinals
  (`ol_start_reversed_and_value`).
- AC-3: `list-style-type` keywords and `list-style: none` apply
  (`list_style_type_keywords`).
- AC-4: the marker x is left of the body text x
  (`outside_marker_hangs_left_of_content_edge`).

## Edge Cases

- A marker whose item has no text lines is placed at the content top with the
  item's own baseline offset.
- Roman numerals above 3999 and alpha values below 1 fall back to decimal.
- Tagged mode: the marker run is owned by the `li` fragment. Moving it into
  `Lbl` is tracked by CORE-374.

## References

- CORE-385, CORE-374, `docs/specifications/tagged-pdf.spec.md`.
- css-lists-3, HTML Standard 4.9.8 (`ol`).
