---
title: Render Report — Layout Diagnostics Mapped to Source
type: spec
status: approved
owner: maintainers
created: 2026-10-25
updated: 2026-10-25
slug: /specifications/render-report
sidebar_position: 44
tags: [cli, diagnostics, layout, ai-native]
spec_id: render-report
issue_id: CORE-571
applies_to:
  - engine/src/render_report.rs
  - engine/src/diagnostics.rs
  - engine/src/dom.rs
  - engine/src/main.rs
dependencies:
  - diagnostics
---

# Render Report — Layout Diagnostics Mapped to Source

## Overview

`--diagnostics json` (spec `diagnostics`) reports the stylesheet surface only.
An agent that gets a PDF back cannot tell that a box overflowed on page 3.
This spec extends the same channel with **layout-outcome events**. Each event
carries a stable code, a page number, a rect in points, and a source pointer:
the HTML line and column of the responsible element plus a selector that
matches it. The document also gains a render **summary**.

## Goals

1. One schema-versioned JSON document holds the CSS events and the layout
   events. The schema version is `2`.
2. Every layout event points at source: page, rect, HTML line/column, element
   selector.
3. Determinism: identical input gives a byte-identical document.
4. Zero cost when `--diagnostics` is absent: no extra work, PDF bytes unchanged.

## Non-Goals

- The CSS rule (stylesheet line) responsible for a layout event. The report
  names the ELEMENT (as a selector that matches it) and its HTML position.
  Cascade provenance per property needs stylo rule tracing; deferred.
- Events for content the engine silently drops for reasons other than the
  classes below (for example unsupported properties already reported by the
  CSS scanner).
- Harness integration.

## Behavior

1. The engine SHALL expose `render_report::analyze_layout(dom, stylesheet,
   layout) -> RenderReport`. It SHALL be a pure read of the FINAL layout pass,
   the DOM, and a fresh cascade. It SHALL NOT change the layout.
2. `Dom::parse` SHALL record each element's 1-based start-tag `(line, column)`
   (column in characters). Elements the parser synthesizes (implied
   `html`/`head`/`body`, anonymous table boxes) report `(0, 0)`.
3. Each layout event SHALL carry: `code`, `severity` (`warning`), `message`,
   `line`, `column`, `source: "layout"`, `page` (1-based; an inline
   element with no box of its own takes the page of its nearest ancestor box,
   with no `rect`; `0` when no ancestor box exists either), `rect` (`[x, y, width, height]` in points, page
   top-left origin, two-decimal precision, or `null`), `element` (selector
   `tag#id.class…`, empty for page-level events), and optional `hint`.
4. Event classes and the seam that emits each (all in
   `engine/src/render_report.rs`, reading `Layout` after pagination):

   | Code | Condition | Seam |
   |---|---|---|
   | `layout-overflow` | A sourced box's border box is wider or taller than its page's content area by more than 0.5pt. Only the deepest such box per subtree is reported, once per element per page. Repeated table chrome is skipped. | fragment walk per page (`overflow_walk`) |
   | `empty-page` | A page has no fragment with paint (`FragmentContent` other than `None`). | per-page content flag |
   | `orphaned-heading` | The last painted in-flow content on a page belongs to an `h1`–`h6`, and the next page has content. Hint: `add \`break-after: avoid\` to <selector>`. | last painted fragment per page (`orphaned_heading`) |
   | `table-header-only` | A table fragment on a page holds repeated header rows and no body rows. | rows per table per page (`table_events`) |
   | `table-single-row-fragment` | A table that spans pages has exactly one body row on a page. | same |
   | `image-missing` | An `<img>` laid out as a broken placeholder (`ImageRun.broken`). | image fragment |
   | `font-fallback` | An element with direct text whose first `font-family` is a named family that resolves to no face. Message names the requested family and the resolved PostScript face. Reported once per family. | `fonts::family_available` |
   | `target-unresolved` | A `target-counter()`/`target-text()` whose `attr()` value names no element id. | computed `content` |
   | `link-target-missing` | An `<a href="#id">` whose id does not exist. | DOM |

5. Event order SHALL be: CSS / Markdown / bookmark events first (unchanged),
   then layout events sorted stably by page (page `0` last). Within a page the
   order is overflow and table events in fragment pre-order, then page events,
   then element events in DOM order.
6. The summary SHALL hold `pages` (final page count), `passes` (pagination
   passes run, `Layout::passes`), and `font_bundle` (`<regular PostScript
   name>@<FNV-1a 64 hex of the four bundled face files>`).
7. `--diagnostics text` SHALL print layout events after the CSS events, one
   per line on stderr: `LINE:COL warning [code]: message (page N, selector)`.
8. Without `--diagnostics` nothing here SHALL run.

## JSON Schema (version 2)

```json
{
  "schema": 2,
  "diagnostics": [
    {"code": "unsupported-property", "severity": "warning", "message": "…", "line": 3, "column": 5},
    {"code": "layout-overflow", "severity": "warning", "message": "<div> overflows page 1: width 900pt exceeds the 468pt content width", "line": 7, "column": 1, "source": "layout", "page": 1, "rect": [72, 90.4, 900, 20], "element": "div.wide"}
  ],
  "counts": {"warnings": 2},
  "summary": {"pages": 1, "passes": 1, "font_bundle": "ArialMT@0123456789abcdef"}
}
```

Changes from schema 1: `schema` is `2`; layout events add the fields above;
`summary` is new. Stylesheet event objects are byte-identical to schema 1.

## Acceptance Criteria

Tests live in `engine/tests/it/render_report.rs`.

1. Given an over-wide block on line N, Then `layout-overflow` reports page 1,
   line N, the start-tag column, and its selector (`overflow_event`).
2. Given a forced break that yields a blank page, Then `empty-page` reports it
   (`empty_page_event`).
3. Given an `h2` that ends page 1, Then `orphaned-heading` reports page 1,
   its line, and the `break-after: avoid` hint (`orphaned_heading_event`).
4. Given a table split with a repeated header and one body row on the last
   page, Then `table-single-row-fragment` reports that page
   (`table_single_row_event`); given a header-only fragment, Then
   `table-header-only` (`table_header_only_event`).
5. Given a missing image, an unknown family, a dangling `target-counter`, and
   a dangling `#link`, Then each emits its code with the element's line
   (`image_font_target_link_events`).
6. Summary page/pass counts are correct for a `counter(pages)` document
   (`summary_counts_passes`).
7. Two CLI runs of the same input give byte-identical stdout
   (`cli_json_is_deterministic`).
8. The CSS-only document is unchanged apart from `"schema": 2`
   (`engine/tests/it/diagnostics.rs`).

## Edge Cases

- Synthesized elements: `(0, 0)` position; the event still carries page/rect.
- Two same-named tags on one line get distinct columns (claimed in order).
- A box both wide and tall reports one event with both sizes.
- Fixed-position boxes cloned onto every page report per page they overflow.

## References

- `docs/specifications/diagnostics.spec.md` — schema 1, CSS events.
- CORE-570 (AI-native epic), CORE-571.
