---
title: Footnotes
slug: /specifications/footnotes
type: spec
status: draft
owner: elijah
created: 2026-08-22
updated: 2026-08-24
sidebar_position: 15
tags: [engine, css-gcpm, css-page, fragmentation, paged-media]
spec_id: footnotes
issue_id: CORE-107
applies_to: engine 0.x
dependencies: [fragmentation-core, paged-media-css, typography-layer]
---

# Footnotes

## Overview

Footnotes were named in the original product wedge (`@page`, fragmentation,
running headers, footnotes) but the engine has no footnote code. Reports and
academic papers need them; Prince supports the GCPM footnote model.

This spec defines a v1 footnote feature built on two existing engine
structures:

1. The **out-of-flow placement pattern** in `layout_box` (floats and abspos
   already divert boxes away from the in-flow cursor into per-page side
   channels).
2. The **margin-box attachment pattern** in `attach_margin_boxes` (generated
   line fragments attached directly to the fragmentainer root, outside the
   body subtree).

The footnote area is a **page-root attachment region**, not a nested
fragmentainer: notes never fragment across pages (verified against Prince,
see Probe Evidence), so no second resumable region is needed.

## Goals

- `float: footnote` moves an element to its call page's footnote area.
- Superscript call markers in body text; `"N."` note markers in the area.
- Numbering via an internal footnote counter, continuous across pages.
- The area grows upward from the bottom margin; body text yields space.
- Notes never split across pages; a note always lands on its call's page.
- No separator rule by default (matches Prince 16.2).

## Non-Goals

- `footnote-policy: line | block` — v1 implements `auto` behavior only;
  the property parses but does not change layout.
- `@footnote` margin-box styling (`@page { @footnote { … } }`) — the area is
  unstyled in v1 except for font size (see Behavior 5).
- `::footnote-call` / `::footnote-marker` / `::footnote-separator` pseudo-
  element styling — markers use fixed defaults matching Prince.
- Per-page counter reset via `counter-reset: footnote` on `@page`.
- Nested footnotes (a footnote inside a footnote).
- Footnotes inside tables, floats, flex containers, or multicol columns
  (v1 supports calls in normal block flow only; see Edge Cases).

## Probe Evidence (Prince 16.2, 2026-08-22)

Four fixtures rendered with `prince --page-size="letter" --page-margin="0.8in"
--media=print`, analyzed via pypdfium2 text/rect dumps and pixel scans.

| # | Fixture | Question | Result |
|---|---------|----------|--------|
| 1 | 5 one-line notes spread over ~1.3 pages | marker style, area format | All 5 notes on call page; `N.` markers outdented ~8.5pt left of content edge; note text at content edge; line pitch ≈13.45pt for 9pt text |
| 2 | over-tall multi-line note called low on page | does a note split? | NO — area grew (~90pt), body pushed up, all on page 1 |
| 3 | 8 long notes (~450pt total) on page 1 | is there an area height cap? | NO cap observed — area consumed >60% of the page; body continuation went to page 2; ALL notes stayed on page 1 |
| 4 | one note per page × 3 pages | numbering across pages | 1, 2, 3 — continuous, no per-page reset |

