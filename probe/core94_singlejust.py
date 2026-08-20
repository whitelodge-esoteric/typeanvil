#!/usr/bin/env python3
"""CORE-94 probe O: single-line justify geometry.

Two/three words justified across the full 288pt line. Measure the rendered
ink: left edge, right edge, and the gap. If TA doesn't reach 324, the
justify fill is broken (not just a break-position choice).
"""

import subprocess
import tempfile
from pathlib import Path

import pypdfium2 as pdfium

ROOT = Path("/Users/elijah/workspace/typeanvil.worktrees/core-94-line-breaking")
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
  @page {{ margin: 0.5in; }}
  body {{ font-family: Arial, sans-serif; font-size: 10pt; line-height: 1.6;
         margin: 0; }}
  p {{ text-align: justify; hyphens: none; margin: 0; }}
</style>
</head>
<body>
<p>{words}</p>
</body>
</html>
"""


def line_geometry(pdf: Path) -> tuple:
    doc = pdfium.PdfDocument(str(pdf))
    page = doc[0]
    tp = page.get_textpage()
    n = tp.count_chars()
    lefts, rights, tops = [], [], []
    for i in range(n):
        l, b, r, t = tp.get_charbox(i)
        if r - l < 0.01:
            continue
        lefts.append(l)
        rights.append(r)
        tops.append((H - b) if False else b)  # placeholder
    if not lefts:
        return None
    return (round(min(lefts), 2), round(max(rights), 2))


def render(html: Path, out: Path, prince: bool) -> bool:
    if prince:
        r = subprocess.run([str(PRINCE_SH), str(html), *GEOM, "-o", str(out)],
                           capture_output=True, text=True)
    else:
        r = subprocess.run([str(TA_BIN), "render", str(html), *GEOM, "-o", str(out)],
                           capture_output=True, text=True)
    return r.returncode == 0


def main():
    work = Path(tempfile.mkdtemp(prefix="core94-o-"))
    print(f"probe workdir: {work}\n")
    cases = [
        ("aa bb", "2 words"),
        ("alpha beta gamma delta", "4 words"),
        ("alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu",
         "14 words"),
    ]
    print(f"{'case':>20} | {'TA L→R':>14} | {'Pr L→R':>14} | {'TA width':>8} | {'Pr width':>8}")
    print("-" * 76)
    for words, label in cases:
        html = work / f"{len(words)}.html"
        html.write_text(TMPL.format(words=words))
        ta = work / f"ta-{len(words)}.pdf"
        pr = work / f"pr-{len(words)}.pdf"
        if not render(html, ta, False) or not render(html, pr, True):
            print(f"{label}: render failed")
            continue
        tg = line_geometry(ta)
        pg = line_geometry(pr)
        if tg and pg:
            tw = tg[1] - tg[0]
            pw = pg[1] - pg[0]
            print(f"{label:>20} | {tg[0]}→{tg[1]:>7} | {pg[0]}→{pg[1]:>7} | {tw:>8.2f} | {pw:>8.2f}")
        else:
            print(f"{label}: no text")
    print(f"\nworkdir kept: {work}")


if __name__ == "__main__":
    main()
