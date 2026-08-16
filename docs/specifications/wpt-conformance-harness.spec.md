---
title: WPT Conformance Harness
type: spec
status: approved
owner: elijah
created: 2026-08-16
updated: 2026-08-16
sidebar_position: 1
tags: [harness, wpt, conformance, testing]
spec_id: wpt-conformance-harness
issue_id: CORE-49
applies_to: harness 0.x
dependencies: []
---

# WPT Conformance Harness

## Overview

The harness runs WPT (web-platform-tests) **print-reftests** against an
HTML/CSS → PDF engine and scores the engine's conformance to the CSS paged-media
specifications. For each test it renders the test page and its reference
through the *same* engine, rasterizes both PDFs at 96 DPI, and compares them
page-by-page with WPT fuzzy semantics. It records every run in SQLite, emits a
wpt-compatible `wptreport.json`, and can gate CI on "no regressions".

It is the first deliverable of Typeanvil: the test oracle built *before* the
engine. Chromium (via Playwright) is the built-in oracle; the engine itself
plugs in later through a documented CLI contract.

## Goals / Non-Goals

**Goals**

- Enumerate WPT paged-media print-reftests without needing WPT's tooling
  (direct scan of a sparse checkout, no `MANIFEST.json`).
- Render test + reference through the same engine and compare strictly
  (mode (a) from the research brief) — the real conformance score.
- Be the executable source of truth for regressions: every run recorded, gated.
- Stay lightweight: `fetch`, `score`, and `history` must work without
  Playwright installed; only `run --engine chromium` needs it.

**Non-Goals**

- Executing `testharness.js` tests (they need a JS runtime; skipped).
- Cross-engine mode (b) — Typeanvil(test) vs Chromium(test) with generous
  fuzz — is a future triage tool, not implemented.
- Full WPT manifest parsing, expectations files, or wpt.fyi-style dashboards.
- Crashtest looping beyond "an engine crash is an ERROR result".

## Behavior

The harness shall:

1. **Fetch**: `python -m harness fetch` sparse-clone the WPT paged-media
   directories into `.wpt/` and print the checkout path. `--force` re-checks
   out even when present.
2. **Guard**: `python -m harness run` shall exit 2 with a pointer to `fetch`
   when no WPT checkout exists (`--wpt` or the default `.wpt/`).
3. **Enumerate**: identify print-reftest candidates as HTML files that end in
   `-print.html`, live under a `print/` directory, or declare a
   `rel="match"`/`rel="mismatch"` link whose target exists.
4. **Parse metadata**: for each candidate, parse `rel` reference links
   (relative paths resolved against the test file, absolute `/...` paths
   against the WPT root), `<meta name="fuzzy">` tolerances (both `a-b` ranges
   and bare `n` forms, with optional per-reference `ref.html:...` prefixes),
   and `<meta name="reftest-pages">` page selection (`-2,4,6-` syntax).
5. **Exclude JS**: drop candidates whose source contains `<script`.
6. **Chain references**: follow references one level; a reference may itself
   carry a `rel` link. For `rel="match"` chains the comparison is AND — the
   reference must also match its own reference.
7. **Render**: render the test and each reference through the same engine and
   rasterize both PDFs at 96 DPI.
8. **Compare**: apply WPT fuzzy semantics page-by-page: `maxDifference` bounds
   the worst per-channel delta of any differing pixel; `totalPixels` bounds how
   many pixels may differ; both budgets are inclusive ranges where a too-small
   difference can also fail. A page-count mismatch is a FAIL unless
   `reftest-pages` selected a subset. `rel="mismatch"` passes iff the images
   differ *beyond* tolerance.
9. **Multi-ref**: a test with several `rel="match"` references shall PASS if
   any reference passes (OR); a lone `rel="mismatch"` reference's result is
   honored directly.
10. **Never abort**: engine crashes, timeouts, or any per-test exception shall
    produce an `ERROR` result and the run continues; a worker-process death
    likewise yields `ERROR` ("worker crash").
11. **Parallelize safely**: Playwright's sync API is not thread-safe, so
    parallelism is process-based (`ProcessPoolExecutor`, one browser per
    worker, default 4); `--workers 1` runs sequentially.
12. **Stay reproducible**: results shall be sorted by test id before reporting.
13. **Record**: each run shall write a wpt-compatible `wptreport.json`
    (`results[]` with `test`, `status`, `duration` in ms, and per-page
    `subtests`) and append one `runs` row plus per-test `results` rows to
    `history.sqlite`.
14. **Score**: `python -m harness score` prints the scoreboard from history
    (totals, pass rate, delta vs previous run). `--gate` exits 1 listing the
    first regressions when any test that passed in the previously recorded run
    now fails.
15. **History**: `python -m harness history` lists recorded runs with pass
    rates, newest first.
