---
title: Paged-Media CSS
slug: /specifications/paged-media-css
type: spec
status: draft
owner: elijah
created: 2026-08-16
updated: 2026-08-20
sidebar_position: 3
tags: [css, paged-media, page, layout, engine]
spec_id: paged-media-css
issue_id: CORE-52
applies_to: engine 0.x
dependencies: [wpt-conformance-harness, fragmentation-core]
---

# Paged-Media CSS

## Overview

The wedge features — what Prince does that Chromium can't, targeted at
reports and invoices:

- `@page` rules: size, margins, margin boxes (running headers/footers)
- Named pages, the `page` property, `:first`/`:left`/`:right` selectors
- `string-set` / `string()` running content
- Counters (page numbers, cross-refs), `target-counter` for TOC page numbers
- `leader('.')` dotted TOC leaders
- PDF bookmarks from heading structure

The fragmentainers-first model from CORE-51 is the foundation: pages are
already first-class fragments with no source box, so `@page` geometry and
margin boxes attach naturally to each `Fragmentainer`. This issue makes the
page box a *resolved, per-page* thing (size, margins, margin-box content)
instead of the fixed CLI geometry.

**Fitness function:** css-page WPT print-reftests via the harness
(`docs/specifications/wpt-conformance-harness.spec.md`) — subset first,
growing as the features land; the harness's `--engine cli` adapter renders
through `typeanvil render`.

**Done:** a real invoice and a multi-section report render with running
headers, page numbers, and TOC leaders — the demo documents for the wedge
market.

## Goals / Non-Goals

**Goals**

- Parse `@page` rules from author CSS: default page and named pages, with
  `:first` / `:left` / `:right` pseudo-selectors; `size` and margin longhands;
  margin boxes (`@top-left`, `@top-center`, `@top-right`, `@bottom-left`,
  `@bottom-center`, `@bottom-right`, and the four corners) with a `content`
  property.
- Resolve each `Fragmentainer`'s page spec independently: size, margins, and
  margin-box content — so named pages and first/left/right selectors actually
  change the rendered page box.
- `page: <name>` property on elements: boxes carrying it switch the page
  context for the pages they start.
- Running content via `string-set` / `string()`: per-page value of a named
  string = the last assignment in that page's flow; margin boxes render
  `string(name)`.
- Counters: implicit `page` counter (increments per fragmentainer),
  `counter-reset`/`counter-increment` (page counter supported; named counters
  parsed for future use), `counter(page)` and `counter(pages)` (total page
  count) in margin boxes, and `target-counter(attr(href), page)` for TOC
  entries resolved by a bounded two-pass layout.
- `leader('.')` in inline content: fills the remaining line width to the
  content edge with a repeating character.
- PDF bookmarks: an outline tree from `h1`–`h6` heading structure with target
  page numbers, emitted via krilla's native outline API.
- Determinism: identical input → byte-identical PDF (all new passes are
  deterministic; no hash-order dependence).

**Non-Goals** (deferred; noted here so scope stays honest)

- `element()` running *elements* (cloning a styled element subtree into a
  margin box) — `string()` covers the demo; element() needs subtree cloning
  and margin-box styling and is a later pass.
- Full css-page WPT parity — the fitness gate is a growing subset; the full
  367-file suite is a stretch goal, not this issue.
- `@page :blank`, `:nth()`, or other page pseudo-classes beyond
  first/left/right.
- Side margin boxes (`@left-*`, `@right-*` — vertical writing-mode boxes):
  parsed and positioned, but content is treated as a single horizontal line
  (no rotation); vertical writing modes are out of scope.
- Footnotes, cross-references beyond `target-counter(..., page)`, named
  counter styles (roman etc.), `target-text`.
- CSS `size: landscape/portrait` keywords beyond the named page-size keywords
  (A4, Letter, Legal) and explicit lengths.

## Behavior

The engine shall:

1. **Parse `@page` rules** from the author stylesheet: a targeted, deterministic
   author-CSS pass (the same pattern as CORE-51's `breaks` module — stylo's
   servo build does not compile `@page` or the paged-media longhands). Rules
   carry: optional name (default page if absent), pseudo-class
   (`:first`/`:left`/`:right`/none), `size`, margin longhands, and margin-box
   declarations. Unknown declarations are skipped without failing.
