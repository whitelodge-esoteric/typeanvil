---
title: Paged-Media CSS
slug: /specifications/paged-media-css
type: spec
status: draft
owner: elijah
created: 2026-08-16
updated: 2026-09-15
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
- Per-side border colours on a box (one colour paints all four sides), for
  margin boxes and elements alike. CORE-201 records the gap and the measured
  reason it flips no WPT test: no fixture in `css-page/margin-boxes` declares
  a per-side border colour at all, and the fixtures that do declare one fail
  on pagination rather than on colour (probe: `border-bottom-color: cyan`
  currently repaints all four bands).
- Margin-box intrinsic sizing in the box's own writing mode (CORE-184).
  **Measured 2026-09-15 (CORE-182): this — not writing-mode rotation — is what
  the four `css-page/margin-boxes/dimensions-004/006/013/014` targets fail on.**
  `writing-mode` is not even parsed for a margin box today (`MarginBoxSpec`
  carries no such field), and the four references SIMULATE vertical text with
  horizontal `<br>`-separated blocks (e.g. dimensions-013's ref paints
  `@top-left`'s seven vertical lines as one
  `<div style="width:17.5em">xxxxxxx</div>`), so a rotation-only change could
  not match them. What differs is intrinsic sizing in the box's own writing
  mode (seven lines stack along the block axis → min-content WIDTH 7em, per
  dimensions-013's own comment). CORE-178 landed the multi-line content that
  sizing depends on.
- Margin-box glyph ROTATION for a real font in a vertical writing mode. The
  `dimensions-*` fixtures use the Ahem font, whose glyph is a solid square, so
  rotation is invisible to them by construction and the gate cannot measure it.
- Document-interior vertical text layout (rotated glyph runs, vertical line
  boxes). CORE-182 re-keyed the vertical BLOCK geometry on the mode in effect
  at the box, but text still advances along physical +x inside a vertical
  subtree: no glyph run is rotated, and an interior vertical subtree that
  crosses a page edge is not sliced along its own block axis. CORE-181's
  root-vertical page progression is built on top of this and owns the eight
  `body-background-*` / `block-00{1,2}-wm-*` / `page-box-008` targets.
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
   declarations. Unknown declarations are skipped without failing. A prelude
   token joins name and pseudo without whitespace (`@page a:first` — the first
   colon starts the pseudo; CORE-143). Rules inherit the cascade-layer rank of
   the `@layer` block that encloses them (css-cascade-5 §6): unlayered rules
   beat every layer, later-declared layers beat earlier ones regardless of
   source position (CORE-143).
2. **Resolve per-page spec**: each `Fragmentainer` selects the matching `@page`
   rule by (a) page name in effect, then (b) pseudo-class from global page
   index — index 0 is `:first` (and `:right` per LTR progression), odd 1-based
   indices are `:right`, even are `:left`; most-specific match wins (layer
   rank dominates pseudo specificity; CORE-143); if no rule
   matches, the CLI-provided geometry is the fallback.
3. **Override CLI geometry**: `@page` `size` and margins override the CLI
   `--page-width`/`--page-height`/`--margin-*` defaults for pages they match;
   the CLI values remain the default when no rule applies. The CLI contract's
   flag shape is unchanged.
3a. **Resolve viewport units against the page box (CORE-140)**: `vh`/`vw`
   (and viewport-unit insets) are carried RAW through the cascade and
   resolved at layout time against the initial containing block's content
   box (css-values-4 §7.8 print behavior; 100vh = page content height). The
   stylo viewport stays fixed at 1024x768 (CORE-66) — resolution happens in
   layout, not in style. This flipped page-margin-001/003 and
   page-size-009 PASS; fixedpos-007/008 and underflow-from-next-page
   flipped FAIL, exposing the abspos-across-pages and negative-margin-at-
   breaks gaps (documented residuals, not vh defects — their accidental
   passes under the fixed viewport were not spec behavior).
4. **Honor named pages**: an element with `page: <name>` switches the page
   context; pages that the element's boxes start use `@page <name>` (falling
   back to the default page spec if no such rule exists). The context is the
   *effective* `page` value (css-page-3 §4): a box's own non-auto declaration,
   else the nearest ancestor-or-self with one, else the default page. A fresh
   box whose effective value is the default (`page: auto` or no named ancestor)
   **resets** the context to the default page; a pure continuation page
   (no box starts fresh at its top) carries the previous name. The very first
   page can activate a named page (the fresh break token descends into the
   first block child).
