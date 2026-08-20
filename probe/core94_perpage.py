#!/usr/bin/env python3
"""CORE-94 probe F: per-page line counts via precise baseline clustering.

Uses the charbox BASELINE (bottom) which is stable, with a tight tolerance
(1.5pt) so lines at 1.6 (16pt apart) never merge. Reports per-page line
counts for both engines across the whole doc, marking the first page where
they diverge.
"""

import re
import subprocess
import tempfile
from pathlib import Path

import pypdfium2 as pdfium

ROOT = Path("/Users/elijah/workspace/typeanvil.worktrees/core-94-line-breaking")
TA_BIN = ROOT / "engine/target/debug/typeanvil"
PRINCE_SH = ROOT / "scripts/render-prince.sh"
CORPUS = Path("/Users/elijah/workspace/typeanvil/demo/corpus/prose.html")

GEOM = ["--page-width", "5in", "--page-height", "3in",
        "--margin-top", "0.5in", "--margin-right", "0.5in",
        "--margin-bottom", "0.5in", "--margin-left", "0.5in"]


def per_page_lines(pdf: Path) -> list[int]:
    doc = pdfium.PdfDocument(str(pdf))
    out = []
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
        out.append(lines)
    return out


def render(html: Path, out: Path, prince: bool) -> bool:
    if prince:
        r = subprocess.run([str(PRINCE_SH), str(html), *GEOM, "-o", str(out)],
                           capture_output=True, text=True)
    else:
        r = subprocess.run([str(TA_BIN), "render", str(html), *GEOM, "-o", str(out)],
                           capture_output=True, text=True)
    return r.returncode == 0


def main():
    work = Path(tempfile.mkdtemp(prefix="core94-f-"))
    src = CORPUS.read_text()
    src = src.replace(
        'body { font-family: Arial, Helvetica, "Liberation Sans", sans-serif; font-size: 10pt; line-height: 1.6;',
        'body { font-family: Arial, Helvetica, "Liberation Sans", sans-serif; font-size: 10pt; line-height: 1.6; margin: 0;')
    html = work / "prose.html"
    html.write_text(src)
    ta = work / "ta.pdf"
    pr = work / "pr.pdf"
    render(html, ta, False)
    render(html, pr, True)
    tl = per_page_lines(ta)
    pl = per_page_lines(pr)
    print(f"TA pages: {len(tl)}  Pr pages: {len(pl)}")
    print(f"TA lines/page: {tl}  total {sum(tl)}")
    print(f"Pr lines/page: {pl}  total {sum(pl)}")
    print("\nper-page delta (Pr − TA):")
    for i in range(max(len(tl), len(pl))):
        a = tl[i] if i < len(tl) else 0
        b = pl[i] if i < len(pl) else 0
        mark = "  <-- first divergence" if b != a and i == next(
            (j for j in range(max(len(tl), len(pl))) if (tl[j] if j < len(tl) else 0) != (pl[j] if j < len(pl) else 0)), -1) else ""
        print(f"  p{i+1}: TA {a}  Pr {b}  ({b-a:+d}){mark}")
    print(f"\nworkdir kept: {work}")


if __name__ == "__main__":
    main()