2. **Resolve per-page spec**: each `Fragmentainer` selects the matching `@page`
   rule by (a) page name in effect, then (b) pseudo-class from global page
   index — index 0 is `:first` (and `:right` per LTR progression), odd 1-based
   indices are `:right`, even are `:left`; most-specific match wins; if no rule
   matches, the CLI-provided geometry is the fallback.
3. **Override CLI geometry**: `@page` `size` and margins override the CLI
   `--page-width`/`--page-height`/`--margin-*` defaults for pages they match;
   the CLI values remain the default when no rule applies. The CLI contract's
   flag shape is unchanged.
4. **Honor named pages**: an element with `page: <name>` switches the page
   context; pages that the element's boxes start use `@page <name>` (falling
   back to the default page spec if no such rule exists).
5. **Emit margin boxes as fragments**: each fragmentainer's root fragment
   carries margin-box child fragments positioned in the page margin area
   (top row above the content box, bottom row below; corners and center/left/
   right per css-page-3), so the existing PDF fragment-tree walk draws them
   with no separate pass.
6. **Render margin-box content**: `content` values of literal text,
   `string(name)`, `counter(page)`, and `counter(<name>)` are resolved at
   fragmentainer build time; each margin box is one line, no wrapping,
   deterministically clipped if it overflows its box. Literal non-ASCII
   (em dash, curly quotes, `·`) is shaped like body text — a real glyph
   with a ToUnicode mapping, never raw UTF-8 bytes as Latin-1 (CORE-83).
7. **Thread running strings**: `string-set: <name> content()` on an element
   assigns the element's text content to the named string; the value in effect
   for a page is the last assignment encountered in that page's document-order
   flow; `string(name)` in a margin box renders that value (empty before the
   first assignment).
8. **Increment the page counter**: the implicit `page` counter equals the
   1-based fragmentainer index unless reset; `counter-reset: page <n>` on an
   element takes effect at the page its box starts; `counter(page)` in margin
   boxes renders the decimal value. The implicit `pages` counter
   (`counter(pages)`) renders the document's total page count on every page;
   because the total is only known after pagination, documents using
   `counter(pages)` run the same bounded two-pass layout as §9 (margin-box
   text cannot change pagination, so the page count is stable and the
   resolution converges).
9. **Resolve `target-counter(attr(href), page)` by bounded two-pass layout**:
   pass 1 lays out and records the fragmentainer index of every element with
   an `id` (from the fragment tree's source mapping); pass 2 re-lays out with
   TOC entries resolved to their target pages. Converges because
   `target-counter` text appears in leader-filled lines whose width does not
   depend on the number glyphs; hard cap 3 passes, last result wins
   (deterministic either way).
10. **Fill leaders**: `leader('.')` in an inline text run fills from the last
    character to the right content edge with the repeating character, at line
    break time, deterministically.
11. **Emit PDF bookmarks**: build an outline tree from `h1`–`h6` elements in
    DOM order (nested by heading level), each node titled with the heading's
    text content and targeted at the fragmentainer index where the heading
    landed; emit via krilla's `Document::set_outline` (krilla 0.8 has native
    `Outline`/`OutlineNode`).
12. **Stay deterministic**: all new passes (page-rule parse, per-page spec
    resolution, running-string state, counter state, two-pass TOC resolution,
    outline build) are deterministic; no `HashMap` iteration order in output
    paths; identical input yields byte-identical PDF.
13. **Preserve O(n)**: named pages and per-page specs do not reintroduce
    super-linear pagination; the existing 1,000-page linear test still holds.
14. **Keep the fragment-tree contract**: `layout(dom, stylesheet, geometry) ->
    Layout` signature is unchanged; `Layout.pages` remain `Fragmentainer`s;
    fragmentation, break tokens, and appeal scoring from CORE-51 are
    untouched by this issue.
15. **Keep the CLI contract**: `typeanvil render <input.html> --page-width ...
    --page-height ... --margin-* ... -o out.pdf` still works (flags are now
    defaults that `@page` may override).

## Interfaces

**New module** `engine/src/paged.rs`:

```text
PageRule            // one parsed @page rule
  name: Option<String>        // None = default page
  pseudo: PagePseudo          // First | Left | Right | None
  size: Option<(Scalar, Scalar)>     // explicit size, points
  margins: Option<PageMargins>        // top/right/bottom/left, points
  margin_boxes: Vec<MarginBoxDecl>

PagePseudo          // First | Left | Right | None

MarginBoxName       // TopLeftCorner | TopLeft | TopCenter | TopRight |
                    // TopRightCorner | BottomLeftCorner | BottomLeft |
                    // BottomCenter | BottomRight | BottomRightCorner |
                    // LeftTop | LeftMiddle | LeftBottom |
                    // RightTop | RightMiddle | RightBottom

MarginBoxDecl       // name: MarginBoxName, content: MarginBoxContent

MarginBoxContent    // Literal(String) | StringRef(String) |
                    // CounterPage | CounterRef(String) | TargetCounter(String)

PageSpec            // resolved page geometry + margin boxes for one page
  size: (Scalar, Scalar)
  margins: PageMargins
  margin_boxes: Vec<(MarginBoxName, MarginBoxContent)>

PageMargins         // top/right/bottom/left: Scalar

RunningStrings      // name -> current value (threaded per page snapshot)
```

**`engine/src/css.rs` additions** — `ComputedStyle` gains:

```text
page: Option<String>                    // the `page` property (named page)
string_set: Vec<(String, StringSetValue)>  // string-set declarations
counter_reset: Vec<(String, i32)>       // counter-reset declarations
counter_increment: Vec<(String, i32)>   // counter-increment declarations
```

`StringSetValue` is `Content` (the element's text content) — `attr()` values
are parsed but resolved as content for now. These are filled by the same
targeted author-CSS pass as CORE-51's `breaks` module (extend it or add a
sibling module — the exact shape is the implementer's call, documented in
code).

**`engine/src/frag.rs` addition** — `Fragment` gains a source mapping so
`target-counter` and bookmarks can find where an element landed:

```text
source: Option<NodeId>   // DOM node that produced this fragment (None for
                         // fragmentainers / anonymous boxes)
```

**`engine/src/layout.rs`**: `layout()` keeps its signature. Internally:
resolve per-page `PageSpec` before building each `Fragmentainer`; thread
running-string and page-counter state; attach margin-box fragments to each
fragmentainer root; run the bounded two-pass TOC resolution when
`target-counter` appears in the document.

**`engine/src/pdf.rs`**: unchanged walk (margin boxes are fragments now);
add outline emission via krilla's `Document::set_outline` built from the
heading structure.