5. **Emit margin boxes as fragments**: each fragmentainer's root fragment
   carries margin-box child fragments positioned in the page margin area
   (top row above the content box, bottom row below; corners and center/left/
   right per css-page-3), so the existing PDF fragment-tree walk draws them
   with no separate pass. A center-aligned top/bottom box is centered on the
   CONTENT-box midline `((left + right) / 2)`, never inside a fixed third
   slot: text wider than a third spills into adjacent slots symmetrically
   (css-page-3 margin-box geometry; matches Prince 16.2 — CORE-117).
6. **Render margin-box content**: `content` values of literal text,
   `string(name)`, `counter(page)`, and `counter(<name>)` are resolved at
   fragmentainer build time, then broken into lines. CSS escapes in the
   declaration text are processed (css-syntax-3 §4.3.7), so `"\a"` is a
   newline. A newline forces a line break when the box's `white-space`
   preserves breaks (`pre`, `pre-wrap`, `pre-line`, `break-spaces`) and
   otherwise becomes a space (css-text-3 §4.1.1). The box's `white-space` is
   its own declaration, else the page context's, else `normal`; css-page-3
   Appendix A lists `white-space` as applicable inside a margin box. Lines
   stack at the box's `line-height`: the content's block extent is the line
   count times that pitch, its max-content extent is the widest line, and its
   min-content extent is the widest line's min-content (css-sizing-3 §5.1).
   Each line aligns independently by `text-align`; no line soft-wraps, and
   each is deterministically clipped if it overflows the box.
   A `url(<path>)` piece (CORE-141) shall paint its image as replaced inline
   content of that image's INTRINSIC size (96 DPI pixels converted to points),
   following the text of the LAST line. The line box shall grow to the image
   (css2 §10.8): a 50px image in a 50px page margin fills the band and the
   text baseline moves to the image's bottom margin edge, rather than the image
   hanging above the box. The path resolves like an `<img src>` and is interned
   in the same image store, so the PDF emitter embeds it once. Literal non-ASCII
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
    break time, deterministically. The fill pitch is the leader character's
    real shaped advance (CORE-99); the width RESERVED for the fixed text
    parts is shaped at real width for literal pieces and uses a flat 0.5em
    per-character estimate for resolved pieces (`counter`, `target-counter`),
    so the fill count never depends on the resolved number glyphs and the
    two-pass TOC still converges (§9).
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
16. **Size and paint the page-margin boxes (CORE-141)**: each margin box shall
    be a real CSS box. The engine shall parse the box-model declarations of a
    margin box (`width`, `height`, the `margin-*` and `padding-*` longhands and
    their shorthands, `border`/`border-<side>`, `background`/`background-color`)
    and merge them property-by-property across `@page` rules.
    - The engine shall resolve each edge's used sizes by css-page-3 §5.3.2:
      the three boxes of an edge share the page area's extent along it, an
      `auto` size is resolved from the box's max-content and min-content sizes
      by the §5.3.2.2 three-step flex distribution, and a box whose size is
      declared keeps it. Auto margins on that axis are zero (§5.3.2.1). The
      start box is flush with the start edge, the middle box is centered, the
      end box is flush with the end edge (§5.3.2.4).
    - The engine shall resolve the fixed dimension by §5.3.3: the box's
      margins, borders, padding and size shall sum to the page margin on that
      edge; an auto size takes the space the margins leave; when both margins
      are auto they are equal (which centers the box in the band); and in the
      over-constrained case the margin facing away from the page center is
      treated as auto (the end margin for a bottom/right box, the start margin
      otherwise).
    - A corner box shall be fixed in both dimensions: its width is the side
      page margin and its height the top/bottom page margin meeting there.
    - Each box shall paint its background over its border box and its border
      as a band inside that box, and shall place its generated content inside
      the content box with its resolved `text-align` / `vertical-align`.
      A box whose content is empty (`content: ""`) shall still generate and
      paint.
    - A margin box's `background-image: url(...)` (or the url() piece of its
      `background` shorthand) shall paint the image over the border box
      tiled at its intrinsic size from the box's top-left (css-backgrounds-3
      §2.1 default `repeat`/position 0% 0%), clipped to the box so an edge
      tile never bleeds into a neighbour, and UNDER the border. The
      shorthand may carry a colour AND an image; the colour paints below the
      tiled image.
    - A declared page area (`@page { width; height }`) shall be honoured
      INSIDE the page box (CORE-144) and shall NOT resize the page box: the
      requested page size is the page size. An earlier attempt grew the box to
      area + margins; it flipped no tests, `width`/`height` are not css-page-3
      page descriptors, Chromium keeps the requested size, and the CLI contract
      requires the page-size flags to be honoured exactly.