16. **Serve absolutely**: the Chromium engine shall serve the WPT checkout over
    `http://127.0.0.1:<port>/` (not `file://`), so absolute-path references
    like `/fonts/ahem.css` resolve; it shall wait for webfonts
    (`document.fonts.ready`) before printing and print at 5in × 3in with 0.5in
    margins, `print_background`, and `prefer_css_page_size=false`.
17. **Diff artifacts**: failed comparisons shall write a per-page triptych
    (test | reference | diff heatmap) under `artifacts/<test-id>/`.

## Interfaces

**CLI** (`python -m harness`):

| Command | Flags | Purpose |
|---|---|---|
| `fetch` | `--force` | sparse-clone WPT into `.wpt/` |
| `run` | `--engine {chromium,cli}` `--cli-cmd` `--filter` `--limit` `--workers` `--timeout` `--report` `--db` `--artifacts` | run a conformance pass |
| `score` | `--db` `--gate` | scoreboard; nonzero exit on regressions |
| `history` | `--db` `--limit` | list recorded runs |

Defaults: `--engine chromium`, `--workers 4`, `--timeout 30.0` (s),
`--report wptreport.json`, `--db history.sqlite`, `--artifacts artifacts`.

**Engine adapter contract** (what the future `typeanvil render` MUST satisfy):

```text
typeanvil render <input.html> \
    --page-width 5in --page-height 3in \
    --margin-top 0.5in --margin-right 0.5in \
    --margin-bottom 0.5in --margin-left 0.5in \
    --base-url http://127.0.0.1:PORT/ \
    -o <output.pdf>
```

Deterministic, offline, fixed page geometry. `PageSpec` defaults are the WPT
print-reftest geometry: 5in × 3in, 0.5in margins on all sides.

## Acceptance Criteria

Given/When/Then, each mapping to a real test in `tests/`:

1. **Reference resolution** — Given a print-reftest with a `rel="match"` link,
   when `parse_test` reads it, then the test id and its refs resolve correctly
   (`test_manifest.py::test_simple_match_ref_resolved`).
2. **Page selection** — Given `reftest-pages` values `"2"`, `"1,3,5"`,
   `"2-4"`, `"-2"`, `"6-"`, when parsed, then they expand to the correct page
   lists (`test_reftest_pages.py` — single, list, closed range, leading-open,
   trailing-open cases).
3. **Fuzzy comparison** — Given two page images, when compared, then identical
   images pass, differences within the fuzzy budget pass, differences beyond
   the budget fail, and `rel="mismatch"` passes only when images differ beyond
   tolerance (`test_compare.py` — identical / within-fuzzy / beyond /
   mismatch cases; `test_fuzzy.py` for budget-range semantics).
4. **Report shape** — Given a run's results, when `build_wptreport` runs, then
   the output matches the wpt report shape with per-page subtests
   (`test_report.py::test_wptreport_shape`).
5. **History + gate** — Given two recorded runs, when the second run's results
   regress a previously-passing test, then `gate` reports the regression and
   fails (`test_report.py` — history recording and regression-gate cases).
6. **Self-check** — the spec file itself passes `scripts/validate_docs.py`
   (frontmatter conforms to `docs/conventions/frontmatter-schema.md`).

## Edge Cases

- **No WPT checkout** → `run` exits 2 with a pointer to `fetch`.
- **Test contains `<script>`** → excluded at enumeration (no JS runtime).
- **Page-count mismatch** → FAIL, unless `reftest-pages` selected a subset.
- **Too-small differences** → fail when a fuzzy range is authored as an
  expectation that differences *should* exist (both-ends semantics).
- **Multiple match refs** → OR; first passing ref wins.
- **Chained ref fails** → the whole comparison fails with
  `chained ref failed: ...`.
- **Engine crash / timeout / any exception** → `ERROR`, run continues.
- **Worker dies** → `ERROR` (`worker crash`), run continues.
- **Filter matches nothing** → an empty run is recorded, not an error.
- **Missing fuzzy per-ref key** → falls back to the test-level fuzzy budget.

## References

- Research brief: `docs/research/wpt-harness/typeanvil-wpt-harness-brief.md`
  (WPT mechanics, print-reftest model, harness design).
- Code: `harness/` (`cli.py`, `runner.py`, `manifest.py`, `compare.py`,
  `engine.py`, `rasterize.py`, `report.py`, `wpt_fetch.py`).
- Tests: `tests/` (manifest, compare, fuzzy, reftest-pages, report).
- Origin issue: CORE-49 (WPT print-reftest conformance harness).
- WPT docs: [reftests](https://web-platform-tests.org/writing-tests/reftests.html),
  [print-reftests](https://web-platform-tests.org/writing-tests/print-reftests.html).
