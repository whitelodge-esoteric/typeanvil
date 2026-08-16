---
title: WPT-Based Conformance Harness for Typeanvil (HTML/CSS→PDF)
type: research
status: approved
owner: elijah
created: 2026-08-14
updated: 2026-08-16
sidebar_position: 1
tags: [wpt, harness, conformance, testing]
---

# WPT-Based Conformance Harness for Typeanvil (HTML/CSS→PDF)

## 1. How WPT structures layout & paged-media tests

WPT test types relevant to a layout engine:

- **Reftests** — a test page plus one or more reference pages linked via `<link rel="match">` (or `rel="mismatch">`). Pass = pixel-identical rendering (with optional fuzz). This is the dominant type for layout ([WPT reftest docs](https://web-platform-tests.org/writing-tests/reftests.html)).
- **Print-reftests** — reftests rendered *paginated* and compared page-by-page. Identified by a `-print` suffix immediately before the extension (`bar-print.html`) or by living under a `print/` directory. Default page box is **5in × 3in with 0.5in margins**; `<meta name=reftest-pages content="-2,4,6-">` selects which pages to compare; fuzzy matching applies per page ([WPT print-reftest docs](https://web-platform-tests.org/writing-tests/print-reftests.html)).
- **testharness.js tests** — JS-driven (parsing/CSSOM tests, e.g. `css/css-page/parsing/*`). Require a JS runtime; mostly skippable for a layout engine, though the parsing tests encode valuable grammar facts.
- **Crashtests** — load without crashing/hanging.

Coverage of the paged-media directories (counted from the GitHub tree, Aug 2026):

| Directory | HTML files | `*-ref*` files | print-named |
|---|---|---|---|
| `css/css-page` | 367 | 156 | **333** |
| `css/css-break` | 640 | 78 | 39 |
| `css/css-multicol` | 744 | 204 | 8 |

`css/css-page` is almost entirely print-reftests (page size, `@page` margins, margin-boxes/, page selectors, `cssom/`, `parsing/`). `css/css-break` covers fragmentation (`break-before/after/inside`, subdirs for flex/grid/table) — most are ordinary reftests exercising fragmentation via fixed-height multicol containers, plus ~39 true print tests. `css/css-multicol` is the largest and mostly screen reftests. Also relevant: `css/css-gcpm/` (generated content for paged media), `css/CSS2/` (normal flow, floats, margins — thousands of reftests), `css/css-fonts/`, `css/css-text/`. wpt.fyi shows 258 tests in `css/css-page` alone in current stable-browser runs ([wpt.fyi/results/css/css-page](https://wpt.fyi/results/css/css-page)).

## 2. Reftest mechanics, and how a PDF engine participates

Mechanics ([reftest docs](https://web-platform-tests.org/writing-tests/reftests.html)):
1. Render test and reference at identical viewport; screenshot both.
2. Compare pixel-for-pixel. `rel=match` passes iff identical; `rel=mismatch` iff different.
3. **Fuzzy matching**: `<meta name=fuzzy content="maxDifference=15;totalPixels=300">` allows per-channel deltas and a bounded count of differing pixels (ranges like `10-15;200-300` supported, per-reference prefixes allowed). References chain (a reference may itself have references, forming AND/OR trees).

How wptrunner does **print**-reftests today (this is the exact model Typeanvil should copy): in `tools/wptrunner/wptrunner/executors/`, `PrintProtocolPart.render_as_pdf(width, height)` invokes the WebDriver classic `print` command (5in×3in page, 0.5in margins, `background=True`, `shrink_to_fit=False`) returning base64 PDF; `pdf_to_png(pdf_base64, ranges)` then rasterizes each page (wptrunner does it with a pdf.js-based runner page) and the standard `RefTestImplementation` hashes/compares page images ([executorwebdriver.py](https://github.com/web-platform-tests/wpt/blob/master/tools/wptrunner/wptrunner/executors/executorwebdriver.py), [protocol.py](https://github.com/web-platform-tests/wpt/blob/master/tools/wptrunner/wptrunner/executors/protocol.py)).

For Typeanvil: implement the same two primitives — `typeanvil render test.html --page-size 5inx3in --margin 0.5in -o out.pdf`, rasterize with **pdftoppm (Poppler)**, **PDFium** (`pdfium_render`), or **MuPDF/mutool draw** at a fixed DPI (Chromium prints at 96 CSS px/in; rasterize both sides at the same DPI), then per-page pixel compare honoring `<meta name=fuzzy>` and `<meta name=reftest-pages>`. Note Firefox went a different way — its print reftests can compare *parsed PDF structure* rather than pixels ([Firefox reftest docs](https://firefox-source-docs.mozilla.org/layout/Reftest.html)) — a useful secondary oracle (text runs, link rects) but pixels are the primary signal.

## 3. How Chromium tests printing/pagination

- Chromium runs WPT reftests/print-reftests through `run_web_tests.py` (blinkpy); `blinkpy/web_tests/port/base.py` has explicit `is_wpt_print_reftest()` detection and drives `image_diff` with fuzzy parameters parsed from `<meta name=fuzzy>` ([base.py](https://chromium.googlesource.com/chromium/src/+/main/third_party/blink/tools/blinkpy/web_tests/port/base.py)). Reference tests are "pixel-by-pixel comparison" of test vs reference page ([writing_web_tests.md](https://chromium.googlesource.com/chromium/src/+/master/docs/testing/writing_web_tests.md)).
- Pagination is implemented as **LayoutNG block fragmentation**: layout produces a physical *fragment tree*; printing traverses fragments. LayoutNG printing shipped in Chrome 108 (tracked in [crbug 1121942](https://bugs.chromium.org/p/chromium/issues/detail?id=1121942), which shows Chromium engineers improving/adding WPT pagination reftests as part of the work; deep-dive: [RenderingNG fragmentation](https://developer.chrome.com/docs/chromium/renderingng-fragmentation)). Chromium's own regression coverage for pagination is largely *these same WPT print-reftests* plus internal web tests under `web_tests/printing/` and C++ unit tests of the fragmentation code — i.e., Chromium itself validates printing mostly via the harness you're about to build.

## 4. Prior art: non-browser engines

- **WeasyPrint** (closest analog): pytest suite in `tests/`; *draw tests* render documents to per-page pixel buffers and assert against small ASCII-art pixel expectations or compare PNGs (failures dump PNGs; Ghostscript required for PDF checks; failures reported as "pixel (x,y) expected rgba… got rgba…") ([contribute docs](https://doc.courtbouillon.org/weasyprint/stable/contribute.html), [issue #1643](https://github.com/Kozea/WeasyPrint/issues/1643)). Hand-written, not WPT-derived — a scale limitation Typeanvil can beat.
- **Typst**: single integration suite; test snippets (`--- name attrs ---`) compiled per stage (`render`/`pdf`/`svg`/`html`/`pdftags`), compared against committed reference PNGs (≤20 KiB each) or **hashed references** (32-byte hashes in `ref/{format}/hashes.txt`, live outputs regenerable via `cargo testit regen`), with an HTML failure report containing image diffs ([typst tests/README.md](https://github.com/typst/typst/blob/main/tests/README.md)). The hashed-reference + HTML-report pattern is directly reusable.
- **Prince**: no public automated conformance suite; claims are a per-spec support matrix ([princexml.com/doc/css-refs](https://www.princexml.com/doc/css-refs/)) plus the famous distinction of being the first non-browser to pass **Acid2** ([howtocreate.co.uk/acid](https://www.howtocreate.co.uk/acid/)).

## 5. Harness design for Typeanvil

**Build order: harness first.** Concretely:

1. **Vendor WPT** (sparse checkout of `css/`), parse `MANIFEST.json` (`./wpt manifest`) to enumerate tests, types, and reference links — don't crawl HTML yourself; the manifest already resolves `rel=match/mismatch`, fuzzy meta, and print-reftest classification.
2. **Subset selection**: start with all `print-reftest` entries plus reftests under `css/css-page`, `css/css-break`, `css/css-multicol`, `css/css-gcpm`, `css/CSS2/normal-flow|floats|margin-padding-clear`, `css/css-text`, `css/css-fonts`. Exclude `testharness` tests (need JS) except as a frozen list of parsing expectations; exclude tests whose source contains `<script`, animations, or interactivity (grep at ingest; WPT layout reftests are overwhelmingly script-free). Run crashtests as "doesn't panic."
3. **No JS runtime needed**: reftests are declarative HTML+CSS. Serve files with WPT's server semantics or just resolve relative paths from disk; honor WPT's `/fonts/ahem.css` — **install Ahem**, the metrics-exact test font virtually all layout reftests depend on.
4. **Engine interface (shaped by the harness)**: `typeanvil render <input.html> --page-width/height/margins --fonts-dir --base-url -o out.pdf` — deterministic, offline, fixed font set. This CLI *is* the public API v0.
5. **Compare pipeline**: render test.pdf and ref.pdf → `pdftoppm -r 96 -png` (or PDFium) → per-page compare with fuzzy budget → artifacts: side-by-side PNGs + diff heatmap in an HTML report (steal Typst's report + hashed-reference idea to keep the repo lean).
6. **Chromium as oracle** for the many reftests where *both* sides are meaningful (and for triage): `chrome --headless=new --print-to-pdf=ref.pdf --no-pdf-header-footer test.html` (or CDP `Page.printToPDF` with exact 5×3in page via `--print-to-pdf` flags / chromedriver's WebDriver `print`). Two modes: (a) strict WPT semantics — Typeanvil(test) vs Typeanvil(ref); (b) cross-engine — Typeanvil(test) vs Chromium(test) with a generous fuzz budget, useful early when Typeanvil can't yet render the reference either. Mode (a) is the real conformance score; mode (b) is the agent's gradient signal.
7. **Scoring/tracking (wpt.fyi-style)**: emit `wptreport.json` (wptrunner's format) per run; store per-test PASS/FAIL/CRASH history in SQLite; dashboard = pass-rate per directory over time + newly-fixed/newly-broken lists per commit. Gate CI on "no regressions" with an expectations file (à la Chromium's `TestExpectations` / wpt `.ini` metadata) so known-fails don't block.

**Word of caution**: pixel-compare across engines is noisy (font raster, AA). Mitigations: Ahem-based tests first, fuzzy budgets per test, and Firefox-style structural PDF assertions (page count, text extraction via `pdftotext`, MediaBox sizes) as a cheap, stable second oracle.