17. **Page-margin box painting order and `z-index` (CORE-179)**: the engine
    shall paint a page in css-page-3 §3.1's layer order — page background,
    document canvas, page borders, document contents, then page-margin boxes —
    and shall treat the document canvas, the page borders and ALL document
    content as a single `z-index: 0` stacking context for that purpose.
    - A margin box shall never interleave with document content: a box paints
      either entirely in front of that group or entirely behind it.
    - `z-index` shall apply to a margin box as if it were positioned, and each
      margin box shall be its own stacking context: a box with a NEGATIVE
      `z-index` shall paint behind the document canvas and all document
      content, while `z-index: auto` (used as 0) or a positive value shall
      paint in front of all document content. Boxes sharing a `z-index` shall
      paint in the default order below.
    - The default paint order among margin boxes shall be `@top-left-corner`
      first, then clockwise: `@top-left`, `@top-center`, `@top-right`,
      `@top-right-corner`, `@right-top`, `@right-middle`, `@right-bottom`,
      `@bottom-right-corner`, `@bottom-right`, `@bottom-center`,
      `@bottom-left`, `@bottom-left-corner`, `@left-bottom`, `@left-middle`,
      `@left-top`. The order shall follow the BOX, not the order the boxes are
      declared in the `@page` rule.
    - Each margin box shall be painted as a UNIT — its background, then its
      border, then its content — before the next box starts, so an overlapping
      box covers the previous box's text. The page-wide "all backgrounds, then
      all borders, then all text" passes do not satisfy this on their own.
    - A page-anchored out-of-flow box with a NEGATIVE used `z-index` shall
      attach BEFORE the in-flow content of its page, because the emitter walks
      the page root in pre-order (CSS2.1 Appendix E: a negative stacking
      context paints after the context's own background and before any in-flow
      block background).

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
running-string and page-counter state; attach each margin-box fragment to the
fragmentainer's `margin_boxes` list (NOT to the content root — a margin box is
its own stacking context and defaults to painting in front of all content);
run the bounded two-pass TOC resolution when `target-counter` appears in the
document.

**`engine/src/frag.rs`**: `MarginBoxFragment { z_index, order, fragment }` —
the margin box's stacking key plus its subtree — and
`Fragmentainer::margin_boxes`. `order` is the box's css-page-3 §3.1 slot, so
it follows the BOX and not the declaration order. `MarginBoxName::paint_order()`
(paged.rs) supplies it.

**`engine/src/pdf.rs`**: paints the page in §3.1 layer order. The content walk
covers document content only; margin boxes paint in two phases around it —
`z_index < 0` before the canvas fill, `z_index >= 0` after the text pass —
each phase sorted by `(z_index, order)` and each box painted by
`paint_margin_box()` as a UNIT (its own background, then border, then
content). A page-anchored out-of-flow box with a negative used `z-index`
attaches before the content fragment in the page root. Also: add outline
emission via krilla's `Document::set_outline` built from the heading
structure.

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
10. **Page-change boundary with empty page-declaring boxes** — Given sibling
    empty `div`s where one declares `page: a` and the next declares `page: b`
    (or declares nothing), when rendered, then a page break fires between them
    (`paged_media.rs::page_change_break_between_empty_page_declaring_divs`,
    `paged_media.rs::page_change_break_to_undeclared_sibling_page` — CORE-143,
    pseudo-first-margin-001..004).
11. **`@page name:pseudo` without whitespace** — Given `@page a:first`, when
    parsed, then the rule matches named page `a` on its first page only, and
    its margin applies there
    (`paged_media.rs::named_page_pseudo_without_whitespace_parses` — CORE-143,
    pseudo-first-margin-002).
12. **Cascade layers order `@page` rules** — Given `@layer a, b;` with `@page`
    rules inside both layers, when rendered, then the later-declared layer's
    margins win regardless of source position, and an unlayered `@page` beats
    both (`paged_media.rs::cascade_layers_order_page_margins` — CORE-143,
    layers-001..004).
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
16. **Named-page margin-box suppression** — Given `@page { @top-left { content:
    "HDR"; } }`, `@page cover { @top-left { content: none; } }`, a cover box
    with `page: cover; break-after: page;`, and a following box with
    `page: auto`, when rendered, then the cover page carries no "HDR" text and
    the following page does (`paged_media.rs::named_page_margin_box_suppression`).
    Also: a document whose FIRST element carries `page: <name>` activates the
    named page on page 1 (`paged_media.rs::named_page_first_page_activates`).