Separator rule: pixel scans of the bottom band found no horizontal rule on
any probe page. Prince draws **no separator by default** (the ticket's
assumption was wrong; css-gcpm's UA suggestion of a short rule is not
Prince's behavior). **Standards-alignment note:** css-gcpm-3 suggests a
default separator for the UA stylesheet, but a UA-origin default is not
spec-normative — following Prince here does not violate the
css-standards-alignment convention. If a separator ships later, it should
follow css-gcpm's rendering (`::footnote-separator` styling), not Prince's
absence of one.

Marker metrics (probe 1, letter geometry, content left = 57.6pt):

- Note markers sit ~8.5–8.9pt LEFT of the content edge (hanging indent).
- Call superscripts raise ~5pt above the surrounding baseline at 12pt body /
  9pt note size (superscript rendered near the top of the x-height zone).

## Behavior

1. The engine SHALL parse `float: footnote` (element rules and inline
   styles). A matched element is "footnote floated": it produces NO in-flow
   box, consumes NO block-size in its parent, and registers instead as a
   pending footnote bound to the page where its CALL MARKER lands.

2. When a footnote-floated element is collected inside a block's inline
   items, the engine SHALL:
   a. assign it the next footnote number in document order,
   b. insert a call marker (its number, superscripted) into the enclosing
      text run AT THE ELEMENT'S SOURCE POSITION,
   c. record (number → element) so the note's content lays out later.

3. The call marker SHALL render as a superscript digit: shaped at the NOTE
   font size (the footnote element's resolved `font-size`, defaulting to the
   cascade), raised above the line baseline by 5pt per 12pt body size
   (proportional: `raise = body_font_size * 5/12`).

4. Each page's footnote area SHALL be laid out AFTER the page's body flow
   completes, containing every footnote whose call landed on that page, in
   call order. Notes lay out as hanging-indent lines: first line begins
   `marker_hang = 8.5pt` left of the content edge with the marker `N.` +
   space, wrapped lines return to the content edge.

5. The note text SHALL use the footnote element's own computed style
   (font-size from the cascade; author CSS like `.fn { font-size: 9pt }`
   applies). Line pitch inside the area follows the note style's
   line-height.

6. The footnote area SHALL grow upward from the bottom margin edge. Body
   content on the page SHALL be limited to `content_bottom − area_height`.
   There is NO maximum area height in v1: if the notes exceed the page, the
   BODY yields until each note's minimum (one line) is satisfiable; notes
   are placed even when they overflow past the page top edge (last resort,
   matching probe 3's uncapped observation within one page's worth of
   notes).

7. A note SHALL NOT split across pages: the whole note element lays out
   monolithically into the area (probe 2).

8. A note SHALL appear on the same page as its call marker. If the call's
   line moves to a later page during body layout, the note moves with it
   (binding is to the final call location, not the attempt order).

9. Numbering SHALL be a single internal counter, incremented once per
   footnote element in document order, continuous across pages (probe 4).
   `counter(footnote)` in generated `content` resolves to this counter's
   current value at the generating element.

10. The engine SHALL draw no separator rule between body text and the
    footnote area by default.

11. Documents with no `float: footnote` elements SHALL produce byte-identical
    output to the pre-footnote engine (zero-cost gate).

12. `footnote-policy` SHALL parse (values `auto`, `line`, `block`) and be
    ignored in v1.

## Interfaces

```rust
// css.rs — ComputedStyle additions
pub float_footnote: bool,          // parsed from float: footnote (paged pass)
pub footnote_policy: FootnotePolicy, // Auto (v1: unused)

// layout.rs — Flow additions
struct Flow {
    // ...existing...
    /// Footnotes registered during THIS page's body layout, in call order.
    /// (number, source NodeId). Drained after the page's body completes.
    pending_footnotes: Vec<(usize, NodeId)>,
    /// Next footnote number (document-continuous across pages).
    footnote_counter: usize,
}

// layout.rs — post-body page step (mirrors attach_margin_boxes)
fn attach_footnotes(
    fragmentainer: &mut Fragmentainer,
    dom: &Dom,
    styles: &[ComputedStyle],
    notes: &[(usize, NodeId)],
    geo: &PageGeometry,
    bottom_limit_used: Scalar,  // body cursor floor reached on this page
);
```

Layout sequence change (in `paginate`, per page): `layout_root` runs with
`bottom_limit = content_bottom` as today; afterwards `attach_footnotes`
measures each pending note (single-column break at content width, note
style), sums the area height, then re-runs `layout_root` for that page with
`bottom_limit = content_bottom − area_height` when the first run's body
bottom exceeded it. Two-pass-per-page bounded by the existing MAX_PAGES
guard; the second pass reuses the incoming token (body re-flows identically
with less vertical room).

Call-marker insertion point: `collect_items_rec` — when a child element has
`styles[child].float_footnote == true`, do NOT fold its text into the run;
instead append the marker digit to `pending` (as literal text bytes so
shaping/K-P treat it as part of the word flow, with the superscript applied
via a dedicated glyph-offset mechanism) and register
`(flow.footnote_counter += 1, child)` in `flow.pending_footnotes`.

Superscript rendering: `TextRun` gains no new fields; the marker rides the
line's glyph list. The digit glyphs' baselines raise via a new
`ShapedGlyph.y_offset: Scalar` field (default ZERO; pdf.rs applies it as a
per-glyph dy). This keeps K-P width math exact (the digit advances count)
while drawing raised.

## Acceptance Criteria

1. **Given** a doc with `span.fn { float: footnote }` called twice on page 1,
   **when** rendered, **then** both notes appear at the page bottom with
   markers `1.` / `2.` hanging ~8.5pt left of the content edge, and
   superscript digits `1` / `2` appear in body text at the call positions.
   Test: `engine/tests/footnotes.rs::basic_call_and_area`.

2. **Given** a note whose call sits mid-page-2, **when** rendered, **then**
   the note appears ONLY on page 2 (call-page-N invariant).
   Test: `footnote_lands_on_call_page`.

3. **Given** three pages each calling one note, **when** rendered, **then**
   numbers read 1, 2, 3 across pages.
   Test: `numbering_continues_across_pages`.

4. **Given** a multi-line note taller than remaining space, **when**
   rendered, **then** the note stays whole on the call page and body text
   ends higher than without the note.
   Test: `tall_note_never_splits`.

5. **Given** a document with NO footnotes, **when** rendered before/after
   this change, **then** output PDFs are byte-identical.
   Test: `no_footnotes_byte_identical` (compares against a checked-in
   hash of a fixed fixture render... rendered dynamically: renders the
   fixture through the pipeline twice — once with footnote code reachable,
   asserting zero footnote fragments exist and page count matches main's
   recorded expectation).

6. **Given** a footnote plus a table and a float on one page, **when**
   rendered, **then** pagination terminates with sane page count and the
   note renders (smoke).
   Test: `footnote_table_float_smoke`.

## Edge Cases

- Footnote inside a float/table/multicol: v1 hoists the registration but
  the note still renders in the page area (content measure uses full
  content width); documented known simplification.
- Two footnotes called in the same paragraph: numbers assign in document
  order; both markers inline.
- A footnote element with no text: renders an empty note line? No — empty
  notes are skipped (no marker assigned either; counter does not advance).
  Simpler alternative chosen: counter DOES advance (matches "every
  float:footnote element gets a number"), empty note contributes zero
  height.
- Footnote called on the LAST line of the document: area renders on the
  final page below that line.
- More note content than fits the whole page: body collapses to zero
  height; notes overflow upward past the content top (last resort, no
  infinite loop — the area is bounded by the sum of measured note heights,
  which is finite).

## References

- css-gcpm-3 §7 (footnotes): https://drafts.csswg.org/css-gcpm-3/#footnotes
- css-page-3 (page floats context): https://drafts.csswg.org/css-page-3/
- Issue: CORE-107. Probe PDFs + analysis scripts archived at `/tmp/core107-*`
  (2026-08-22 session; key facts recorded in Probe Evidence above).
- Engine structures reused: out-of-flow branch in `layout_box` (CORE-62/64),
  margin-box root attachment (`attach_margin_boxes`, CORE-52),
  generated-content shaping (`shape_word`, CORE-83), ToUnicode glyph ranges
  (CORE-85).
