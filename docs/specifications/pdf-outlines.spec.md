---
title: PDF Outlines and Bookmarks
slug: /specifications/pdf-outlines
type: spec
status: in-review
owner: maintainers
created: 2026-09-03
updated: 2026-09-27
sidebar_position: 21
tags: [engine, pdf, outlines, bookmarks, gcpm]
spec_id: pdf-outlines
applies_to: engine 0.x
dependencies: [paged-media-css, tagged-pdf, diagnostics]
---

# PDF Outlines and Bookmarks

## Overview

PDF bookmarks (the outline tree) are the most buyer-visible navigation feature
after hyperlinks. An initial cut shipped first: `h1`–`h6` elements are collected
in document order and nested by heading level in `pdf.rs::build_outline`.
That covers the default case only. This spec completes Prince parity per
css-gcpm-3 §Bookmarks: the `bookmark-level`, `bookmark-label`, and
`bookmark-state` properties, a correct UA default mapping, per-heading
destinations at the heading's y-offset, and a diagnostics event for anchors
that resolve to no destination.

Verified krilla facts (krilla 0.8.2, checked 2026-09-03):

- `krilla::outline::Outline` holds top-level `OutlineNode`s;
  `OutlineNode::new(text, XyzDestination)`, `.with_open(bool)` (default
  `false` = collapsed), `.push_child` for nesting.
- `XyzDestination::new(page_index, point)` — point is in the engine's
  top-left-origin page coordinates; krilla converts to PDF coordinates
  itself (`destination.rs` inverts y around the page height). A page index
  out of range panics at export — never emit one.
- `with_open(true)` writes a positive `/Count` (children visible);
  `false` (default) writes a negative `/Count`. This maps `bookmark-state`
  directly.

## Goals

1. css-gcpm-3 `bookmark-level` / `bookmark-label` / `bookmark-state` parsed
   and honored.
2. UA default that mirrors Prince's html.css: `h1`–`h6` → levels 1–6 with
   label `contents()`; `bookmark-level: none` removes a box from the outline.
3. Destinations anchored at the heading's first fragment (page + y), not the
   page top.
4. Diagnostics event when a `bookmark-*` declaration resolves to no
   destination.
5. Determinism: document-order collection, no hash iteration.

## Non-Goals

- `bookmark-state` values other than `open`/`closed` (css-gcpm has only these).
- Outline entry color/style (PDF 1.4 `/C` + `/F` — Prince does not expose it).
- `content()` inside `bookmark-label` resolving to `::before`/`::after`
  generated content; label `contents()` reads the element's text content.
- Named destinations; entries target XYZ destinations directly.

## Behavior

1. The UA stylesheet shall carry
   `h1 { bookmark-level: 1; bookmark-state: open }` …
   `h6 { bookmark-level: 6; bookmark-state: open }` with label `contents()`
   implied (the default label).
2. `bookmark-level` shall accept `none` or an integer 1–6. Values below 1 or
   above 6 are invalid; the whole declaration is ignored (the UA default
   survives). `none` suppresses the outline entry.
3. `bookmark-label` shall be a content list parsed by the same parser as the
   `content` property (`parse_content`): quoted strings, `contents()`
   (element text), `counter(name)`, `string(name)`, `counter(page(s))`,
   `leader()` (parsed, resolved as in `resolve_content`). Malformed or empty
   labels fall back to `contents()`.
4. `bookmark-state` shall accept `open` and `closed`; invalid values are
   ignored (UA default `open` survives). The outline node is built with
   `with_open(state == open)`; leaf nodes ignore the flag (krilla behavior).
5. Outline entries shall be collected in document (pre-order) order during
   the fragment-tree walk — the same order the element→page map is built —
   never from hash iteration.
6. Each entry's destination shall be an XYZ destination at the entry's
   element's first fragment: the recorded page index and the fragment's
   top-left y (PDF top-left origin, converted by krilla).
7. Nesting shall resolve at emit time from the collected levels with a
   stack, exactly as today: a deeper level nests under the nearest shallower
   entry; a shallower level closes deeper subtrees.
8. `--diagnostics json` (and text) shall gain a `bookmark-anchor-unresolved`
   warning event when an element carries any `bookmark-*` declaration but
   produces no fragment on any page (suppressed by `display: none`,
   removed from the tree, or otherwise unrendered). Headings without any
   `bookmark-*` declaration never trigger it.
9. Rendering with zero bookmarks (no headings, or all suppressed) shall
   produce no `/Outlines` object and remain byte-identical to the current
   output for documents that do not use bookmark properties.

## Interfaces

