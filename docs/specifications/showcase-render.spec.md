---
title: Showcase Render
slug: /specifications/showcase-render
type: spec
status: draft
owner: elijah
created: 2026-09-08
updated: 2026-09-15
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

- Five showcase fixtures that demonstrate the wedge at real print geometry:
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
  5. **Invoice and remittance advice** — business document at print
     geometry: a letterhead mark, a 56-row line-item table that fragments
     across a page boundary with its header row repeated on the continuation
     page, a totals block, and remittance / payment-terms sections. This is
     the invoice half of the wedge ("reports and invoices") and it was
     previously absent from both demo pipelines.
- Output committed like the comparison gallery: per-page PNGs plus a
  markdown gallery promoted into `demo/showcase/README.md`, viewable
  directly on GitHub (separate file from the comparison gallery's
  `demo/README.md`).

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
   promote its body into `demo/showcase/README.md` below the
   `BEGIN GENERATED SHOWCASE` marker, rewriting image paths to
   `out/images/...` so they resolve from `demo/showcase/`. The comparison
   gallery's `demo/README.md` is never touched by the showcase build.
4. The build SHALL fail if any showcase render exits non-zero or produces
   zero pages (showcase output is prospect-facing; silent empties are worse
   than a red build).
5. Re-running the build on an unchanged tree SHALL produce byte-identical
   PNGs and index.md (no timestamps in the showcase output; determinism is
   the product promise).
6. The showcase build SHALL NOT change the comparison pipeline's
   MECHANICS: the same commands run, with the same exit codes, and the
   comparison build stays deterministic on an unchanged corpus. "Unaffected"
   does NOT freeze the corpus — adding a comparison fixture is a deliberate
   change specified by `visual-comparison-demo.spec.md` Behavior 1, and it
   legitimately moves the scoreboard. A showcase-only change SHALL NOT move
   the comparison scoreboard.

## Interfaces

- Build entry: `scripts/build-showcase.sh [--keep-work] [--dry-run]
  [--determinism]`.
- Manifest: `demo/showcase/manifest.json`, same entry schema as the
  comparison manifest (name, file, wedge_features, known_limitations,
  expected_deltas) minus the diff-specific fields.
- Gallery writer: `scripts/demo_compare.py assemble-showcase --manifest
  demo/showcase/manifest.json --images-dir demo/showcase/out/images --out
  demo/showcase/out/index.md` (new subcommand; reuses the markdown-writer
  style of the comparison gallery).
- Gallery promotion: the generated body is promoted into
  `demo/showcase/README.md` below a `BEGIN GENERATED SHOWCASE` marker —
  its own file, NOT `demo/README.md`, so showcase and comparison outputs
  stay separable (GitHub renders a README per directory). The comparison
  gallery owns `demo/README.md` exclusively; each build rewrites only its
  own file.
- Engine contract: unchanged (`typeanvil render <in.html> --page-width 8.5in
  --page-height 11in --margin-* 0.75in -o out.pdf`).

## Acceptance Criteria

- **AC1 (render)** — Given the built engine binary, When
  `build-demo.sh --showcase` runs, Then all five fixtures render, each
  producing a sane page count (≥ 3 pages for report/paper/book/invoice,
  ≥ 1 for rich-media), and the build exits 0. For the invoice, its
  line-item table SHALL span a page boundary with the header row repeated
  on the continuation page, and every page SHALL carry a running footer
  built from `counter(page)` and `counter(pages)`.
- **AC2 (raster)** — Given a rendered PDF, When rasterized, Then each page
  PNG exists under `demo/showcase/out/images/` and measures 2550 × 3301 px.
  (Letter @ 300 DPI is 2550 × 3300; the rasterizer rounds the height up by
  one pixel. The committed baseline has always been 3301, so this criterion
  now states the measured value.)
- **AC3 (gallery)** — Given a completed build, Then
  `demo/showcase/README.md` contains the generated showcase section with
  `<img>` tags whose `src` paths resolve from `demo/showcase/` on GitHub,
  and `demo/README.md` (comparison gallery) is untouched by the showcase
  build.
- **AC4 (determinism)** — Given two consecutive builds, Then the showcase
  output trees are byte-identical.
- **AC5 (no comparison impact from the showcase)** — Given a showcase-only
  change, When the comparison build runs, Then its outputs match a
  pre-change build. A comparison-corpus addition is out of this criterion's
  scope; it is verified by `visual-comparison-demo.spec.md`'s own acceptance
  criteria, and it does move the scoreboard by design.

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
- **Size after the invoice fixture (2026-09-15):** 18 pages @ 300 DPI,
  ≈ 8.1 MB committed (book 1.92, invoice 1.85, journal 2.31, poster 0.35,
  report 1.69). Still roughly 5× under the cap, so no page caps, no JPEG
  conversion, and no palette reduction were needed.
- **Engine facts this spec relies on** (verified during implementation):
  fixture-internal `@page { margin }` wins over CLI `--margin-*` (the
  poster renders full-bleed under shared CLI flags); `content: ... leader()
  target-counter()` pieces on TOC anchors require `display: block`;
  `string-set` accepts `content()` only (literal strings are ignored —
  running-head titles ride an inner span).
