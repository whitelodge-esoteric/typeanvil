---
title: Hyperlink Annotations
slug: /specifications/hyperlinks
type: spec
status: draft
owner: maintainers
created: 2026-08-21
updated: 2026-09-27
sidebar_position: 14
tags: [engine, pdf, annotations, determinism]
spec_id: hyperlinks
applies_to: engine 0.x
dependencies: [fragmentation-core, pdf-metadata]
---

# Hyperlink Annotations

## Overview

`<a href>` currently renders as dead text. This spec adds PDF link
annotations: external URLs become `/URI` actions and internal `#fragment`
links become GoTo destinations at the target's page. krilla 0.8.2 provides
the full annotation surface needed (verified 2026-08-21 against
`krilla-0.8.2/src/interactive/{annotation,action,destination}.rs`):

- `krilla::interactive::annotation::{Annotation, LinkAnnotation, Target}`
- `LinkAnnotation::new(rect, target)` — single rect; multi-line links emit
  one annotation per line segment (Decision D2).
- `Target::Action(Action::Link(LinkAction::new(uri)))` for external URLs.
- `Target::Destination(Destination::Xyz(XyzDestination::new(page_index,
  point)))` for internal links.
- `page.add_annotation(annotation)` on `krilla::page::Page`.
- Annotation alt-text validation errors only register under PDF/A / PDF-UA
  validators; the engine's default settings are unaffected.

The engine's coordinate system (top-left origin, y down, points) matches
krilla's surface space directly — no flip is needed for link rects.

## Goals

1. External `<a href="https://…">` produces a clickable `/URI` annotation
   over exactly the glyphs of the link text, per line segment.
2. Internal `<a href="#id">` produces a GoTo destination to the page the
   `id`-carrying element landed on.
3. Multi-line links produce one activation rect per line.
4. Deterministic output: annotations in document/paint order, identical
   input → byte-identical PDF.

## Non-Goals

1. **Link styling** (`color`, `text-decoration: underline`). The inline
   content model folds inline elements into the parent block's styled run;
   per-inline-element styling needs an inline-box model and is deferred.
   Prince's default renders links blue + underlined; TypeAnvil renders them
   unstyled body color. Recorded as a known gap (follow-up ticket), not a
   silent accept.
2. Named destinations (`/Dests` name tree) — direct XYZ destinations suffice.
3. `target-counter(attr(href))` interplay changes (already works via
   `target_pages`; untouched here).
4. Link borders or highlight modes beyond krilla defaults.

## Behavior

1. The engine SHALL collect, for every `<a>` element with an `href`
   attribute encountered during inline item collection, the byte range of
   its text within the enclosing item's text plus the raw href value.
2. After line breaking, for each line whose source span overlaps a link
   span, the engine SHALL compute the x extent of the overlap from the
   line's shaped glyphs (`ShapedGlyph.range` byte ranges) and
   record one link rect per overlapping segment, attached to the page the
   line was placed on.
3. A link whose text breaks across pages SHALL produce rects on each page,
   each pointing at the same target.
4. An href beginning with `#` SHALL resolve against element `id` attributes:
   the destination is the page index of the first element (document order)
   carrying that id. No matching id → no annotation is emitted (no error,
   no crash).
5. Any other href value (`https://`, `http://`, `mailto:`, relative) SHALL
   be emitted verbatim as a URI action string.
6. Annotations SHALL be emitted in document paint order (order of line
   placement during pagination); internal destinations use
   `XyzDestination::new(page_index, Point::from_xy(0.0, 0.0))` — the top of
   the target page (v1; exact-y targets deferred).
7. On pages with `page-orientation` set, link rects SHALL be mapped through
   the same rotation transform applied to content (pdf.rs orientation arms),
   because krilla serializes annotation rects independently of the surface
   transform.
8. Layout SHALL expose collected links on `Layout` (like `headings`) so
   pdf.rs can emit them without re-walking fragments.
9. Determinism: link collection SHALL depend only on DOM order and layout
   results — no HashMap iteration order, no wall clock.

## Interfaces

