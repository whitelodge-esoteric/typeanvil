#!/usr/bin/env python3
"""CORE-94 probe P: measure TA's actual line natural-width vs content width
in the REAL prose fixture, via the fragment tree's TextRun expansion values.

Expansion ≠ 0 means the line needed glue + font-expansion to fit. If many
prose lines have negative expansion (shrunk), TA is packing overfull lines
— the total-fit signature.
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


def render(html: Path, out: Path, prince: bool) -> bool:
    if prince:
        r = subprocess.run([str(PRINCE_SH), str(html), *GEOM, "-o", str(out)],
                           capture_output=True, text=True)
    else:
        r = subprocess.run([str(TA_BIN), "render", str(html), *GEOM, "-o", str(out)],
                           capture_output=True, text=True)
    return r.returncode == 0


def char_geoms(pdf: Path) -> list[float]:
    """Right edges of all lines' last chars (ink), page 1-3."""
    doc = pdfium.PdfDocument(str(pdf))
    out = []
    for pi in range(min(3, len(doc))):
        page = doc[pi]
        H = page.get_size()[1]
        tp = page.get_textpage()
        n = tp.count_chars()
        chars = []
        for i in range(n):
            l, b, r, t = tp.get_charbox(i)
            if r - l < 0.01:
                continue
            chars.append({"l": l, "b": b, "r": r})
        chars.sort(key=lambda c: (-c["b"], c["l"]))
        lines = []
        for c in chars:
            placed = False
            for ln in lines:
                if abs(ln["b"] - c["b"]) < 3.0:
                    ln["chars"].append(c)
                    placed = True
                    break
            if not placed:
                lines.append({"b": c["b"], "chars": [c]})
        for ln in lines:
            cs = sorted(ln["chars"], key=lambda c: c["l"])
            if len(cs) >= 4:
                out.append(round(cs[-1]["r"], 2))
    return out


def main():
    work = Path(tempfile.mkdtemp(prefix="core94-p-"))
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
    te = char_geoms(ta)
    pe = char_geoms(pr)
    # Content right edge = 324pt.
    ta_short = sum(1 for e in te if e < 322)
    pr_short = sum(1 for e in pe if e < 322)
    ta_full = sum(1 for e in te if e >= 322)
    pr_full = sum(1 for e in pe if e >= 322)
    print(f"TA: {len(te)} justified lines, {ta_full} reach ≥322pt, {ta_short} short")
    print(f"Pr: {len(pe)} justified lines, {pr_full} reach ≥322pt, {pr_short} short")
    print(f"\nTA short line right edges: {[e for e in te if e < 322][:12]}")
    print(f"Pr short line right edges: {[e for e in pe if e < 322][:12]}")
    print(f"\nworkdir kept: {work}")


if __name__ == "__main__":
    main()
