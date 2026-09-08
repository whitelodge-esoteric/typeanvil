# TypeAnvil Showcase — print-resolution renders

Realistic-size pages (US Letter @ 300 DPI) rendered by the TypeAnvil
engine only — no Prince comparison. Rebuild with
`scripts/build-showcase.sh`. Generated pages live below the marker; this
preamble is hand-maintained.

## Known issues in these renders (engine gaps, tracked on Linear)

- **Academic Sample, page 1 — text collision at the column bottom.**
  Where the two-column section meets the footnote band, the columns fill
  against the full page height and a column-bottom heading overlaps the
  footnote text. [CORE-150](https://linear.app/whitelodge/issue/CORE-150).
- **Technical Report — the config sample block renders as one run-on
  line.** `white-space: pre` is not yet honored, so the 6-line config
  collapses. [CORE-151](https://linear.app/whitelodge/issue/CORE-151).
- Line-break (`<br>`) support: poster/book fixtures use block-span
  workarounds. [CORE-149](https://linear.app/whitelodge/issue/CORE-149).

The bar-chart bars-outside-plot issue (first/last bar crossing the axis,
2026-09-08) was an asset-generation bug, fixed in
`assets/generate.py` — charts are regenerated and correct.

<!-- BEGIN GENERATED SHOWCASE -->

## Showcase — print-resolution renders (US Letter @ 300 DPI)

Realistic-size pages rendered by the TypeAnvil engine only (commit `bb1991f`). The side-by-side comparison gallery lives in [the main demo README](../README.md) and runs at 5in × 3in @ 96 DPI so diffs stay cheap; these pages show the same engine at the geometry documents actually print at. Prince renders only the comparison pipeline — the showcase is a TypeAnvil output gallery, not a diff target.

### Book Sample

*Exercises:* paged-media-css, cross-references, fragmentation-core

<img src="out/images/book/page-001-ta.png" alt="Book Sample — page-001-ta" width="420">
<img src="out/images/book/page-002-ta.png" alt="Book Sample — page-002-ta" width="420">
<img src="out/images/book/page-003-ta.png" alt="Book Sample — page-003-ta" width="420">
<img src="out/images/book/page-004-ta.png" alt="Book Sample — page-004-ta" width="420">
<img src="out/images/book/page-005-ta.png" alt="Book Sample — page-005-ta" width="420">
<img src="out/images/book/page-006-ta.png" alt="Book Sample — page-006-ta" width="420">

### Academic Sample

*Exercises:* typography-layer, fragmentation-core, footnotes, cross-references

<img src="out/images/journal/page-001-ta.png" alt="Academic Sample — page-001-ta" width="420">
<img src="out/images/journal/page-002-ta.png" alt="Academic Sample — page-002-ta" width="420">
<img src="out/images/journal/page-003-ta.png" alt="Academic Sample — page-003-ta" width="420">

### Rich Media Print

*Exercises:* images, paged-media-css

<img src="out/images/poster/page-001-ta.png" alt="Rich Media Print — page-001-ta" width="420">

### Technical Report

*Exercises:* paged-media-css, fragmentation-core, tables-fragmentation, images

<img src="out/images/report/page-001-ta.png" alt="Technical Report — page-001-ta" width="420">
<img src="out/images/report/page-002-ta.png" alt="Technical Report — page-002-ta" width="420">
<img src="out/images/report/page-003-ta.png" alt="Technical Report — page-003-ta" width="420">
