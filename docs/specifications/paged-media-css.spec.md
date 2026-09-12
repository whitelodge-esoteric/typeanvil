---
title: Paged-Media CSS
slug: /specifications/paged-media-css
type: spec
status: draft
owner: elijah
created: 2026-08-16
updated: 2026-09-12
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
