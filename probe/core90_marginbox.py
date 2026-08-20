#!/usr/bin/env python3
"""CORE-90 probe v9: does the @top-center margin box push content down?

v7 vs v8 suggested TA's first baseline moves 57→51 when the margin box is
removed, while Prince's stays 50.2. Toggle the margin box on/off and measure
the h1 first baseline at factor 1.2 and 1.6 in both engines.
"""

import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path("/Users/elijah/workspace/typeanvil.worktrees/core-90-line-box-height")
TA_BIN = ROOT / "engine/target/debug/typeanvil"
PRINCE_SH = ROOT / "scripts/render-prince.sh"

GEOM = ["--page-width", "5in", "--page-height", "3in",
        "--margin-top", "0.5in", "--margin-right", "0.5in",
        "--margin-bottom", "0.5in", "--margin-left", "0.5in"]

TMPL = """<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  @page {{ margin: 0.5in;{marginbox} }}
  body {{ font-family: Arial, sans-serif; font-size: 10pt; line-height: {factor}; margin: 0; }}
  h1 {{ font-size: 15pt; margin: 0; }}
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
    work = Path(tempfile.mkdtemp(prefix="core90-mb-"))
    print(f"probe workdir: {work}\n")
    mb_on = "\n    @top-center { content: \"Header\"; font-family: Arial; font-size: 10pt; }"
    print(f"{'factor':>6} | {'mb':>3} | {'TA base':>8} | {'Pr base':>8} | {'Pr-TA':>6}")
    for factor in [1.2, 1.6]:
        for mb in [False, True]:
            html = work / f"mb{int(mb)}-lh{factor}.html"
            html.write_text(TMPL.format(factor=factor, marginbox=mb_on if mb else ""))
            ta_pdf = work / f"ta-mb{int(mb)}-lh{factor}.pdf"
            pr_pdf = work / f"pr-mb{int(mb)}-lh{factor}.pdf"
            if render(html, ta_pdf, False) != 0 or render(html, pr_pdf, True) != 0:
                continue
            ta_b = first_baseline(ta_pdf)
            pr_b = first_baseline(pr_pdf)
            print(f"{factor:>6} | {str(mb):>3} | {ta_b:>8} | {pr_b:>8} | {pr_b-ta_b:>6.2f}")
    print(f"\nworkdir kept: {work}")


if __name__ == "__main__":
    main()
