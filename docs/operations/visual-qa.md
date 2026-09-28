---
title: Visual QA
type: runbook
status: draft
owner: maintainers
created: 2026-09-16
updated: 2026-09-27
sidebar_position: 4
tags: [demo, qa, visual, geometry]
trigger: When checking rendered demo output for page-level defects
---

# Visual QA

`scripts/visual-qa.sh` renders a demo track and runs deterministic geometry checks over its PDFs. The checks use PDF text, raster pixels, and source tokens.

## Prerequisites

- A built `engine/target/debug/typeanvil` binary.
- Python dependencies used by the harness.
- Prince installed when checking the `corpus` track.

## Steps

From the repository root:

```bash
bash scripts/visual-qa.sh --track corpus --report /tmp/qa-corpus.json
bash scripts/visual-qa.sh --track showcase --report /tmp/qa-showcase.json
```

Set `PY=/path/to/python` when the default interpreter is not suitable:

```bash
PY=/path/to/python bash scripts/visual-qa.sh \
  --track corpus --report /tmp/qa-corpus.json
```

Options are `--track corpus|showcase`, required `--report PATH`, `--keep-pdfs`, `--check-pr`, and `--dry-run`. The script exits 0 only when rendering and all checks pass.

## Checks

1. Ink-row overlap detects substantially overlapping text lines.
2. Page-box overflow detects ink at the physical media-box edge.
3. Text round-trip coverage checks source tokens against the PDF text layer.
4. Declared-fill presence checks that declared background colors paint pixels.

The script applies track-specific page geometry and DPI from its source. Read `scripts/visual-qa.sh` when a fixture requires exact thresholds or geometry.

## Verification

Inspect the JSON report and the per-fixture failure lines. Use `--dry-run` to verify render commands without producing PDFs. A report from a failed render or rasterization is partial and cannot establish a pass.

## Troubleshooting

- **Engine binary missing:** build with `cargo build --manifest-path engine/Cargo.toml`.
- **Prince missing:** install it or use the `showcase` track without `--check-pr`.
- **No fixtures selected:** check the track manifest.
- **A check fails:** inspect the named fixture and PDF geometry before changing a threshold.
