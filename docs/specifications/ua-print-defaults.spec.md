---
title: Print-Adapted UA Defaults
slug: /specifications/ua-print-defaults
type: spec
status: draft
owner: elijah
created: 2026-08-20
updated: 2026-09-04
sidebar_position: 12
tags: [engine, ua-stylesheet, prince-parity, css-break, css-page]
spec_id: ua-print-defaults
issue_id: CORE-95
applies_to: engine 0.x
dependencies: [fragmentation-core, paged-media-css, typography-layer]
---

# Print-Adapted UA Defaults

## Overview

The engine's UA stylesheet (`engine/src/css.rs` `CascadeSession::UA_CSS`) was
a copy of the HTML4/WHATWG **screen** defaults. In print those defaults are
wrong: `body { margin: 8px }` inset every line (fixed by CORE-92), and the
em-based heading sizes and margins scale with body font-size instead of
matching a print engine's fixed defaults. Prince 16.2 ships its actual UA
sheet at `lib/prince/style/html.css`; this spec makes the engine's UA block
defaults match that sheet and adds the css-break-3 margin-truncation rule
that makes the two render identically at page starts.

The observable bug being fixed (CORE-93 triage driver 1): an unstyled `<h1>`
at the top of a page pushed content down ~27pt (0.67em × 2em × body font)
and could defer a float past a page boundary by a few points — Prince
renders the same heading flush with the content top because (a) its UA
heading margins are fixed points, and (b) margins adjoining a fragmentainer
start are truncated to zero.

## Goals / Non-Goals

**Goals**

- UA heading sizes and margins match Prince 16.2 `html.css` exactly: fixed
  points, independent of body font-size.
- UA paragraph/list/quote margins match Prince: `1.12em` top/bottom,
  `blockquote` side margins `22.5pt`, `ul`/`ol` `padding-left: 40pt`.
- The first in-flow box on a fragmentainer KEEPS its top margin at a page
  start. **Refined (CORE-153):** css-break-3 §3.1 truncates only a
  fragmented box's CONTINUATION top margin at a fragmentainer edge; a fresh
  box at a page start (document start, after a forced break, or after
  natural pagination) applies its margin. Chromium — the WPT oracle —
  behaves this way (page-box-006, page-left-right-001/002 flip PASS with
  the change; the suite gates clean). This supersedes the CORE-95
  Prince-matching truncation: the CSS specification wins over PrinceXML
  (docs/conventions/css-standards-alignment.md). Parent-child margin
  collapse (body margin + first child margin) is still not modeled —
  the two margins are handled independently (documented deviation).
- Floats and abspos boxes do not consume "first in-flow" status: a paragraph
  after a top-of-page float still truncates (verified vs Prince 16.2).
- Determinism unchanged: identical input → byte-identical PDF.

**Non-Goals**

- Full margin collapsing (css-box-3 sibling/parent collapsing). The engine
  still adds adjacent margins; collapsing is tracked separately.
- Truncation of the LAST box's bottom margin at a fragmentainer end. Only
  top-of-fragmentainer truncation ships here.
- `style=""` attribute support in the stylo cascade (only the paged/breaks
  passes read it); not part of this spec.
- Margin truncation inside table cells (tables are a separate layout path
  and do not apply margins today).

## Behavior

The engine SHALL implement the following, stated as "shall" rules:

1. **Fixed heading sizes.** `UA_CSS` SHALL declare `h1 { font-size: 24pt }`,
   `h2 { font-size: 18pt }`, `h3 { font-size: 14pt }`, `h4 { font-size:
   12pt }`, `h5 { font-size: 10pt }`, `h6 { font-size: 8pt }` — fixed
   points, not `em` values. An unstyled heading SHALL render at the same
   size regardless of the body font-size (verified: Prince h1 is 24pt at any
   body size).
2. **Fixed heading margins.** `UA_CSS` SHALL declare heading margins `16pt
   0`, `15pt 0`, `14pt 0`, `16pt 0`, `16.5pt 0`, `21pt 0` for h1..h6
   respectively (Prince 16.2 `html.css` values).
3. **1.12em block margins.** `p`, `blockquote`, `pre`, `ul`, `ol` SHALL
   carry `margin-top: 1.12em; margin-bottom: 1.12em`; `blockquote` SHALL
   additionally carry `margin-left/right: 22.5pt`; `ul`/`ol` SHALL carry
   `padding-left: 40pt`.
4. **Top-of-fragmentainer truncation.** A fresh (break-before) box that is
   the first IN-FLOW box on a fragmentainer SHALL have its `margin-top`
   truncated to zero (css-break-3 §4). Truncation applies to UA and author
   margins alike (it is a fragmentation rule, not a UA rule).
5. **Floats/abspos do not consume first-in-flow.** A float or absolutely
   positioned box placed before the first in-flow box SHALL NOT clear the
   first-in-flow state; the following in-flow box still truncates.
6. **Columns are fragmentainers.** Each multicol column SHALL truncate its
   first in-flow box's top margin, and a multicol container that is itself
   first-in-flow on the page SHALL truncate its own top margin.
