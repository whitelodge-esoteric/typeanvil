# TypeAnvil Showcase — print-resolution renders

Realistic-size pages (US Letter @ 300 DPI) rendered by the TypeAnvil
engine only — no Prince comparison. Rebuild with
`scripts/build-showcase.sh`. Generated pages live below the marker; this
preamble is hand-maintained.

## Known issues in these renders (engine gaps, tracked on Linear)

- **Images inside table cells do not paint.** A letterhead logo or signature
  image placed in a `<td>` renders as blank space, and the cell collapses so
  the text beside it shifts. The invoice fixture works around it with a
  block-level image (`display: block`).
  [CORE-210](https://linear.app/whitelodge/issue/CORE-210).

Three issues previously listed here are fixed. Each was re-checked against the
renders in this directory, not taken on trust:

- **Academic Sample column/footnote collision** — gone. No overlapping text
  lines remain on page 1. [CORE-150](https://linear.app/whitelodge/issue/CORE-150)
  (commit `203fef8`).
- **Technical Report config block** — `white-space: pre` is now honored, so
  the config renders as its six separate lines instead of one run-on line.
  [CORE-151](https://linear.app/whitelodge/issue/CORE-151) (commit `b322697`).
- **Line-break (`<br>`)** — breaks the line now; a three-`<br>` probe renders
  four lines.
  [CORE-149](https://linear.app/whitelodge/issue/CORE-149) closed as a
  duplicate of CORE-159. The poster and book fixtures still carry their
  block-span workarounds, which render correctly either way.

The bar-chart bars-outside-plot issue (first/last bar crossing the axis,
2026-09-08) was an asset-generation bug, fixed in
`assets/generate.py` — charts are regenerated and correct.

<!-- BEGIN GENERATED SHOWCASE -->

## Showcase — print-resolution renders (US Letter @ 300 DPI)

Realistic-size pages rendered by the TypeAnvil engine only (commit `5803952`). The side-by-side comparison gallery lives in [the comparison README](../corpus/README.md) and runs at 5in × 3in @ 96 DPI so diffs stay cheap; these pages show the same engine at the geometry documents actually print at. Prince renders only the comparison pipeline — the showcase is a TypeAnvil output gallery, not a diff target.

### Book Sample

*Exercises:* paged-media-css, cross-references, fragmentation-core

<img src="out/images/book/page-001-ta.png" alt="Book Sample — page-001-ta" width="420">
<img src="out/images/book/page-002-ta.png" alt="Book Sample — page-002-ta" width="420">
<img src="out/images/book/page-003-ta.png" alt="Book Sample — page-003-ta" width="420">
<img src="out/images/book/page-004-ta.png" alt="Book Sample — page-004-ta" width="420">
<img src="out/images/book/page-005-ta.png" alt="Book Sample — page-005-ta" width="420">
<img src="out/images/book/page-006-ta.png" alt="Book Sample — page-006-ta" width="420">

### Invoice & Remittance Advice

*Exercises:* paged-media-css, tables-fragmentation, fragmentation-core, images

<img src="out/images/invoice/page-001-ta.png" alt="Invoice & Remittance Advice — page-001-ta" width="420">
<img src="out/images/invoice/page-002-ta.png" alt="Invoice & Remittance Advice — page-002-ta" width="420">
<img src="out/images/invoice/page-003-ta.png" alt="Invoice & Remittance Advice — page-003-ta" width="420">

### Academic Sample

*Exercises:* typography-layer, fragmentation-core, footnotes, cross-references

<img src="out/images/journal/page-001-ta.png" alt="Academic Sample — page-001-ta" width="420">
<img src="out/images/journal/page-002-ta.png" alt="Academic Sample — page-002-ta" width="420">
<img src="out/images/journal/page-003-ta.png" alt="Academic Sample — page-003-ta" width="420">
<img src="out/images/journal/page-004-ta.png" alt="Academic Sample — page-004-ta" width="420">
<img src="out/images/journal/page-005-ta.png" alt="Academic Sample — page-005-ta" width="420">

### Rich Media Print

*Exercises:* images, paged-media-css

<img src="out/images/poster/page-001-ta.png" alt="Rich Media Print — page-001-ta" width="420">

### Technical Report

*Exercises:* paged-media-css, fragmentation-core, tables-fragmentation, images

<img src="out/images/report/page-001-ta.png" alt="Technical Report — page-001-ta" width="420">
<img src="out/images/report/page-002-ta.png" alt="Technical Report — page-002-ta" width="420">
<img src="out/images/report/page-003-ta.png" alt="Technical Report — page-003-ta" width="420">