17. **Margin-box centering** — Given `@top-center` content wider than the
    middle-third slot, when rendered, then every page's head center sits on
    the content midline ±0.5pt and inside the top margin band
    (`paged_media.rs::margin_box_center_aligns_on_content_midline`).
18. **Alpha forms parse** — Given `#f008`, `#ff000088`, `rgba(255, 0, 0,
    0.5)`, `rgba(100% 0% 0% / 50%)`, `rgb(0 0 255)`, `rgba(0, 0, 0, 0)`,
    `rgba(1, 2, 3)`, `rgb(1, 2, 3, 0.5)`, `RGB(1 2 3)`,
    `rgb(100% 0 0 / 50%)`, and `transparent`, when parsed, then each yields
    the exact expected `Color` (alpha 0x88/128/0 as applicable; alpha-less
    forms opaque; `rgba()` is an alias of `rgb()` per css-color-4 §7.1,
    function names are ASCII case-insensitive, and the modern space grammar
    mixes number and percentage channels)
    (`alpha_color.rs::hex_alpha_forms`, `alpha_color.rs::rgb_function_forms`,
    `alpha_color.rs::transparent_keyword_is_fully_transparent_black`).
19. **Invalid color declarations are rejected** — Given `#f00zz`,
    `rgba(255, 0, 0, garbage)`, `rgba(255, 0, 0, NaN)`, `rgb(255, 0, 0, 0.5,
    123)`, `rgb(255 0 0 123)`, and `rgb(100%, 0, 0)`, when parsed, then every
    one fails (`None`) — no silent truncation, no opaque-on-error alpha, no
    filtered hex rescue, and the legacy comma grammar still requires one
    uniform channel kind
    (`alpha_color.rs::invalid_declarations_are_rejected`,
    `alpha_color.rs::legacy_mixed_kinds_still_rejected`).
20. **Invalid alpha preserves cascade fallback** — Given `@page {
    background: blue; background: rgba(255, 0, 0, garbage); }`, when
    rendered, then the page box background is the earlier valid blue
    (`alpha_color.rs::invalid_alpha_preserves_cascade_fallback`).
21. **Alpha survives canvas propagation** — Given an opaque blue `@page`
    background and a `#f008` body background, when laid out, then the page
    box fill is opaque blue and the propagated canvas background is
    `rgba(255, 0, 0, 0x88)`
    (`alpha_color.rs::semitransparent_body_background_keeps_alpha_in_canvas_propagation`,
    `alpha_color.rs::opaque_backgrounds_default_to_opaque_alpha`).
22. **Raster probe** — Given `probe/core153_alpha/probe_alpha.py` run with
    the main checkout's `.venv/bin/python` against a built engine binary,
    when executed, then every composited pixel case (page-box-002 shape,
    alpha 0, nested overlap) prints OK and the script exits 0.
    Command from this worktree's root:
    `/Users/elijah/workspace/typeanvil/.venv/bin/python probe/core153_alpha/probe_alpha.py engine/target/debug/typeanvil`.

23. **Margin-box sizing** — Given the `@page` declarations of
    `css-page-3 §5.3.2`'s own worked example (left box min 4em / max 17em,
    right box min 2em / max 5em, 20em available), when the two boxes are sized,
    then they come out 15.375em and 4.625em and fill the display width
    (`margin_box::tests::distributes_between_mins_when_max_overflows`).
24. **Margin-box fixed dimension** — Given a declared height with both margins
    auto, when the band is solved, then the box is centered in the band; given
    an auto height, then it fills what the margins leave; given declared
    height and both margins, then the over-constrained rule drops the ignored
    edge's margin
    (`margin_box::tests::fixed_dimension_centers_with_auto_margins`,
    `margin_box::tests::fixed_dimension_auto_height_fills_the_band`,
    `margin_box::tests::fixed_dimension_overconstrained_drops_the_ignored_edge`).
25. **Margin-box paint** — Given `@page { margin: 4em; @top-left { background:
    hotpink; content: "" } }`, when rendered, then the box's border box covers
    its resolved share of the page area and the fill is visible, with the
    content placed inside the content box.
26. **Page box grows to a declared area** — Given `@page { margin: 6em;
    width: 20em; height: 16em }` on a 5in×3in CLI default, when rendered, then
    the page box is 32em×28em and the page area is 20em×16em, so a reference
    that declares the same 32em box renders at the same size.