**CLI**: unchanged (see the harness spec's engine adapter contract).

## Acceptance Criteria

Given/When/Then, each mapping to a real test in `engine/tests/paged_media.rs`:

1. **Page size from `@page`** — Given `@page { size: 8.5in 11in; }` with CLI
   defaults 5in×3in, when rendered, then every fragmentainer's size is
   8.5in×11in (`paged_media.rs::page_size_from_at_page`).
2. **Page margins from `@page`** — Given `@page { margin: 1in; }`, when
   rendered, then the content rect is inset 1in on all sides
   (`paged_media.rs::page_margins_from_at_page`).
3. **Margin-box header** — Given `@page { @top-center { content: "Report"; } }`,
   when rendered, then a text run "Report" appears in the top margin area of
   every page (`paged_media.rs::margin_box_header`).
4. **Named pages** — Given `@page landscape { size: 8.5in 11in; }` and
   `section.note { page: landscape; }`, when a section with that class renders,
   then the pages it starts use the landscape size while other pages keep the
   default (`paged_media.rs::named_pages`).
5. **First/left/right selectors** — Given `@page :first { margin-top: 2in; }`,
   when rendered, then page 1's top margin differs from page 2's, and
   `:left`/`:right` selectors apply by page parity
   (`paged_media.rs::first_left_right`).
6. **Running string** — Given `h1 { string-set: chapter content(); }` and
   `@top-left { content: string(chapter); }`, when a multi-section document
   renders, then the header text reflects the current chapter on each page
   and is empty on pages before the first `h1`
   (`paged_media.rs::running_header_string`).
7. **Page counter** — Given `@bottom-right { content: counter(page); }`, when a
   3-page document renders, then "1", "2", "3" appear in the bottom-right of
   successive pages, and `counter-reset: page 0` on a section restarts the
   count (`paged_media.rs::page_counter`). Given
   `@bottom-center { content: "Page " counter(page) " of " counter(pages); }`,
   when a multi-page document renders, then every footer reads "Page N of M"
   with M = the document's total page count, on every page — no "of 0"
   (`paged_media.rs::total_page_counter`).
8. **TOC target-counter** — Given a TOC whose entries use
   `content: leader('.') target-counter(attr(href), page)`, when rendered,
   then each entry shows the correct target page number and dotted leaders
   reach the right content edge (`paged_media.rs::toc_target_counter`).
9. **PDF bookmarks** — Given a document with nested `h1`/`h2`, when rendered,
   then the outline has entries titled with the headings, nested by level, at
   the correct pages (`paged_media.rs::pdf_bookmarks` — assert on the outline
   model, or krilla's emitted outline if directly readable).
10. **Invoice demo** — Given the invoice fixture
    (`engine/tests/fixtures/invoice.html`) with a running header, footer page
    numbers, and a styled table, when rendered, then it is multi-page, valid,
    and byte-deterministic (`paged_media.rs::invoice_demo`).
11. **Report demo** — Given the report fixture
    (`engine/tests/fixtures/report.html`) with per-chapter running headers,
    page numbers, TOC leaders, and headings, when rendered, then it is
    multi-page, byte-deterministic, and contains the TOC page numbers
    (`paged_media.rs::report_demo`).
12. **Determinism** — Given the same multi-page input with margin boxes
    rendered twice, then the PDF bytes are identical
    (`paged_media.rs::determinism_multi`).
13. **Regression: fragmentation intact** — the existing fragmentation tests
    (forced break, orphans/widows, avoid, monolithic, 1,000-page linear) still
    pass unchanged.
14. **Margin-box non-ASCII** — Given `@top-center { content: "Northwind — ·
    2026"; }` (and a header with a middle dot), when rendered, then the
    header extracts as the correct em dash and middle dot (raster + text
    extraction), no `â□□` mojibake
    (`tounicode.rs::margin_box_runs_are_shaped`,
    `tounicode.rs::margin_box_tounicode_map`).
15. **Self-check** — the spec file itself passes `scripts/validate_docs.py`.

## Edge Cases

- **No `@page` rule matches** → CLI geometry is used; no margin boxes.
- **Named page rule missing** → falls back to the default page spec.
- **`@page` rule with no `size`** → inherits the default/CLI size.
- **`string()` referenced before any assignment on page 1** → empty string.
- **`target-counter` target on the same page as the TOC entry** → its page
  number is used; no infinite loop (pass 2 converges or the 3-pass cap hits).
- **Heading without an `id`** → bookmark still emitted (outline targets the
  page, not a fragment anchor); TOC link without a matching id → the entry
  resolves to "?" or is left empty (deterministic choice, documented in code).
- **Margin-box text wider than its box** → clipped deterministically, one
  line, no wrap.
- **`leader('.')` with no room** → no fill characters; line ends normally.
- **Deep heading nesting (h1–h6)** → outline depth capped at 6; h6 is the
  deepest node.
- **Anonymous/box fragments with `source: None`** → never targeted by
  `target-counter` and never bookmarked.
- **`counter-reset: page` on an element that starts mid-page** → takes effect
  at the page the element's box starts.
- **Two-pass convergence failure** (page numbers shift between passes) → cap
  at 3 passes, use the last result; documented in code as a known limitation.

## References

- CORE-51 spec: `docs/specifications/fragmentation-core.spec.md` (the
  fragment-tree contract this builds on).
- Harness spec: `docs/specifications/wpt-conformance-harness.spec.md`
  (fitness function; the `--engine cli` adapter contract for `typeanvil
  render`; css-page is 367 WPT files, 333 print-named).
- CORE-51 code: `engine/src/frag.rs`, `engine/src/layout.rs` (fragmentainers,
  break tokens), `engine/src/css.rs` (the `breaks` author-CSS pass pattern).
- krilla 0.8: `Document::set_outline`, `interchange::outline::{Outline,
  OutlineNode}` (native PDF bookmarks; source in
  `~/.cargo/registry/src/*/krilla-0.8*/`).
- W3C: CSS Paged Media Module Level 3 (w3.org/TR/css-page-3), CSS
  Generated Content for Paged Media (w3.org/TR/css-gcpm-3 — `string-set`,
  `leader()`, `target-counter`).
- Vault: `brain/Projects/Typeanvil/Project Overview.md` (typography/demo
  positioning), `docs/research/rust-ecosystem/typeanvil-rust-typesetting-research.md`.