```rust
// layout.rs
pub struct PageLink {
    /// Zero-based page index the rect sits on.
    pub page_index: usize,
    /// Rect in page coordinates (top-left origin, points): x, y, w, h.
    pub x: Scalar, pub y: Scalar, pub w: Scalar, pub h: Scalar,
    pub target: LinkTarget,
}

pub enum LinkTarget {
    /// Verbatim URI action (external links).
    Url(String),
    /// Zero-based index of the destination page (internal links,
    /// resolved from `#fragment` via the id → page map).
    Page(usize),
}

pub struct Layout {
    // existing fields…
    /// Link annotation records, in document paint order.
    pub links: Vec<PageLink>,
}
```

```rust
// pdf.rs (per page, after content draw, before surface.finish())
for pl in layout.links.iter().filter(|pl| pl.page_index == idx) {
    let target = match &pl.target {
        LinkTarget::Url(u) => Target::Action(Action::Link(LinkAction::new(u.clone()))),
        LinkTarget::Page(p) => Target::Destination(
            Destination::Xyz(XyzDestination::new(*p, Point::from_xy(0.0, 0.0)))),
    };
    pdf_page.add_annotation(Annotation::from(LinkAnnotation::new(rect, target)));
}
```

Implementation seam (layout.rs):

- `collect_items_rec` records `(start_byte, end_byte, href)` spans around
  inline `<a>` recursion into the pending string; `Item::Text` carries its
  span list alongside the text.
- The line-placement loop already tracks absolute source offsets per line
  (`lr.consumed`); a line's link sub-spans come from intersecting
  `[line_start, line_end)` with the recorded spans, then mapping byte ranges
  to glyph x-extents via `ShapedGlyph.range`.

## Acceptance Criteria

1. **External link emits a URI annotation** — Given an HTML doc with
   `<a href="https://example.com/">Example</a>`, When rendered, Then the
   PDF contains a `/URI` annotation whose rect covers the link glyphs and
   whose action string is `https://example.com/`. Test:
   `engine/tests/links.rs::external_link_emits_uri_annotation`.
2. **Internal cross-page link resolves to the target page** — Given a TOC
   entry linking `#sec` where `#sec` lands on page 2 (forced with
   `break-before: page`), When rendered, Then the link annotation's
   destination references page index 1. Test:
   `engine/tests/links.rs::internal_link_targets_correct_page`.
3. **Multi-line link produces one rect per line** — Given a long URL forced
   to wrap across two lines inside a narrow container, When rendered, Then
   two annotations exist with disjoint y ranges covering both lines. Test:
   `engine/tests/links.rs::multi_line_link_has_rect_per_line`.
4. **Link across a page break yields rects on both pages** — Given a link
   paragraph broken across pages by page height, When rendered, Then rects
   exist on both pages targeting the same href. Test:
   `engine/tests/links.rs::link_across_page_break_rects_on_both_pages`.
5. **Dangling fragment emits nothing** — Given `href="#missing"` with no
   such id, When rendered, Then no annotation exists and export succeeds.
   Test: `engine/tests/links.rs::dangling_fragment_emits_no_annotation`.
6. **Determinism** — rendering the link fixture twice yields byte-identical
   PDFs. Test: `engine/tests/links.rs::link_render_is_deterministic`.

## Edge Cases

- Nested inline elements inside `<a>` (`<a><strong>x</strong>y</a>`): the
  span covers all folded text of the anchor.
- A link spanning a float-induced width change: each line segment gets its
  own rect at its true x extent (the segment loop already handles this).
- Whitespace-only anchors (`<a href="…"> </a>`): zero-width overlap → no
  rect emitted.
- Rotated pages: rect corners mapped through the orientation transform;
  axis-aligned bounding box emitted (krilla rect model has no rotation).
- `target_pages` map reuse: internal resolution uses the same bounded two-
  pass machinery as `target-counter`; unresolved ids after the final pass
  drop the annotation.

## References

- krilla interactive modules (`annotation.rs`, `action.rs`, `destination.rs`) —
  API verified against the vendored dependency.
- krilla 0.8.2 interactive modules (`annotation.rs`, `action.rs`,
  `destination.rs`) — API verified 2026-08-21.
- `docs/specifications/pdf-metadata.spec.md` (Layout-level document data +
  deterministic emit precedent).
- `docs/specifications/hyphenation-line-break-parity.spec.md` (source-byte
  accounting, `lr.consumed`).
- css-flexbox/css-text are not implicated; the WPT gate applies unchanged.