27. **Margin-box image content** — Given
    `@page { margin: 0; margin-top: 50px; @top-left { content: "Ti "
    url(green.png) } }` and a 100x50 PNG, when laid out, then one image
    fragment is produced at 75pt x 37.5pt (the image's intrinsic size) with a
    positive x (after the text) and inside the 37.5pt top margin band
    (`images.rs::margin_box_content_url_paints_intrinsic_image`).
28. **Margin-box background image** — Given
    `@page { margin: 0; margin-top: 50px; @top-center { content: "";
    border: 2px solid blue; background: url(green.png) } }` and a 100x50 PNG,
    when laid out, then the top-center box carries a BackgroundImage fragment
    as its FIRST child, sized to the whole border box (360pt x 37.5pt) at
    zero offset, with the image's natural 75pt x 37.5pt tile; the border
    fragment follows it (paint order: colour < image < border), and the PDF
    embeds the image exactly once
    (`images.rs::margin_box_background_image_tiles_over_border_box`).
29. **Margin-box multi-line content (CORE-178)** — Given
    `@page { size: 400px; margin: 100px; @top-left-corner { white-space:
    pre-wrap; content: "Line 1\aLine 2"; width: 100px; height: 100px;
    background: green } }`, when laid out, then the box's font size leaves the
    corner box oversized for one line and it paints TWO text runs whose
    baselines differ by exactly one `line-height`; the escape is processed
    rather than painted as the literal characters `\` and `a`
    (`layout.rs::core178_tests::pre_wrap_content_paints_two_lines`). Given the
    same content WITHOUT a break-preserving `white-space`, then ONE run is
    produced and the newline is rendered as a space
    (`core178_tests::normal_white_space_collapses_the_newline`).
30. **Page-context `white-space` inheritance (CORE-178)** — Given
    `@page { white-space: pre-wrap; @top-left { content: "a\a b" } }`, when
    laid out, then the box inherits the page context's value and paints two
    runs; the same rule with no `white-space` anywhere paints one
    (`core178_tests::page_context_white_space_inherits`).
31. **Margin-box CSS escapes (CORE-178)** — Given a string token containing
    `\a`, a `\41` hex escape, an escaped quote and a `\` line continuation,
    when the paged parser reads it, then it unescapes to a newline, `A`, the
    quote, and nothing respectively (css-syntax-3 §4.3.7)
    (`paged.rs::unescapes_css_string_escapes`).
32. **Margin-box paint order and `z-index` (CORE-179)** — Given the 16 margin
    boxes declared in a non-clockwise order, when the page spec resolves, then
    each box carries its own css-page-3 §3.1 slot and its declared `z-index`
    (an undeclared `z-index` is 0), and no margin box fragment sits in the
    content tree (`margin_box_paint_order.rs::paint_order_is_clockwise_from_top_left_corner`,
    `::margin_boxes_attach_with_their_stacking_key`,
    `::z_index_is_parsed_in_the_margin_context_and_defaults_to_auto`). Given a
    page-anchored out-of-flow box with `z-index: -1`, when the page is laid
    out, then its fragment precedes the in-flow content fragment in the page
    root; with an auto `z-index` it follows it
    (`::negative_z_index_abspos_attaches_before_the_in_flow_content`,
    `::zero_z_index_abspos_still_attaches_after_the_in_flow_content`). Given
    `css/css-page/margin-boxes/paint-order-003-print.html` in the WPT gate,
    when the full suite runs, then the pair matches — it is the fixture that
    measures a negative-`z-index` margin box staying behind the document
    background.

## Edge Cases

- **No `@page` rule matches** → CLI geometry is used; no margin boxes.
- **Named page rule missing** → falls back to the default page spec.
- **`page: auto` on a fresh box** → resets the page context to the default page
  (the letterhead cover flow: cover page suppressed, letter pages normal).
- **No box starts fresh at a page top** (content continues from the previous
  page) → the previous page's named context carries.
- **`@page` rule with no `size`** → inherits the default/CLI size.
- **`string()` referenced before any assignment on page 1** → empty string.
- **`target-counter` target on the same page as the TOC entry** → its page
  number is used; no infinite loop (pass 2 converges or the 3-pass cap hits).
- **Heading without an `id`** → bookmark still emitted (outline targets the
  page, not a fragment anchor); TOC link without a matching id → the entry
  resolves to "?" or is left empty (deterministic choice, documented in code).
- **Margin-box text wider than its box** → clipped deterministically, no
  soft wrap (lines break only at newlines the resolved `white-space`
  preserves).
- **`leader('.')` with no room** → no fill characters; line ends normally.
- **Deep heading nesting (h1–h6)** → outline depth capped at 6; h6 is the
  deepest node.
- **Anonymous/box fragments with `source: None`** → never targeted by
  `target-counter` and never bookmarked.
- **`counter-reset: page` on an element that starts mid-page** → takes effect
  at the page the element's box starts.
- **Two-pass convergence failure** (page numbers shift between passes) → cap
  at 3 passes, use the last result; documented in code as a known limitation.
- **Background color alpha (CORE-153, css-color-3/4)** → CSS colors carry an
  alpha channel through the whole pipeline: parse (`#rgba`, `#rrggbbaa`,
  `rgb()`/`rgba()` in comma and space syntax, the `transparent` keyword =
  fully transparent black) → stylo computed color → fragment paint → PDF
  fill opacity (krilla `Fill.opacity`, the PDF `ca` graphics-state
  parameter). Alpha is never pre-blended: a semitransparent fill composites
  at paint time over whatever is actually beneath it (the `@page` fill for
  the canvas background, the page/canvas for element boxes). Colors parsed
  without an alpha component are opaque (alpha 255). Migration: the `a`
  field is required, so pre-CORE-153 `Color { r, g, b }` struct literals no
  longer compile; opaque construction is `Color::rgb(r, g, b)` (no Rust
  field-default syntax). Invalid color declarations are rejected at parse
  time (malformed hex tokens, non-finite numbers, wrong channel arity,
  mixed channel kinds in legacy comma syntax, invalid alpha), so the
  cascade keeps the earlier valid declaration (css-syntax-3). The engine
  shall preserve alpha through each supported color parse and paint path.
  This change leaves broader color syntax support and whitespace-bearing
  function colors in the manual `border` shorthand parser for later work.
  The canvas
  propagation of a semitransparent body/html background keeps that alpha
  (page-box-002: opaque blue `@page` under a `#f008` body paints
  violet `rgb(136, 0, 119)`, not red, not blue).
