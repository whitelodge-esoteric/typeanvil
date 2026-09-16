---
title: Visual Comparison Demo
slug: /specifications/visual-comparison-demo
type: spec
status: draft
owner: elijah
created: 2026-08-17
updated: 2026-09-15
sidebar_position: 5
tags: [demo, comparison, prince, gallery, pipeline]
spec_id: visual-comparison-demo
issue_id: CORE-68
applies_to: demo 0.x
dependencies: [wpt-conformance-harness, fragmentation-core, paged-media-css, typography-layer, tables-fragmentation]
---

# Visual Comparison Demo

## Overview

The wedge's calling card: a browsable, static side-by-side gallery proving
TypeAnvil renders real documents like PrinceXML — the incumbent it targets.
Two audiences:

1. **Outbound** — prospects evaluating a Prince replacement need pixel-level
   proof on the wedge features TypeAnvil actually ships: paged-media CSS
   (`@page`, margin boxes, running headers, counters, TOC), fragmentation,
   typography (Knuth-Plass justification, hyphenation, protrusion), and
   fragmenting tables.
2. **Internal QA** — the pixel-diff scoreboard + triage surfaces genuine
   engine gaps as follow-up CORE-* issues, exactly like the WPT conformance
   baseline (CORE-60) did.

The demo is **measurement + presentation only**: no engine feature work
happens here. The engine already renders the wedge; this spec defines how we
*show* that.

**Scope boundary (decided on the parent CORE-67):** Prince is the only
comparison engine. Chromium headless is an explicit non-goal — it does not
support paged-media CSS, so it would only show the wedge gap, not the wedge
strength.

## Goals / Non-Goals

**Goals**

- A deterministic `scripts/build-demo.sh` that renders the corpus through
  both engines with **identical CLI flags**, rasterizes page-by-page, diffs,
  and writes a gallery into `demo/corpus/README.md` plus a scoreboard at
  `demo/corpus/out/scoreboard.json`. Every corpus-track artifact SHALL live
  under `demo/corpus/`, mirroring the showcase track's layout.
- A corpus of 6–8 real documents, each exercising a **shipped** wedge feature
  (CORE-51/52/53/61), self-contained (fonts/assets resolve via `--base-url`).
- Per-doc notes from a manifest: what the doc exercises, known TypeAnvil
  limitations, expected visual deltas vs Prince.
- A triage step (the closing child) that buckets every diff and files
  follow-up issues for genuine engine bugs.
- Reuse `harness/rasterize.py` + `harness/compare.py` — no new diff machinery.

**Non-Goals**

- Chromium headless comparison (explicit non-goal on CORE-67).
- Any engine feature work (floats/multicol/flexbox remain backlog CORE-62/63/65
  and may appear only as "known gap" notes, never as features we claim).
- A live/web-served gallery — the artifact is static files committed to the
  repo, openable from disk.
- WPT-style pass/fail semantics — the scoreboard is a **diff percentage +
  bucket**, not a conformance gate.

## Behavior

The demo SHALL implement the following, stated as "shall" rules:

1. **Corpus definition.** The corpus SHALL live in `demo/corpus/` as
   self-contained `.html` files plus `manifest.json`. Each fixture SHALL
   exercise at least one shipped wedge feature and SHALL render through both
   engines without crashing (a fixture that crashes either engine is a corpus
   bug, not a skip). Known-gap features SHALL be used minimally or not at all,
   with an explicit "known gap" note in the manifest.
2. **Manifest schema.** `demo/corpus/manifest.json` SHALL be a JSON array of
   doc objects, each with: `name` (display title), `file` (relative path),
   `wedge_features` (array of feature slugs from
   `fragmentation-core | paged-media-css | typography-layer |
   tables-fragmentation`), `known_limitations` (array of strings, may be
   empty), and `expected_deltas` (array of strings describing anticipated
   visual differences vs Prince, may be empty).
3. **Engine CLI contract.** Both engines SHALL be invoked with the identical
   flag set, matching the `typeanvil render` contract (CORE-60 harness spec):
   `<engine> <input.html> --page-width W --page-height H --margin-top MT
   --margin-right MR --margin-bottom MB --margin-left ML --base-url
   http://127.0.0.1:PORT/ -o <output.pdf>`. `scripts/render-prince.sh` SHALL
   satisfy this contract for Prince; `engine/target/debug/typeanvil render`
   is the TypeAnvil side.
4. **Rasterization.** Both PDFs SHALL be rasterized page-by-page with
   `harness/rasterize.py::rasterize_pdf(pdf, dpi=96)` (the same DPI the WPT
   harness uses, so page images are 5in × 3in → 480 × 288 px for default
   geometry). A page-count mismatch between engines SHALL be recorded in the
   scoreboard as `page_count_mismatch: true` on that doc (not a crash).
5. **Pixel diff.** Per page, the diff SHALL be computed with
   `harness/compare.py::compare(test_images, ref_images)` semantics: the
   absolute per-pixel channel difference, reported as a **diff percentage**
   (fraction of pixels differing beyond a small tolerance, 0–100). The
   TypeAnvil render is the *test* side, Prince is the *reference* side.
