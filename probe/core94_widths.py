#!/usr/bin/env python3
"""CORE-94 probe J: does Prince measure text WIDER than TA?

A line of known words at a fixed width: if Prince's glyph advances are
larger (different Arial version/metrics), it fits fewer words per line →
more lines. Test: single line "Hello world" at 10pt, measure the rendered
ink width in each engine's PDF via charbox right-left.

Also test a longer line and bold/italic variants to see if it's a font
substitution issue.
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
  body {{ font-family: Arial, sans-serif; font-size: {fs}pt; line-height: 1.6;
         margin: 0; font-weight: {weight}; }}
</style>
</head>
<body>
<p>{words}</p>
</body>
</html>
"""


def line_width(pdf: Path) -> float:
    """Ink width (right-left of charbox extremes) of the first text line."""
    doc = pdfium.PdfDocument(str(pdf))
    page = doc[0]
    tp = page.get_textpage()
    n = tp.count_chars()
    lefts, rights = [], []
    for i in range(n):
        l, b, r, t = tp.get_charbox(i)
        if r - l < 0.01:
            continue
        lefts.append(l)
        rights.append(r)
    if not lefts:
        return float("nan")
    return round(max(rights) - min(lefts), 3)


def render(html: Path, out: Path, prince: bool) -> bool:
    if prince:
        r = subprocess.run([str(PRINCE_SH), str(html), *GEOM, "-o", str(out)],
                           capture_output=True, text=True)
    else:
        r = subprocess.run([str(TA_BIN), "render", str(html), *GEOM, "-o", str(out)],
                           capture_output=True, text=True)
    return r.returncode == 0


def main():
    work = Path(tempfile.mkdtemp(prefix="core94-j-"))
    print(f"probe workdir: {work}\n")
    cases = [
        ("Hello world.", "10", "normal"),
        ("Hello world this is a test sentence.", "10", "normal"),
        ("Hello world this is a test sentence.", "10", "bold"),
        ("Hello world this is a test sentence.", "12", "normal"),
        ("The quick brown fox jumps over the lazy dog.", "10", "normal"),
    ]
    print(f"{'text':>50} | {'fs':>3} | {'wgt':>5} | {'TA w':>7} | {'Pr w':>7} | {'Δ':>6}")
    print("-" * 90)
    for words, fs, weight in cases:
        html = work / f"t-{fs}-{weight}-{len(words)}.html"
        html.write_text(TMPL.format(fs=fs, weight=weight, words=words))
        ta = work / f"ta-{fs}-{weight}-{len(words)}.pdf"
        pr = work / f"pr-{fs}-{weight}-{len(words)}.pdf"
        if not render(html, ta, False) or not render(html, pr, True):
            print(f"{words[:48]:>50}: render failed")
            continue
        tw = line_width(ta)
        pw = line_width(pr)
        print(f"{words[:48]:>50} | {fs:>3} | {weight:>5} | {tw:>7} | {pw:>7} | {pw-tw:>6.2f}")
    print(f"\nworkdir kept: {work}")


if __name__ == "__main__":
    main()
