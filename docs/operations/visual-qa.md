---
title: Visual QA — per-page checks for demo outputs
type: runbook
status: draft
owner: elijah
created: 2026-09-16
updated: 2026-09-16
sidebar_position: 4
tags: [demo, qa, visual, geometry]
issue_id: CORE-216
---

# Visual QA — geometry checks for rendered demo pages

`scripts/visual-qa.sh` runs four deterministic, geometry-based checks over a
track's rendered pages. It never uses a vision model: every verdict is
derivable from the PDF text layer, the raster pixels, or both. The checks
exist to catch page-level defects — overlaps, clipped content, dropped text,
and missing declared fills — as they land, not when a human happens to look.

## Run it

From the repo root (a built `engine/target/debug/typeanvil` binary is
required):

```bash
PY=~/workspace/typeanvil/.venv/bin/python bash scripts/visual-qa.sh \
  --track corpus   --report /tmp/qa-corpus.json
PY=~/workspace/typeanvil/.venv/bin/python bash scripts/visual-qa.sh \
  --track showcase --report /tmp/qa-showcase.json
```

Options:

| Flag | Meaning |
|---|---|
| `--track corpus\|showcase` | Which demo track to check (required) |
| `--report PATH` | Machine-readable JSON report (required) |
| `--keep-pdfs` | Keep the work dir (`<track>/out/.qa-keep-*`) instead of deleting it |
| `--dry-run` | Print the exact render commands without running them |

Exit code is 0 only when every fixture and every check passes. A failure
prints `FAILED: <fixture> <page> <engine> <check>` per violation, and the
report records the reasons.

## The four checks

1. **Ink-row overlap** — reads the PDF text layer (pypdfium2 char boxes),
   clusters chars into visual lines by baseline, and flags two lines whose
   boxes substantially overlap (>60% of the narrower line's width) with
   line centers within 60% of the shorter line's height. This is the
   CORE-150 class (text on text) and it is geometry-proven — a vision model
   produced two false "text collision" claims during CORE-209 that this
   geometry disproved. Duplicate text runs (clean + garbled copies of the
   same physical line; CORE-85-class text-layer artifacts) and side-by-side
   columns that merely touch are excluded, so legal margin boxes and
   two-column layouts do not fail.
2. **Page-box overflow** — flags ink within 2 px of the MEDIA BOX edge (the
   physical page). Margin boxes and a full-bleed poster legally live inside
   the page; only content escaping the page itself is overflow. A fixture
   whose `@page` rule declares zero margins (`margin: 0`) is treated as
   full-bleed and exempt.
3. **Text round-trip coverage** — every visible source token must appear in
   the PDF text layer (≥98% token coverage). Lowercase, strips punctuation
   and the engine's U+FFFE soft-hyphen markers, and splits digit/letter
   boundaries so glued runs ("1Contents") and hyphenation fragments match.
   Catches the silently-dropped/replaced-content class (CORE-83/85).
4. **Declared-fill color presence** — an element with a declared
   `background-color` must paint pixels of that color (within tolerance, at
   least an 8×8 region). Checked per fixture, not per page: a color may
   legally appear on whichever page its element lands on.

## Thresholds

| Parameter | Value | Where |
|---|---|---|
| Baseline cluster tolerance | 4.0 pt | `_pdf_text_line_boxes` |
| Overlap x-threshold | >60% of narrower line | `_overlaps` |
| Overlap center distance | <0.6 × shorter line height | `_overlaps` |
| Duplicate-run center distance | <3.0 pt → skip | `check_ink_row_overlap` |
| Overflow edge tolerance | 2 px from media edge | `check_page_box_overflow` |
| Round-trip coverage floor | 98% of source tokens | `check_text_roundtrip_coverage` |
| Fill tolerance | ±10 per channel, ≥64 px | `check_declared_fill_colors` |

## Determinism

The report is byte-identical across re-runs on an unchanged tree (no
timestamps; fixtures in manifest order; sorted pages/tokens). Both demo
tracks pass determinism today. The checks are side-effect-free and CI-shaped:
they only read PDFs, rasters, and sources, and write one JSON report.

## Verification

- Corpus track (8 fixtures × TA+PR) and showcase track (5 fixtures × TA)
  both pass all four checks at their committed state (2026-09-16).
- Deliberately broken fixtures (two absolutely-positioned text paragraphs
  overlapping; a declared color painted off-page) are detected by the
  overlap and declared-fill checks respectively.
- `python3 scripts/validate_docs.py` — OK (this runbook).