- **Canvas background propagation (CORE-144)** → the html (else body)
  background paints the CANVAS over the page CONTENT area, under all content
  but above the `@page` box fill (so page margins keep the page box's own
  background); the donor box paints none of its own. Vertical percentage
  `@page` padding resolves against the page HEIGHT.
- **`@page` logical margins/padding map per writing mode (CORE-153)** → the
  `margin-inline-*` / `margin-block-*` longhands and their `padding-*`
  counterparts stay SYMBOLIC through the `@page` parser and map to physical
  edges at resolution time against the page context's EFFECTIVE writing
  mode (css-writing-modes-1 logical properties): the cascaded
  `@page { writing-mode }` wins, else the page context inherits the ROOT
  element's computed mode (css-page-3 §3). Under vertical-rl, inline
  percentages resolve against the page HEIGHT and block percentages against
  the WIDTH, with inline-start = top, inline-end = bottom, block-start =
  right, block-end = left (page-box-008/009; their refs simulate the
  margins with border widths 16/32/48/80 top/right/bottom/left). Document
  content still lays out horizontal-tb (CORE-127's orthogonal-flow
  suppression); only the page context's margin/padding geometry is
  writing-mode aware. Zero WPT status flips on landing — page-box-008/009
  remain FAIL on separate residuals (vertical-rl block geometry, a
  declared-height ref fragmentation bug) — but the margin bands now match
  the refs' border simulation exactly. Vertical-LR keeps LTR block
  anchoring (its block axis runs left→right; page-margin-003's ref).
  This session's composition (CORE-153, 2026-09-14): `:root { writing-mode }`
  is read via the engine's OWN cascade pass (stylo's servo build does not
  compute writing-mode for `:root` selectors — the paged pass previously
  never matched `:root` at all); a block in a vertical writing mode fills
  its INLINE axis (auto height = the page content box, page-size-012); and
  a vertical-rl definite-width box with `margin-right` anchors at the RIGHT
  edge (page-margin-002's ref). page-box-008/009, page-margin-002 and
  page-size-012 land through this composition.
- **`sideways-rl` / `sideways-lr` are vertical page-context modes (CORE-181,
  2026-09-14)** → both are valid `writing-mode` values (css-writing-modes-3
  §3.1) with a HORIZONTAL block axis, so the @page logical margin/padding
  mapping and the orthogonal-flow predicate must treat them as vertical. They
  parsed as horizontal in BOTH places: the `@page` parser mapped them to
  `None`, and the author paged-props pass fell through to `HorizontalTb`
  while still setting `writing_mode_declared`. `sideways-rl` shares
  `vertical-rl`'s axes; `sideways-lr`'s inline axis runs bottom-to-top, so its
  inline pair is REVERSED (inline-start = the physical BOTTOM). This landing
  flipped ZERO WPT statuses — the two fixtures that depend on sideways parsing
  (`body-background-slr/srl`) still fail on root-vertical page progression,
  which CORE-181 owns — so the seam is proven by unit tests instead:
  `paged::tests::logical_margins_map_sideways_modes` (the logical→physical
  edge arithmetic for both modes) and
  `layout::core153_vertical_rl_tests::sideways_modes_are_vertical_page_flows`
  (the root's mode resolves through the engine's own cascade pass, and
  block-start anchoring follows the block axis: sideways-rl right-anchors like
  vertical-rl, sideways-lr runs left-to-right like vertical-lr). Both were
  proven RED with the mapping reverted.
- **Element `content: url()` paints as replaced content (CORE-183, 2026-09-15)** →
  an element whose `content` carries a `url()` image is REPLACED content
  (css-content-3 §2): the declaration replaces the element's own children and
  the image paints at its natural size (96dpi px → pt, the same rule the `<img>`
  path uses). Chromium agrees and does NOT fall back to the element's own text —
  the harness oracle's PDF for `firefox-bug-2026295-print` has an EMPTY text
  layer on every one of its pages, while page 1 carries the in-flow `<h6>`'s
  image as ink. Two seams had to be wired: the image is interned in
  `collect_image_sources` (the pre-pass that handles `<img>`), and
  `is_replaced_image` routes such an element through `layout_image`.
  Supporting this also needed a second fix: author-CSS declaration bodies now
  split on TOP-LEVEL `;` only (`split_top_level_decls`, which already existed for
  the `src` list) in both the stylesheet pass and `parse_inline_decls`. A naive
  `split(';')` cut `content: url(data:image/png;base64,...)` at `;base64,`, so
  the value arrived as `url(data:image/png` — an unbalanced path that interned as
  a BROKEN 0x0 image and painted nothing.
  **GIF is still not decoded** (`images::sniff` covers PNG, JPEG and SVG), and
  the `firefox-bug-2026295-print` fixture's images are GIFs, so that target still
  FAILS; GIF support is tracked as **CORE-186** (it blocks CORE-183). Zero WPT status
  flips on landing, so the seam is proven by two RED-first unit tests
  (`layout::core183_element_content_image_tests::element_content_url_paints_image_at_natural_size`,
  `css::core183_decl_split_tests::data_uri_in_content_survives_inline_declaration_splitting`)
  plus a before/after ink control: the same PNG markup painted 0 ink before and
  64 after.
- **Vertical block geometry is keyed on the mode IN EFFECT AT THE BOX (CORE-182,
  2026-09-15)** → the CORE-153 block-start anchoring rule was keyed on the ROOT
  element's mode, so an INTERIOR `writing-mode` declaration was ignored for
  geometry. It now reads `Ctx::writing_mode_at(id)` (the nearest
  ancestor-or-self declaration, else the page flow mode), which is what
  css-writing-modes-3 §7 inheritance means: a nested `vertical-rl` div inside a
  HORIZONTAL page flow anchors from the right edge, because its own block axis
  runs right-to-left.
  This landing flipped ZERO WPT statuses (159 PASS / 124 FAIL on both sides,
  A/B diffed PER TEST ID), so the seam is proven by a unit test instead:
  `layout::core153_vertical_rl_tests::core182_interior_vertical_declaration_right_anchors`,
  shown RED with the re-key reverted (`x=0 want 285`). Zero flips is EXPECTED,
  not evidence of a dead seam: the WPT print-reftests compare our test render
  against our reference render through the SAME engine, so a change that applies
  uniformly to both sides of a pair is invisible to the gate. The payoff for this
  capability arrives with CORE-181's root-vertical page PROGRESSION, which is
  what the eight `body-background-*` / `block-00{1,2}-wm-*` / `page-box-008`
  targets actually fail on.
  **The CORE-153 inline-extent fill was deliberately NOT re-keyed the same way.**
  Widening it to the box's own mode was built and measured, and it regressed the
  Chromium-verified page count of the `page-name-orthogonal-writing-003` shape:
  an interior `vertical-rl` wrapper under a horizontal page filled its inline
  axis (210pt of a 216pt page) and pushed its second child onto a new page, 1 →
  2 pages (`tests/page_boundaries.rs::page_change_suppressed_when_inner_mode_orthogonal_to_page_flow`).
  That rule models "the box fills its CONTAINING BLOCK's inline size"; under a
  vertical page flow the fragmentainer is that containing block for the
  root/body chain, but an interior vertical box inside a horizontal flow has a
  content-based, indefinite containing-block inline extent. Generalising it needs
  a definite containing-block inline size, which the engine only models for
  DECLARED extents (`specified_extent`, CORE-167) — so the fill keeps its
  page-flow key and the NOTE in `layout_box` records why. The WPT gate could not
  catch this class of change at all: both sides of the pair moved together.
- **UA body margin is the WHATWG 8px in print (CORE-153, 2026-09-14)** → the
  UA sheet's `body { margin: 0 }` (Prince alignment, CORE-92) is REVERSED to
  `margin: 8px` (6pt): Chromium applies the 8px body margin in print, and
  the page-box-002/003 refs simulate it with an inner `margin: 8px` div, so
  css-standards-alignment (spec wins over PrinceXML) decides. The `//`
  line-comments in UA_CSS previously poisoned stylo's rule stream, so even
  the OLD `body { margin: 0 }` was never parsed (the initial 0 matched by
  accident). Flips: page-box-002/003 FAIL→PASS; layers-003 and
  page-name-margin-001 PASS→FAIL — both are CORE-177 bucket-3 invalid-test
  candidates (Chrome AND Firefox fail them), so the flips EXPOSE test bugs
  rather than regress.
- **Border shorthands apply 1-4 value widths per side (CORE-153,
  2026-09-14)** → `border-width: 40px 80px 120px 160px` (and the `border`
  shorthand) previously collapsed every side to the LAST length token;
  page-box-007's ref renders its 400x800px simulation box with all borders
  160px. The shorthand now expands per css-backgrounds-3 §4.3 (1→all,
  2→vertical/horizontal, 3→top/h/bottom, 4→top right bottom left).
- **html-root-box (CORE-165)** → the `<html>` element's own border paints as
  page chrome: a band at the page-area rect's edge on every page (the root
  box fragments per page; its border repeats per fragment). The root's
  background still propagates to the canvas (CORE-144) — never double-painted.
