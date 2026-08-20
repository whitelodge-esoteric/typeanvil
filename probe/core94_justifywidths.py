#!/usr/bin/env python3
"""CORE-94 probe K: chars-per-line in pure justify, multiple widths.

One paragraph, justify, no hyphens, uniform-ish words. Vary content width.
Measure TOTAL lines across the doc (not just page 1). If TA consistently
packs more chars/line than Prince at justify, that's the K-P total-fit
tightness vs Prince's algorithm.
"""

import re
import subprocess
import tempfile
from pathlib import Path

import pypdfium2 as pdfium

ROOT = Path("/Users/elijah/workspace/typeanvil.worktrees/core-94-line-breaking")
TA_BIN = ROOT / "engine/target/debug/typeanvil"
PRINCE_SH = ROOT / "scripts/render-prince.sh"

# Long justify paragraph, no hyphens, all simple words.
WORDS = ("lorem ipsum dolor sit amet consectetur adipiscing elit sed do "
         "eiusmod tempor incididunt ut labore et dolore magna aliqua ut enim "
         "ad minim veniam quis nostrud exercitation ullamco laboris nisi ut "
         "aliquip ex ea commodo consequat duis aute irure dolor in "
         "reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla "
         "pariatur excepteur sint occaecat cupidatat non proident sunt in "
         "culpa qui officia deserunt mollit anim id est laborum " * 4).strip()

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


def total_lines(pdf: Path) -> int:
    doc = pdfium.PdfDocument(str(pdf))
    total = 0
    for pi in range(len(doc)):
        page = doc[pi]
        H = page.get_size()[1]
        tp = page.get_textpage()
        n = tp.count_chars()
        baselines = []
        for i in range(n):
            l, b, r, t = tp.get_charbox(i)
            if r - l < 0.01:
                continue
            baselines.append(round(H - b, 2))
        baselines.sort()
        lines = 0
        prev = None
        for bl in baselines:
            if prev is None or bl - prev > 1.5:
                lines += 1
            prev = bl
        total += lines
    return total


def render(html: Path, out: Path, prince: bool, geom: list[str]) -> bool:
    if prince:
        r = subprocess.run([str(PRINCE_SH), str(html), *geom, "-o", str(out)],
                           capture_output=True, text=True)
    else:
        r = subprocess.run([str(TA_BIN), "render", str(html), *geom, "-o", str(out)],
                           capture_output=True, text=True)
    return r.returncode == 0


def main():
    work = Path(tempfile.mkdtemp(prefix="core94-k-"))
    print(f"probe workdir: {work}\n")
    html = work / "lorem.html"
    html.write_text(TMPL.format(words=WORDS))
    print(f"{'content w':>10} | {'TA lines':>8} | {'Pr lines':>8} | {'Pr-TA':>6} | {'TA/line':>7} | {'Pr/line':>7}")
    print("-" * 62)
    for w_in in [5.0, 4.5, 4.0, 3.5, 3.0, 2.5]:
        geom = ["--page-width", f"{w_in}in", "--page-height", "3in",
                "--margin-top", "0.5in", "--margin-right", "0.5in",
                "--margin-bottom", "0.5in", "--margin-left", "0.5in"]
        ta = work / f"ta-{w_in}.pdf"
        pr = work / f"pr-{w_in}.pdf"

        def rnd(cmd):
            r = subprocess.run(cmd, capture_output=True, text=True)
            return r.returncode == 0

        if not rnd([str(TA_BIN), "render", str(html), *geom, "-o", str(ta)]) or \
           not rnd([str(PRINCE_SH), str(html), *geom, "-o", str(pr)]):
            print(f"w={w_in}: render failed")
            continue
        tl = total_lines(ta)
        pl = total_lines(pr)
        n_words = len(WORDS.split())
        ta_per = round(n_words / tl, 1) if tl else 0
        pr_per = round(n_words / pl, 1) if pl else 0
        content_w = round((w_in - 1.0) * 72, 1)
        print(f"{content_w:>10} | {tl:>8} | {pl:>8} | {pl-tl:>6} | {ta_per:>7} | {pr_per:>7}")
    print(f"\nworkdir kept: {work}")


if __name__ == "__main__":
    main()
