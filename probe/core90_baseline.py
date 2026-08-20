#!/usr/bin/env python3
"""CORE-90 probe v8: FIRST-LINE BASELINE POSITION.

The v7 dump showed the divergence is NOT line-box height (heights match) but
the baseline position of the FIRST line in a block: Prince's first baseline
moves with factor (half-leading!), TA's is fixed. This probe renders a single
block (h1 at 15pt, no margin box, no preceding content) and measures the
baseline y-from-top of its first line at each factor, TA vs Prince.

Expected per CSS2.1 §10.8.1: first baseline = content_top + ascent +
half_leading, where half_leading = (line_height - font_size)/2. If Prince
follows this, its first baseline rises with factor. If TA's is fixed, TA
ignores half-leading (baseline = box_top + font_size).
"""

import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path("/Users/elijah/workspace/typeanvil.worktrees/core-90-line-box-height")
TA_BIN = ROOT / "engine/target/debug/typeanvil"
PRINCE_SH = ROOT / "scripts/render-prince.sh"

FACTORS = [1.0, 1.2, 1.5, 1.6, 2.0]
SIZES = [10.0, 15.0]
GEOM = ["--page-width", "5in", "--page-height", "3in",
        "--margin-top", "0.5in", "--margin-right", "0.5in",
        "--margin-bottom", "0.5in", "--margin-left", "0.5in"]

TMPL = """<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  @page {{ margin: 0.5in; }}
  body {{ font-family: Arial, sans-serif; font-size: 10pt; margin: 0; }}
  h1 {{ font-size: {fs}pt; line-height: {factor}; margin: 0; }}
</style>
</head>
<body>
<h1>Heading Aq</h1>
</body>
</html>
"""


def render(html: Path, out: Path, prince: bool) -> int:
    if prince:
        r = subprocess.run([str(PRINCE_SH), str(html), *GEOM, "-o", str(out)],
                           capture_output=True, text=True)
    else:
        r = subprocess.run([str(TA_BIN), "render", str(html), *GEOM, "-o", str(out)],
                           capture_output=True, text=True)
    if r.returncode != 0:
        print(f"  FAILED: {r.stderr[-500:]}")
    return r.returncode


def first_baseline(pdf: Path) -> float:
    """Baseline (charbox bottom) of the first text char, converted to
    y-from-page-top."""
    import pypdfium2 as pdfium
    doc = pdfium.PdfDocument(str(pdf))
    page = doc[0]
    H = page.get_size()[1]
    tp = page.get_textpage()
    n = tp.count_chars()
    for i in range(n):
        left, bottom, right, top = tp.get_charbox(i)
        if right - left > 0.01:
            return round(H - bottom, 3)
    return float("nan")


def main():
    if not TA_BIN.exists():
        print(f"TA binary missing at {TA_BIN}; build first.")
        sys.exit(2)
    work = Path(tempfile.mkdtemp(prefix="core90-baseline-"))
    print(f"probe workdir: {work}\n")
    for fs in SIZES:
        print(f"=== font-size {fs}pt, one h1 line ===")
        print(f"{'factor':>6} | {'TA first base':>13} | {'Pr first base':>13} | "
              f"{'Pr-TA':>6} | {'TA-36':>6} | {'Pr-36':>6}")
        for factor in FACTORS:
            html = work / f"h1-fs{fs}-lh{factor}.html"
            html.write_text(TMPL.format(fs=fs, factor=factor))
            ta_pdf = work / f"ta-h1-fs{fs}-lh{factor}.pdf"
            pr_pdf = work / f"pr-h1-fs{fs}-lh{factor}.pdf"
            if render(html, ta_pdf, False) != 0 or render(html, pr_pdf, True) != 0:
                continue
            ta_b = first_baseline(ta_pdf)
            pr_b = first_baseline(pr_pdf)
            print(f"{factor:>6} | {ta_b:>13} | {pr_b:>13} | {pr_b-ta_b:>6.2f} | "
                  f"{ta_b-36:>6.2f} | {pr_b-36:>6.2f}")
        print()
    print(f"workdir kept: {work}")


if __name__ == "__main__":
    main()
