#!/usr/bin/env python3
"""CORE-90 probe v6: dump page-1 text layout of full prose.html at 1.2 and
1.6, TA vs Prince — every line's baseline y and approximate font size
(charbox height), so we can see WHERE the two engines' vertical sums diverge
(heading block? first body para? margins?).
"""

import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path("/Users/elijah/workspace/typeanvil.worktrees/core-90-line-box-height")
TA_BIN = ROOT / "engine/target/debug/typeanvil"
PRINCE_SH = ROOT / "scripts/render-prince.sh"
CORPUS = Path("/Users/elijah/workspace/typeanvil/demo/corpus/prose.html")

GEOM = ["--page-width", "5in", "--page-height", "3in",
        "--margin-top", "0.5in", "--margin-right", "0.5in",
        "--margin-bottom", "0.5in", "--margin-left", "0.5in"]


def patched(factor: float) -> str:
    src = CORPUS.read_text()
    return re.sub(r"line-height:\s*[\d.]+", f"line-height: {factor}", src, count=1)


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


def lines_pg1(pdf: Path) -> list[tuple]:
    """Return (baseline_y_from_top, approx_font_pt, first_word) per line on
    page 1. Charbox y is from page BOTTOM; convert to from-top. Font-size
    proxy: median charbox height among that line's chars (caps taller than
    x-height, so use the max charbox top-bottom for 'A'.. hmm — use the
    tallest charbox in the line as the cap-height proxy and divide by 0.72
    (Arial cap height ratio) for an approximate point size."""
    import pypdfium2 as pdfium
    from collections import defaultdict
    doc = pdfium.PdfDocument(str(pdf))
    page = doc[0]
    H = page.get_size()[1]  # page height in pt
    tp = page.get_textpage()
    n = tp.count_chars()
    chars = []
    for i in range(n):
        left, bottom, right, top = tp.get_charbox(i)
        if right - left < 0.01:
            continue
        chars.append((left, bottom, right, top))
    # cluster into lines by bottom (baseline): chars with bottoms within 2pt
    lines = []
    for c in chars:
        left, bottom, right, top = c
        placed = False
        for ln in lines:
            if abs(ln["baseline"] - bottom) < 2.0:
                ln["chars"].append(c)
                placed = True
                break
        if not placed:
            lines.append({"baseline": bottom, "chars": [c]})
    out = []
    for ln in lines:
        l = min(c[0] for c in ln["chars"])
        r = max(c[2] for c in ln["chars"])
        max_h = max(c[3] - c[1] for c in ln["chars"])  # tallest glyph box
        # cap-height approx: tallest box height / 0.72 (Arial cap ~ 72% em)
        approx_fs = max_h / 0.72
        # first word: chars sorted by x, group until a big gap
        xsorted = sorted(ln["chars"], key=lambda c: c[0])
        word = ""
        prev_x = None
        for c in xsorted:
            if prev_x is not None and c[0] - prev_x > 6.0:
                break
            word += "?"  # we don't have text mapping here; use x-span
            prev_x = c[2]
        y_top = H - ln["baseline"]  # baseline from top
        out.append((round(y_top, 2), round(approx_fs, 1), round(l, 1), round(r, 1)))
    out.sort()
    return out


def main():
    if not TA_BIN.exists():
        print(f"TA binary missing at {TA_BIN}; build first.")
        sys.exit(2)
    work = Path(tempfile.mkdtemp(prefix="core90-dump-"))
    print(f"probe workdir: {work}\n")
    for factor in [1.2, 1.6]:
        html = work / f"prose-{factor}.html"
        html.write_text(patched(factor))
        ta_pdf = work / f"ta-{factor}.pdf"
        pr_pdf = work / f"pr-{factor}.pdf"
        render(html, ta_pdf, False)
        render(html, pr_pdf, True)
        ta_lines = lines_pg1(ta_pdf)
        pr_lines = lines_pg1(pr_pdf)
        print(f"===== factor {factor} — page 1 lines (baseline-from-top, ~fs, x-span) =====")
        print(f"{'TA':>44} | {'Prince':>44}")
        for i in range(max(len(ta_lines), len(pr_lines))):
            a = ta_lines[i] if i < len(ta_lines) else None
            b = pr_lines[i] if i < len(pr_lines) else None
            sa = f"{a[0]:>7} ~{a[1]:>4}pt x[{a[2]:.0f},{a[3]:.0f}]" if a else " " * 30
            sb = f"{b[0]:>7} ~{b[1]:>4}pt x[{b[2]:.0f},{b[3]:.0f}]" if b else " " * 30
            print(f"{sa:>44} | {sb:>44}")
        print()
    print(f"workdir kept: {work}")


if __name__ == "__main__":
    main()