6. **Scoreboard schema.** `demo/corpus/out/scoreboard.json` SHALL be a JSON object:
   `{ "generated": <ISO timestamp>, "typeanvil_version": <git short sha>,
   "prince_version": <prince --version output or null>, "docs": [ { "name",
   "file", "typeanvil_pages", "prince_pages", "page_count_mismatch": bool,
   "pages": [ { "page": <1-based>, "diff_percent": <float> } ],
   "overall_diff_percent": <float> } ] }`.
7. **Bucket semantics.** The triage child (CORE-72) SHALL classify each doc's
   overall diff into exactly one bucket: `identical` (< 1% diff, no visible
   difference), `cosmetic` (1–20%, spacing/positioning/font-substitution
   differences only — no content missing or misplaced), `missing-feature`
   (content absent or structurally different because TypeAnvil lacks a CSS
   feature the doc uses), or `engine-bug` (content wrong in a way that is NOT
   explainable by a known missing feature). Only `engine-bug` produces a
   follow-up issue; `missing-feature` may produce one only when the gap is
   undocumented.
8. **Gallery interface.** The gallery SHALL be a GitHub-viewable markdown
   document, and it SHALL be the `README.md` of the corpus track itself
   (`demo/corpus/README.md`) — not a separate `index.md`, and not an HTML
   page. The generated body SHALL contain, per doc: a header (name + the
   manifest's wedge features, known limitations, and expected deltas), the
   two page sequences side by side in a markdown table (TypeAnvil left,
   Prince right) with the per-page diff percentage between them, a markdown
   scoreboard table (doc × pages × overall diff % + bucket), and the
   benchmark section. It SHALL reference the rasterized PNGs with relative
   `<img>` paths (`out/images/...`) so GitHub's file browser renders them
   without a build step. The build SHALL replace only the content below the
   `BEGIN GENERATED GALLERY` marker, leaving the hand-maintained preamble
   above it untouched.
   A build SHALL NOT emit an HTML gallery: the markdown README is the single
   committed gallery artifact (the HTML page and its inlined base64 images
   were dropped as redundant, superseded by this requirement).
9. **Determinism.** Re-running `scripts/build-demo.sh` on an unchanged tree
   SHALL produce byte-identical `demo/corpus/out/` (sorted iteration, no
   timestamps in image files; the only timestamp is `generated` in the
   scoreboard JSON).
10. **Failure isolation.** A render failure for one doc SHALL NOT abort the
    pipeline: the doc is marked `render_error: <stderr tail>` in the
    scoreboard, its page images show a placeholder, and the pipeline
    continues. A missing Prince binary SHALL abort with a clear message
    pointing at `demo/corpus/README.md` (install + license steps).
11. **License honesty.** `demo/corpus/README.md` SHALL document that this
    track uses Prince's free non-commercial license, comparison-only, with
    install + version-pin steps (CORE-69).

## Interfaces

**Scripts**

- `scripts/render-prince.sh <input.html> [same flags as typeanvil render] -o
  <out.pdf>` — Prince wrapper honoring the CLI contract; records
  `prince --version`.
- `scripts/build-demo.sh` — full pipeline: for each corpus doc → render both
  → rasterize → diff → write `demo/corpus/out/scoreboard.json` and splice the
  gallery into `demo/corpus/README.md`.
- `scripts/demo_compare.py assemble` — owns the README splice for BOTH tracks
  (`--readme`, `--marker`, `--image-prefix`), so the corpus and showcase
  galleries share one implementation and neither emits an intermediate
  `index.md`.
- `demo/corpus/README.md` — Prince install/license/version-pin documentation
  plus the generated gallery. `demo/README.md` is the short index that
  describes both tracks.

**Manifest shape** (`demo/corpus/manifest.json`):

```json
[
  {
    "name": "Invoice",
    "file": "invoice.html",
    "wedge_features": ["tables-fragmentation", "paged-media-css"],
    "known_limitations": ["border-spacing separate unsupported (collapse only)"],
    "expected_deltas": ["Prince may use a different default font"]
  }
]
```

**Scoreboard shape** (`demo/corpus/out/scoreboard.json`):

```json
{
  "generated": "2026-08-17T00:00:00Z",
  "typeanvil_version": "198b483",
  "prince_version": "16.1",
  "docs": [
    {
      "name": "Invoice",
      "file": "invoice.html",
      "typeanvil_pages": 3,
      "prince_pages": 3,
      "page_count_mismatch": false,
      "pages": [ { "page": 1, "diff_percent": 4.2 } ],
      "overall_diff_percent": 4.2,
      "render_error": null
    }
  ]
}
```

**Corpus fixture list** (CORE-70 authors these; names are illustrative):

- `invoice.html` — tables, repeating table headers, page numbers
- `report.html` — running headers/footers, page counters, TOC
- `paper.html` — footnote-heavy, justified + hyphenated prose
- `letterhead.html` — named pages, `@page` margin boxes
- `table-stress.html` — long tables fragmenting across pages
- `prose.html` — Knuth-Plass justification showcase (the differentiator)
- `invoice-statement.html` — business document: two fragmenting tables in one
  file (line items, then a running-balance statement of account), each
  repeating its header row per page, plus a margin-box running header and
  accented glyphs that keep the ToUnicode path exercised (CORE-209)

## Acceptance Criteria

Each maps to a real check in `scripts/build-demo.sh` or a committed artifact:

1. **Pipeline runs** — Given a clean checkout with Prince installed, when
   `scripts/build-demo.sh` runs, then it exits 0, writes the gallery into
   `demo/corpus/README.md`, and produces `demo/corpus/out/scoreboard.json`
   with an entry for every corpus doc (`build-demo` completes without
   aborting on any single-doc failure).
2. **Scoreboard validates** — Given the emitted scoreboard, when checked
   against the schema above, then every doc has `name/file/pages/diff` fields
   and `diff_percent` is a finite 0–100 float; `page_count_mismatch` is
   present on every doc (`validate-scoreboard` check in the script).
3. **Identical flags** — Given any corpus doc, when the two render commands
   are compared, then the page-geometry flags are identical between
   `typeanvil render` and `scripts/render-prince.sh` (asserted by the
   pipeline's command construction, visible in `--dry-run` output).
4. **Gallery is static and single** — Given `demo/corpus/README.md`, when
   viewed on GitHub or in a local markdown preview, then it renders both
   engines' pages side by side with per-page diff % and per-doc notes, and
   every image path resolves relative to the README. No `index.md` and no
   `index.html` SHALL exist under `demo/corpus/out/`.
5. **Determinism** — Given two consecutive pipeline runs on an unchanged
   tree, when the outputs are compared, then `demo/corpus/out/` is byte-identical
   except `scoreboard.json`'s `generated` field.
6. **Failure isolation** — Given a deliberately broken fixture (temporarily),
   when the pipeline runs, then it completes, the broken doc shows
   `render_error` + a placeholder image, and other docs are unaffected
   (`build-demo` continues past a failing doc).
7. **Self-check** — the spec file itself passes `scripts/validate_docs.py`.

## Edge Cases

- **Missing Prince binary** — pipeline aborts with a clear message;
  `demo/corpus/README.md` has the install + free-license steps (CORE-69's
  deliverable).
- **Page-count mismatch** — recorded in the scoreboard, not a crash; gallery
  shows both page sequences with the count noted.
- **Zero-diff tolerance** — sub-1% diffs from font substitution/antialiasing
  are `identical`, not noise: diff computed with the compare tolerance used by
  the WPT harness.
- **Font differences** — Prince and TypeAnvil may resolve different system
  fonts; the corpus uses `--base-url` + explicit `font-family` stacks to
  minimize this, and any residual is a `cosmetic` bucket, documented per doc.
- **Non-determinism** — if a re-run differs beyond `generated`, the pipeline
  fails loudly (byte-compare in the script) so a flaky fixture is caught, not
  silently committed.
- **Baseline freshness** — the committed scoreboard records the engine commit
  it was built from (`typeanvil_version`). Rebuilding after engine changes
  legitimately moves every doc's diff, because the corpus measures the engine
  against Prince rather than measuring the corpus against itself. A refresh
  SHALL record the new commit, SHALL state each doc's delta against the
  previous baseline, and SHALL NOT present a moved number as an improvement
  or a regression until the movement is attributed (char-box evidence per
  doc, both sides measured at baseline AND candidate).
- **UA defaults are Prince-parity, not HTML4-screen (CORE-92 + CORE-95)** —
  the engine's UA stylesheet matches Prince's print defaults: `body { margin:
  0 }` (CORE-92, probe 2026-08-20: Prince's first baseline = content top +
  half-leading exactly), fixed-point heading sizes/margins (h1 24pt/16pt …
  h6 8pt/21pt) and 1.12em paragraph margins (CORE-95, `ua-print-defaults`
  spec), plus css-break-3 top-of-fragmentainer margin truncation — the first
  in-flow box on every page/column renders flush with the content top. The
  old HTML4 em-based screen defaults pushed unstyled headings ~27pt down and
  flipped prose page counts. Corpus fixtures therefore inherit zero body
  margin; a fixture that needs a top margin must declare it explicitly (it
  will be truncated only when it is the first in-flow box on a page).

## Verification

1. `python3 scripts/validate_docs.py` — OK (this spec).
2. CORE-69: `scripts/render-prince.sh` renders a corpus fixture with the
   identical flag set as the engine; `demo/corpus/README.md` documents license +
   version pin.
3. CORE-70: all corpus fixtures render through both engines without crashing;
   `manifest.json` complete and committed.
4. CORE-71: the `demo/corpus/README.md` gallery + `scoreboard.json` build
   from a clean checkout; scoreboard validates; re-run byte-identical.
5. CORE-72: baseline gallery + scoreboard committed; every doc bucketed
   (identical/cosmetic/missing-feature/engine-bug); genuine engine bugs filed
   as follow-up CORE-* issues with gallery pages as evidence.
