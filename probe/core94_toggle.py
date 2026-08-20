#!/usr/bin/env python3
"""CORE-94 probe I: toggle prose CSS features to isolate the divergence.

prose.html uses: justify, hyphens: auto, protrusion (hanging punctuation),
expansion. Toggle each off and compare TA vs Pr total line counts. The
toggle that makes them match identifies the driver.
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


def variant(kind: str) -> str:
    src = CORPUS.read_text()
    src = src.replace(
        'body { font-family: Arial, Helvetica, "Liberation Sans", sans-serif; font-size: 10pt; line-height: 1.6;',
        'body { font-family: Arial, Helvetica, "Liberation Sans", sans-serif; font-size: 10pt; line-height: 1.6; margin: 0;')
    if kind == "base":
        return src
    if kind == "no-justify":
        return src.replace("text-align: justify", "text-align: left")
    if kind == "no-hyphens":
        return src.replace("hyphens: auto", "hyphens: none")
    if kind == "no-justify-no-hyph":
        src = src.replace("text-align: justify", "text-align: left")
        return src.replace("hyphens: auto", "hyphens: none")
    if kind == "no-protru":
        # remove hanging punctuation via CSS not possible; skip
        return src
    return src


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


def render(html: Path, out: Path, prince: bool) -> bool:
    if prince:
        r = subprocess.run([str(PRINCE_SH), str(html), *GEOM, "-o", str(out)],
                           capture_output=True, text=True)
    else:
        r = subprocess.run([str(TA_BIN), "render", str(html), *GEOM, "-o", str(out)],
                           capture_output=True, text=True)
    return r.returncode == 0


def main():
    work = Path(tempfile.mkdtemp(prefix="core94-i-"))
    print(f"probe workdir: {work}\n")
    kinds = ["base", "no-justify", "no-hyphens", "no-justify-no-hyph"]
    print(f"{'variant':>18} | {'TA lines':>8} | {'Pr lines':>8} | {'Pr-TA':>6}")
    print("-" * 50)
    for kind in kinds:
        html = work / f"{kind}.html"
        html.write_text(variant(kind))
        ta = work / f"ta-{kind}.pdf"
        pr = work / f"pr-{kind}.pdf"
        if not render(html, ta, False) or not render(html, pr, True):
            print(f"{kind}: render failed")
            continue
        tl = total_lines(ta)
        pl = total_lines(pr)
        print(f"{kind:>18} | {tl:>8} | {pl:>8} | {pl-tl:>6}")
    print(f"\nworkdir kept: {work}")


if __name__ == "__main__":
    main()