7. **Body margin stays zero.** CORE-92's `body { margin: 0 }` SHALL remain.
8. **Bold table headers.** `UA_CSS` SHALL declare `th { font-weight: bold }`
   (Prince 16.2 `html.css` line 482; browsers' UA sheets use `bolder`).
   Without it, header cells measure ~10% narrower than Prince's, the frozen
   header column lands ~0.4pt too narrow, and borderline rows ("Rocket-powered
   …") wrap to an extra line — the table-stress 43→45 page lever (CORE-96).
9. **Comment hygiene.** `UA_CSS` SHALL use `/* */` block comments only.
   `//` line comments are not valid CSS and can poison stylo's rule stream
   for every rule that follows (hit on this ticket).

## Interfaces

**UA stylesheet** — `engine/src/css.rs` `CascadeSession::UA_CSS` (the
`h1..h6`, `p`, `blockquote`, `pre`, `ul`, `ol` rules). Reference values:
Prince 16.2 `lib/prince/style/html.css` (shipped at
`/opt/homebrew/lib/prince/style/html.css`).

**Layout seam** — `engine/src/layout.rs` `layout_box` gains a
`first_in_flow: bool` parameter (alongside the existing
`page_has_content`). It starts `true` for the root on every fragmentainer;
is cleared when any in-flow content (line, block, generated content) is
placed; floats and abspos pass it through unchanged. The margin rule:

```rust
let margin_top = if fresh && first_in_flow {
    Scalar::ZERO
} else if fresh {
    style.margin_top
} else {
    Scalar::ZERO
};
```

The same flag threads through `layout_table_like`/`layout_table_cell`
(fall-through only; table cells pass `false`) and
`layout_multicol_container`/`fill_columns`/`fill_one_column` (each column
starts fresh).

## Acceptance Criteria

Each maps to a real test in `engine/tests/` or a committed probe:

1. **Top-of-page truncation** — Given `<h1>` as the first element of a bare
   document at demo geometry, when laid out, then the first baseline equals
   content top + half-leading (no 16pt margin), matching Prince within
   0.1pt (`ua_heading_margin_truncates_at_page_top`).
2. **Fixed heading sizes** — Given `body { font-size: 8pt }` with an
   unstyled `<h1>`, when laid out, then the h1 still measures 24pt
   (`ua_heading_font_sizes_are_fixed_pt`).
3. **1.12em paragraph margin mid-page** — Given two unstyled paragraphs
   where the first is author-zero-margined, when laid out, then the second
   paragraph's baseline sits 13.44pt below the first line box
   (`ua_paragraph_margin_is_1_12em_mid_page`).
4. **h6 fixed 21pt margin mid-page** — Given an h6 after a zero-margined
   paragraph, when laid out, then the h6 margin is 21pt, not 2.33em × 8pt
   (`ua_h6_margin_is_21pt_mid_page`).
5. **Prince parity end-to-end** — Given the probe corpus (h1..h6, p,
   blockquote, pre, ul, ol at page top + an h1 mid-page), when rendered
   through both engines, then baselines and margin-tops match Prince within
   tolerance (`probe/core95_ua.py`, committed; PASS).
6. **Zero WPT regression** — Given the full WPT print-reftest run
   (css-page + css-break + css-multicol, ~281 tests), when the new binary is
   compared against the pre-change binary on the same checkout, then
   fixed − regressed ≥ 0 and regressed == 0.
7. **Fixture self-containment** — No engine test may depend on the UA
   paragraph/heading margins for its pagination; fixtures that need a top
   margin declare it explicitly (`avoid_moves_block` pins `p { margin: 12pt
   0 }` for this reason).

## Edge Cases

- **Mid-page headings keep their margin.** Truncation applies ONLY at
  fragmentainer starts; an h1 after a paragraph renders with its full 16pt
  margin (Prince-verified: mid-page baseline gap 44.65 both engines).
- **Float at page top.** A top-of-page float does not clear first-in-flow;
  the paragraph after it truncates (Prince-verified).
- **Empty blocks.** A block that places no in-flow content does not clear
  the flag (approximates margin-collapsing-through empty boxes).
- **Table cells excluded.** Cell content keeps declared margins; a
  `<table>` as the first box does not truncate (tables apply no margins
  today; out of scope).
- **Multicol columns.** Each column is a fresh fragmentainer; the first
  box of column 2+ truncates even when the container placed content in
  column 1.
- **`pre` font substitution.** The probe's `pre` row compares different
  system monospace fonts between engines; its margin-top parity is the
  signal, not its absolute baseline.

## References

- Prince 16.2 UA sheet: `/opt/homebrew/lib/prince/style/html.css`
  (`h1..h6` fixed sizes/margins, `p`/`blockquote`/`pre`/`ul`/`ol` 1.12em).
- css-break-3 §4 "Adjoining margins at breaks" — margin truncation at
  fragmentainer boundaries.
- CORE-92 (body margin 0, the sibling ticket), CORE-93 (triage that
  attributed the float page-count gap to UA screen margins), CORE-94
  (line-breaking parity, tracked separately).
- Probe methodology: `probe/core92_margin.py` (first-baseline measurement),
  this ticket's `probe/core95_ua.py`.
