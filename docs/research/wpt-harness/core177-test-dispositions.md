---
title: CORE-177 Test Dispositions — the 29 Chrome-Failing WPT Targets
type: research
status: approved
owner: elijah
created: 2026-09-14
updated: 2026-09-14
sidebar_position: 3
tags: [wpt, conformance, triage, chrome, core-177]
---

# CORE-177 Test Dispositions — the 29 Chrome-Failing WPT Targets

Companion to [the ground-truth sweep](./wpt-ground-truth-sweep-2026-09.md).
Every test where wpt.fyi records a Chrome failure was re-rendered locally in
Chromium (Playwright, WPT print geometry: 5×3in, 0.5in margins) as both test
and reference sides, then compared pixel-level at 96 DPI. Verdicts below are
from that render, the fixture sources, and upstream trackers.

## Method

- Local oracle: `harness.engine.ChromiumEngine` (the harness's own Chromium),
  `PageSpec.wpt_default()`, rasterized with pypdfium2 at 96 DPI.
- Comparison: per-page differing-pixel count (channel delta > 2) and ink
  bounding boxes. Data: `/tmp/core177-oracle.json`, `/tmp/core177-pixdiff.json`.
- Upstream checks: wpt.fyi master runs (rev `f2d99507a2`), WPT issue search,
  csswg-drafts issue search.

## Key corrections to the sweep

The sweep classified by wpt.fyi status alone. Local rendering corrects it:

1. **The four `body-background-*` tests are VALID, not invalid.** Firefox
   "fails" them only because it does not run print-reftests. Chrome nearly
   passes them locally (page 1 exact on three of four; residuals ≤ 2,696 px
   on page 2 only — gradient stop alignment). They are Chrome minor bugs,
   and the spec-correct engine behavior (canvas gradient propagation under
   root writing modes) stays a valid CORE-155 target.
2. **Three "accidental passes" are actually clean.** `page-box-003`,
   `overflowed-abs-pos-with-percentage-height`, and
   `root-element-display-none` render pixel-identical (0 diff) on local
   Chromium. The wpt.fyi canary failures do not reproduce — version skew
   between Playwright's Chromium and the canary build (or runner flags).
   Do not count these as pre-paid regressions.
3. **`fixedpos-with-abspos-with-link` is pixel-clean locally too** (all 3
   pages 0 diff). Its wpt.fyi failure is suspected in the LINK annotation
   geometry (pixels cannot see link rects). Verify with a PDF annotation
   dump before treating it either way.

## Disposition table

Diffs are differing pixels per page (96 DPI); `p1 0` means page 1 exact.

### Bucket 3a — valid tests, Chrome bug (engine may target; spec-first)

| Test | Local Chromium | Verdict | Action |
|---|---|---|---|
| body-background-slr | p1 0, p2 2696 | Chrome minor bug (gradient stop alignment on page 2) | Keep as CORE-155 target; spec = canvas bg propagation |
| body-background-srl | p1 1152, p2 1536 | Chrome minor bug | same |
| body-background-vlr | p1 0, p2 1200 | Chrome minor bug | same |
| body-background-vrl | p1 0, p2 1200 | Chrome minor bug | same |
| layers-003 | p1 158, p2 0 | Chrome near-pass (1px layer offset p1) | Not engine work; note only |
| page-name-margin-001 | p1 0, p2 0, p3 158 | Chrome near-pass (1px on :right page) | Not engine work; note only |
| single-line-column-flex-065 | p1 0, p2 16959 | Chrome bug (flex item fragment height) | VALID test; Firefox passes it; engine may target after Chrome-side clarity |
| single-line-row-flex-045 | p1 0, p2 16959 | Chrome bug (same class) | same |
| single-line-column-flex-069b | p1 0, p2 21011, p3 extra | Chrome bug (spurious 3rd page) | same |
| float-with-large-margin-bottom-001 | p1 9216, p2 12496 | Chrome bug (float margin fragmentation) | Firefox passes; engine may target |
| table-fragmentation-003a/b | p1 4608, p2 4608, p3 0 | Chrome bug/ambiguity (row bg continuation pages 1–2) | csswg-clarity candidate; Firefox passes |
| table-fragmentation-003c/d | p1 9216, p2 9216, p3 0 | same class | same |

### Bucket 3b — broken/disputed tests (do NOT tune engine to them)

| Test | Local Chromium | Verdict | Action |
|---|---|---|---|
| margin-boxes/dimensions-013 | 1 page both sides, 32200 px diff | Test broken in Chrome: margin-box layout diverges grossly from ref simulation | **Test-bug candidate** (no upstream issue exists; we do not file — documented divergence only) |
| margin-boxes/dimensions-014 | same, 32452 px | same | same issue |
| page-name-zero-height-001 | t=6 pages vs ref=3 | **Disputed upstream**: author filed [wpt#57480](https://github.com/web-platform-tests/wpt/issues/57480) — "cannot find this in any spec" | Divergence pending upstream; reference #57480 |
| tentative/safe-printable-inset-001 | 62600 px | Unimplemented csswg **proposal** ([PR #13190](https://github.com/w3c/csswg-drafts/pull/13190), `page-margin-safety`) | Not a bug; revisit when proposal lands |
| tentative/safe-printable-inset-002 | 32800 px | same | same |
| tentative/safe-printable-inset-003 | 45600 px | same | same |

### Bucket 4 — our passes re-examined

| Test | Local Chromium | Verdict |
|---|---|---|
| page-name-flex-001/002/004 | ref side renders 1 page (Chrome's known ref-mechanism gap) | **Confirmed accidental**: our ref side collapses the same way (t=4/r=1 etc.). Exposed when ref-side flex+named-page simulation lands. Keep flagged. |
| page-name-abspos-002 | t=2, ref=1 in Chrome | Confirmed accidental (same shape) |
| page-background-003 | p1 26, p2 12 | Near-pass (anti-aliasing-scale diffs); Chrome minor |
| overflowed-abs-pos-with-percentage-height | 0 diff both pages | **Not accidental** — wpt.fyi canary fail does not reproduce locally |
| root-element-display-none | 0 diff (blank vs blank, 1 page) | **Not accidental** — same |
| page-box-003 | 0 diff | **Not accidental** — same |
| fixedpos-with-abspos-with-link | 0 diff all 3 pages | Pixel-clean; suspected link-annotation-only failure. Verify annotations before engine work |

## Upstream signals observed (no filings — we do not file upstream bug reports)

- **dimensions-013/014** — no existing upstream issue was found; noted here
  only. Per policy we do not file WPT/csswg bug reports for these.
- **`page` on flex items** — the Chrome-vs-Firefox split behind
  flex-065/069b/045 and the page-name-flex-* ref gap looks like an open
  spec question (does the `page` property apply between flex items?);
  noted as a csswg-tracker question to monitor, not to file.
- Already tracked upstream (reference only, filed by others):
  [wpt#57480](https://github.com/web-platform-tests/wpt/issues/57480)
  (zero-height named pages, by the fixture author).

## Scoreboard footnote (standing)

Quote pass rates as **"153/283 (54.1%); Chrome baseline 254/283 (89.8%)"** —
never the raw number alone. Of Chrome's 29 failures: ~12 are Chrome minor
bugs on valid tests, 8 are broken/disputed/tentative tests, 4 do not
reproduce on local Chromium, and the rest need deeper characterization.