```rust
// css.rs — ComputedStyle additions (paged-decl pass, not stylo):
pub bookmark_level: BookmarkLevel,          // None | Level(u8 1..=6)
pub bookmark_label: Vec<ContentPiece>,      // empty = contents()
pub bookmark_state_open: bool,              // default true

// css.rs — PagedDecl variants:
BookmarkLevel(BookmarkLevel)
BookmarkLabel(Vec<ContentPiece>)
BookmarkState(bool)

// layout.rs — Heading gains the resolved fields + anchor:
pub struct Heading {
    pub level: u8,                  // from bookmark-level (1..=6)
    pub title: String,              // resolved bookmark-label
    pub state_open: bool,
    pub page_index: usize,          // first fragment's page
    pub y: f64,                     // first fragment's top y in the page (pt)
}

// pdf.rs — unchanged signature; build_outline nests by `level` and uses
// OutlineNode::new(title, XyzDestination::new(page, (0.0, y))).with_open(state)

// diagnostics.rs — new event code:
"bookmark-anchor-unresolved" (severity: warning)
```

## Collection mechanics

`build_headings`/`collect_headings` (layout.rs) already walk the DOM in
pre-order with the final pass's element→page map. The same pass additionally
records each bookmarked element's fragment y: a per-page walk of each
fragmentainer's fragment tree accumulating parent-relative offsets
(`record_sources` shape, plus a y accumulator), keyed by source NodeId. The
first (document-order) occurrence wins, matching the page map. Elements that
carry a `bookmark-*` declaration but are absent from the map produce the
`bookmark-anchor-unresolved` diagnostic and no entry.

Level semantics: `bookmark-level` is authoritative — a plain `h3` is level 3;
`h1 { bookmark-level: 3 }` nests as a level-3 entry. Nesting never skips a
generation in the tree: a level-4 entry following a level-1 entry still
nests under it (Prince behavior), which the existing stack algorithm
already does.

## Acceptance Criteria

1. **UA defaults** — Given an unstyled `h1>h2>h2` document, when rendered,
   the PDF outline nests `h1 > h2 > h2` and every entry targets its own
   heading's page. (engine/tests/bookmarks.rs::ua_defaults_nest)
2. **bookmark-level: none suppresses** — Given `h2 { bookmark-level: none }`,
   the h2 is absent from the outline while siblings remain.
   (::bookmark_level_none_suppresses)
3. **Explicit level overrides** — Given `h2 { bookmark-level: 1 }`, the entry
   is a root sibling of the h1 entries. (::explicit_level_overrides)
4. **Label literal + counter** — Given
   `h2 { bookmark-label: "Chapter " counter(chapter) }` with
   `counter-reset`/`counter-increment` in scope, entries read
   `Chapter 1`, `Chapter 2`. (::label_literal_and_counter)
5. **State open/closed** — Given `bookmark-state: closed` on a parent and
   `open` on another, the PDF `/Count` values are negative / positive
   respectively (asserted via the emitted PDF bytes).
   (::state_controls_count)
6. **Destination y-offset** — The destination of a heading mid-page points
   below the page top (destination y < page height in PDF coordinates;
   asserted on the emitted PDF). (::destination_y_offset)
7. **Broken anchor diagnostics** — Given
   `<h2 style="display:none">`-equivalent suppression via a class rule with
   `bookmark-level: 2`, `--diagnostics json` emits one
   `bookmark-anchor-unresolved` event. (::diagnostics_broken_anchor)
8. **No-regression** — Documents without bookmark declarations render
   byte-identical to the pre-bookmark output for the existing test suite
   (`pdf_bookmarks` test in paged_media.rs continues to pass with updated
   expectations); `cargo test` green.

## Edge Cases

- Heading inside a table cell or flex item — the anchor comes from the
  element's own first fragment wherever it lands; no special case.
- A heading broken across pages — first fragment wins.
- `bookmark-label: ""` — empty literal resolves to an empty entry text
  (valid, renders as a blank outline row); not an error.
- `counter()` in the label reads the counter state at the element's box
  start, same as `string-set` assignment timing.
- A document with only `bookmark-state` set (no level) keeps the UA level.
- Out-of-range page anchors can never be emitted: entries are collected
  from the same map `target-counter` uses; an unresolved anchor drops the
  entry and emits the diagnostic instead of an invalid destination
  (krilla panics on out-of-range page indexes — never construct one).

## References

- css-gcpm-3 §Bookmarks (W3C)
- Prince 16.2 html.css default heading bookmarks (probed: headings produce
  bookmarks; non-headings never do without explicit bookmark-level)
- `paged-media-css.spec.md` (first bookmark cut), `pdf-metadata.spec.md` (metadata render
  plumbing), `tagged-pdf.spec.md` (fragment-source attribution), and `diagnostics.spec.md` (diagnostics
  schema).
