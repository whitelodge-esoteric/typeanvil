---
title: Showcase Render
slug: /specifications/showcase-render
type: spec
status: draft
owner: elijah
created: 2026-09-08
updated: 2026-09-08
sidebar_position: 6
tags: [demo, showcase, print-resolution, gallery]
spec_id: showcase-render
issue_id: CORE-148
applies_to: demo 0.x
dependencies: [visual-comparison-demo, paged-media-css, fragmentation-core, images, footnotes, cross-references]
---

# Showcase Render

## Overview

The comparison demo (visual-comparison-demo.spec.md) renders at 5in × 3in
@ 96 DPI by design: the geometry matches the WPT harness so diff numbers are
comparable, small pages pack many page boundaries (fragmentation coverage),
and 96 DPI keeps diffs deterministic and cheap. That geometry is the right
instrument for the Prince diff pipeline, but it produces no realistic,
print-resolution pages to show prospects.

This spec adds an ADDITIVE showcase render target: realistic page sizes
(US Letter) at 300 DPI, rendered through TypeAnvil only. The comparison
pipeline is untouched. A Prince side-by-side is out of scope (the showcase
is not a diff target).

## Goals

- Four showcase fixtures that demonstrate the wedge at real print geometry:
  1. **Technical report** — narrative report with raster `<img>` chart
     figures at print resolution, headings, tables, running header/footer
     via `@page` margin boxes.
  2. **Academic sample** — single-column journal layout: abstract, justified
     prose with hyphenation, footnotes (`float: footnote`), section
     cross-references via `target-counter()`.
  3. **Book sample** — table of contents with leader fills and
     `target-counter(attr(href), page)` page numbers, a chapter opening,
     body pages with running heads via `string-set`, `counter(pages)`.
  4. **Rich media print** — full-color poster-style page: large raster
     imagery, bold display typography, expressive layout.
- Output committed like the comparison gallery: per-page PNGs plus a
  markdown section promoted into `demo/README.md`, viewable directly on
  GitHub.

## Non-Goals

- No Prince rendering, diffing, or scoring for showcase fixtures.
- No changes to the comparison pipeline's geometry, manifest, or scoreboard.
- SVG content in fixtures waits for CORE-131 to reach main (raster images
  only). When SVG lands on main, a showcase fixture MAY add an SVG figure.
- No new engine features. If a fixture exposes an engine gap, file an issue;
  the fixture either avoids the gap or records a known-limitation.

## Behavior

1. `scripts/build-demo.sh --showcase` SHALL render every fixture listed in
   `demo/showcase/manifest.json` through the TypeAnvil binary at US Letter
   (`8.5in × 11in`) with `0.75in` margins, from `demo/showcase/` as the
   process CWD (relative `url()`/`src` resolution matches the comparison
   pipeline's corpus-dir pattern; `--base-url` threading is CORE-140).
2. The renderer SHALL rasterize each render's pages to PNG at 300 DPI into
   `demo/showcase/out/images/<fixture>/page-NNN-ta.png`.
3. The build SHALL assemble `demo/showcase/out/index.md`, a markdown
   showcase document (fixture name, page images, manifest notes), and SHALL
   promote its body into `demo/README.md` below the generated-gallery
   marker, rewriting image paths to `showcase/out/images/...` so they
   resolve from `demo/`.
4. The build SHALL fail if any showcase render exits non-zero or produces
   zero pages (showcase output is prospect-facing; silent empties are worse
   than a red build).
5. Re-running the build on an unchanged tree SHALL produce byte-identical
   PNGs and index.md (no timestamps in the showcase output; determinism is
   the product promise).
6. The comparison pipeline (`scripts/build-demo.sh` without `--showcase`)
   SHALL be unaffected: same commands, same outputs, same exit codes.

## Interfaces

- Build entry: `scripts/build-demo.sh --showcase [--keep-work] [--dry-run]`.
- Manifest: `demo/showcase/manifest.json`, same entry schema as the
  comparison manifest (name, file, wedge_features, known_limitations,
  expected_deltas) minus the diff-specific fields.
- Gallery writer: `scripts/demo_compare.py assemble-showcase --results
  <dir> --manifest demo/showcase/manifest.json --out-dir demo/showcase/out`
  (new subcommand; reuses the markdown-writer style of the comparison
  gallery).
- Engine contract: unchanged (`typeanvil render <in.html> --page-width 8.5in
  --page-height 11in --margin-* 0.75in -o out.pdf`).

## Acceptance Criteria

- **AC1 (render)** — Given the built engine binary, When
  `build-demo.sh --showcase` runs, Then all four fixtures render, each
  producing a sane page count (≥ 3 pages for report/paper/book, ≥ 1 for
  rich-media), and the build exits 0.
- **AC2 (raster)** — Given a rendered PDF, When rasterized, Then each page
  PNG exists under `demo/showcase/out/images/` and measures 2550 × 3300 px
  (Letter @ 300 DPI).
- **AC3 (gallery)** — Given a completed build, Then `demo/README.md`
  contains the showcase section with `<img>` tags whose `src` paths resolve
  from `demo/` on GitHub.
- **AC4 (determinism)** — Given two consecutive builds, Then the showcase
  output trees are byte-identical.
- **AC5 (no comparison impact)** — Given the showcase additions, When the
  comparison build runs, Then its outputs match a pre-change build.

## Edge Cases

- **Repo size budget:** 300 DPI Letter pages are large. Budget: ≤ 40 MB
  committed total. Control levers, in order: page-count limits per fixture
  (book sample caps at the first ~6 pages), PNG quantization/optimization
  (pngquant-style palette reduction in the rasterizer step is acceptable —
  visually lossless at viewing size), JPEG for the rich-media fixture's
  full-color pages. Record the actual committed size in the spec's
  References when landing.
- **Fixture 1 chart images:** committed under `demo/showcase/assets/` with
  licenses; generated programmatically (deterministic script) rather than
  hand-drawn, so a rebuild can regenerate identical bytes.
- **Multi-byte/non-ASCII text** in fixtures is fine (CORE-83 fixed the
  mojibake class); fixtures SHOULD include some to keep the ToUnicode path
  exercised at print sizes.

## References

- CORE-148 (this feature), CORE-146 discussion (why 5×3@96 for the
  comparison), CORE-147 (markdown gallery promotion pattern).
- `docs/specifications/visual-comparison-demo.spec.md` — the comparison
  contract this spec extends.
- **Size budget decision (2026-09-08):** 13 pages @ 300 DPI total
  ≈ 6.2 MB committed (book 1.8, journal 1.9, poster 0.35, report 1.6) —
  far under the 40 MB cap; no JPEG conversion or page caps needed. PNG
  optimization (e.g. pngquant) is a future lever if the page count grows.
- **Engine facts this spec relies on** (verified during implementation):
  fixture-internal `@page { margin }` wins over CLI `--margin-*` (the
  poster renders full-bleed under shared CLI flags); `content: ... leader()
  target-counter()` pieces on TOC anchors require `display: block`;
  `string-set` accepts `content()` only (literal strings are ignored —
  running-head titles ride an inner span).
