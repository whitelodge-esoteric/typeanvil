#!/usr/bin/env python3
"""CORE-90 probe v2: measure line-box height in TypeAnvil vs Prince.

Design: N single-line paragraphs ("Aq" — cap + descender) stacked with zero
margins. Consecutive baseline deltas = line-box height exactly, regardless of
glyph shape. This isolates the construction math (font ascent/descent +
half-leading vs font-size * factor) from line breaking and margins.

Same geometry as the demo pipeline: 5in x 3in pages, 0.5in margins
(288pt x 144pt content). Line-height factor in {1.0, 1.2, 1.5, 1.6, 2.0}.
"""

import os
import re
import statistics
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path("/Users/elijah/workspace/typeanvil.worktrees/core-90-line-box-height")
TA_BIN = ROOT / "engine/target/debug/typeanvil"
PRINCE_SH = ROOT / "scripts/render-prince.sh"

FACTORS = [1.0, 1.2, 1.5, 1.6, 2.0]
GEOM = ["--page-width", "5in", "--page-height", "3in",
        "--margin-top", "0.5in", "--margin-right", "0.5in",
        "--margin-bottom", "0.5in", "--margin-left", "0.5in"]

# 24 single-line paragraphs fit easily on one 144pt-tall page even at
# line-height 2.0 (24 * 20pt = 480pt > 144pt -> spills to 2 pages; that's fine,
# we only read page 1's deltas).
N_PARAS = 24
PARA = "Aq"
LINE_HTML = '<p style="margin:0">Aq</p>'

PROBE_TMPL = """<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  @page {{ margin: 0.5in; }}
  body {{ font-family: Arial, sans-serif; font-size: 10pt; line-height: {factor};
         margin: 0; }}
  p {{ margin: 0; }}
</style>
</head>
<body>
{paras}
</body>
</html>
"""


def render_ta(html: Path, out: Path) -> int:
    r = subprocess.run([str(TA_BIN), "render", str(html), *GEOM, "-o", str(out)],
                       capture_output=True, text=True)
    if r.returncode != 0:
        print(f"  TA FAILED: {r.stderr[-500:]}")
    return r.returncode


def render_prince(html: Path, out: Path) -> int:
    r = subprocess.run([str(PRINCE_SH), str(html), *GEOM, "-o", str(out)],
                       capture_output=True, text=True)
    if r.returncode != 0:
        print(f"  PRINCE FAILED: {r.stderr[-500:]}")
    return r.returncode


def page_count(pdf: Path) -> int:
    raw = pdf.read_bytes()
    m = re.search(rb"/Count\s+(\d+)", raw)
    return int(m.group(1)) if m else -1


def baseline_deltas(pdf: Path) -> list[float]:
    """Consecutive line-box heights on page 1. Each paragraph is 'Aq' starting
    at x=36pt; take every char whose left edge is that first column (the 'A'),
    use its charbox BOTTOM as the line anchor (identical glyph per line), and
    delta consecutive anchors = line-box height. get_charbox returns
    (left, bottom, right, top) with y from the page BOTTOM."""
    import pypdfium2 as pdfium
    doc = pdfium.PdfDocument(str(pdf))
    page = doc[0]
    tp = page.get_textpage()
    n = tp.count_chars()
    anchors = []
    first_left = None
    for i in range(n):
        left, bottom, right, top = tp.get_charbox(i)
        if first_left is None:
            first_left = left
        if abs(left - first_left) < 0.5:
            anchors.append(bottom)
    anchors.sort(reverse=True)  # reading order: high y (page bottom coords) first
    return [round(a - b, 3) for a, b in zip(anchors, anchors[1:])]


def main():
    if not TA_BIN.exists():
        print(f"TA binary missing at {TA_BIN}; build first.")
        sys.exit(2)
    work = Path(tempfile.mkdtemp(prefix="core90-probe2-"))
    print(f"probe workdir: {work}\n")
    print(f"{'factor':>6} | {'TA pages':>8} | {'Pr pages':>8} | "
          f"{'TA median Δbase':>15} | {'Pr median Δbase':>15} | {'Pr/TA':>6}")
    print("-" * 78)
    for factor in FACTORS:
        html = work / f"probe-{factor}.html"
        html.write_text(PROBE_TMPL.format(factor=factor, paras=LINE_HTML * N_PARAS))
        ta_pdf = work / f"ta-{factor}.pdf"
        pr_pdf = work / f"pr-{factor}.pdf"
        if render_ta(html, ta_pdf) != 0 or render_prince(html, pr_pdf) != 0:
            continue
        ta_d = baseline_deltas(ta_pdf)
        pr_d = baseline_deltas(pr_pdf)
        ta_m = statistics.median(ta_d) if ta_d else float("nan")
        pr_m = statistics.median(pr_d) if pr_d else float("nan")
        ratio = pr_m / ta_m if ta_m else float("nan")
        print(f"{factor:>6} | {page_count(ta_pdf):>8} | {page_count(pr_pdf):>8} | "
              f"{ta_m:>15} | {pr_m:>15} | {ratio:>6.3f}")
        print(f"        TA deltas: {ta_d[:8]}")
        print(f"        PR deltas: {pr_d[:8]}")
    print(f"\nworkdir kept: {work}")


if __name__ == "__main__":
    main()