- **Page chrome insets content (CORE-165)** → document content lays out
  inside the `@page` border+padding bands AND the html root's border/padding
  (the body's containing block is the html content box; Chromium-verified:
  page-box-011 oracle text at border+padding offset). The chrome bands paint
  at the full page-area rect's edges; a degenerate rect (margins taller than
  the page) clamps chrome to 0. Border+padding no longer merge: the border
  band keeps its own width and the padding band its declared size
  (css-box-3 §3: border box ⊃ padding box ⊃ content box — superseding
  CORE-144's border-eats-padding model).
- **`@page` border currentColor (CORE-165)** → an omitted `border-color`
  resolves at used-value time to the page's cascaded `color`
  (css-backgrounds-3 §3; page-box-005: `@page :first { color: orange }`
  colors page 1's border).
- **Block content respects its border (CORE-165)** → a block's content box
  starts after its border widths, not just its padding (css-box-3 §3;
  oracle-verified: bordered div text at border+padding offset). inner_width
  already subtracted the borders; the content origin now matches.
- **`position: fixed` under canvas propagation** → the fixed clone attaches
  after the page's body content, and the canvas fill of the FINAL pass is
  the only one emitted (multi-pass TOC/counter documents produce one
  fragmentainer set per pass; earlier passes are discarded before emit, so
  the canvas fill can never paint over the fixed clones).
- **Page-anchored abspos fragments inside, continuations drain (CORE-153)**
  → a page-anchored `position: absolute` box (auto insets, no positioned
  ancestor, page content box as containing block) that FITS the remaining
  fragmentainer still lays with a real bottom limit (not the monolithic
  `f64::MAX`): a forced `break-before: page` child or natural page-bottom
  overflow slices the fragment and returns an outgoing token (css-break-3
  fragments abspos across pages — page-margin-004). The continuation is
  enqueued as an abspos drain job with its resume token; the body does NOT
  carry a deferred child token (the drain runs after the body, so a carry
  would keep paging past the drain's final page — a blank trailing page).
  When no in-flow content follows, the keepalive drives the drain-only
  page; a later body with real content skips the box via the
  drain-owns-it guard (pending job or resume token). Verified: 
  page-margin-004 flips PASS (Chromium 2 pages / 0 px diff) and
  monolithic-overflow-013/027 flip PASS. fixedpos-001/002 flip FAIL —
  EXPOSED accidental passes: our test now paginates the 300vh abspos
  (Chromium-exact at 3 pages); our REF still renders 1 page because the
  pinned inset-abspos model (`bottom: -100vh`) never advances the page
  loop. The ref-pagination of inset-anchored abspos is a documented
  residual, not a defect of this change.

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
