#!/usr/bin/env python3
"""CORE-90 probe v4: line box per FONT SIZE.

prose.html has 10pt body, 15pt h1, 11pt h2, 8pt note. Hypothesis from v2/v3:
uniform 10pt lines agree, so the divergence must appear at other font sizes
(em-box/font-metric leading scales with font size) or in mixed-size inline
lines. This probe measures the line-box height of a single-line block at each
font size × each line-height factor, TA vs Prince.

Geometry: 5in x 3in, 0.5in margins. One line per page; read the line's
baseline from the PDF. To get several lines per page use multiple identical
blocks stacked (margin 0), and take the median consecutive delta.
"""

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
FONT_SIZES = [8.0, 10.0, 11.0, 15.0]
GEOM = ["--page-width", "5in", "--page-height", "3in",
        "--margin-top", "0.5in", "--margin-right", "0.5in",
        "--margin-bottom", "0.5in", "--margin-left", "0.5in"]
N_BLOCKS = 20

TMPL = """<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  @page {{ margin: 0.5in; }}
  body {{ font-family: Arial, sans-serif; font-size: 10pt; margin: 0; }}
  .b {{ font-size: {fs}pt; line-height: {factor}; margin: 0; }}
</style>
</head>
<body>
{blocks}
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


def baseline_deltas(pdf: Path) -> list[float]:
    import pypdfium2 as pdfium
    doc = pdfium.PdfDocument(str(pdf))
    page = doc[0]
    tp = page.get_textpage()
    n = tp.count_chars()
    anchors = []
    first_left = None
    for i in range(n):
        left, bottom, right, top = tp.get_charbox(i)
        if right - left < 0.01:
            continue
        if first_left is None:
            first_left = left
        if abs(left - first_left) < 0.5:
            anchors.append(bottom)
    anchors.sort(reverse=True)
    return [round(a - b, 3) for a, b in zip(anchors, anchors[1:])]


def median(xs):
    return statistics.median(xs) if xs else float("nan")


def main():
    if not TA_BIN.exists():
        print(f"TA binary missing at {TA_BIN}; build first.")
        sys.exit(2)
    work = Path(tempfile.mkdtemp(prefix="core90-fontsize-"))
    print(f"probe workdir: {work}\n")
    for fs in FONT_SIZES:
        print(f"=== font-size {fs}pt ===")
        print(f"{'factor':>6} | {'TA Δbase':>9} | {'Pr Δbase':>9} | {'Pr/TA':>6}")
        for factor in FACTORS:
            html = work / f"fs{fs}-lh{factor}.html"
            block = '<p class="b">Aq</p>'
            html.write_text(TMPL.format(fs=fs, factor=factor, blocks=block * N_BLOCKS))
            ta_pdf = work / f"ta-fs{fs}-lh{factor}.pdf"
            pr_pdf = work / f"pr-fs{fs}-lh{factor}.pdf"
            if render_ta(html, ta_pdf) != 0 or render_prince(html, pr_pdf) != 0:
                continue
            ta_m = median(baseline_deltas(ta_pdf))
            pr_m = median(baseline_deltas(pr_pdf))
            ratio = pr_m / ta_m if ta_m else float("nan")
            print(f"{factor:>6} | {ta_m:>9} | {pr_m:>9} | {ratio:>6.3f}")
        print()
    print(f"workdir kept: {work}")


if __name__ == "__main__":
    main()
