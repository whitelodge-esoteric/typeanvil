#!/usr/bin/env python3
"""CORE-90 probe v11: count lines per page + last-line ink bottom for full
prose at 1.2/1.6, TA vs Prince. If page counts diverge because of LINE
COUNT per page, this shows it; also measure the lowest ink (glyph bottom)
on each page vs the content bottom (180pt from top: 216 - 0.5in*72)."""

import re
import statistics
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

PAGE_H = 216.0
CONTENT_BOTTOM = 216.0 - 36.0  # 180pt from top


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
        print(f"  FAILED: {r.stderr[-300:]}")
    return r.returncode


def page_stats(pdf: Path) -> dict:
    import pypdfium2 as pdfium
    doc = pdfium.PdfDocument(str(pdf))
    out = {"pages": len(doc), "lines": [], "max_ink_bottom": []}
    for pi in range(len(doc)):
        page = doc[pi]
        H = page.get_size()[1]
        tp = page.get_textpage()
        n = tp.count_chars()
        anchors = []
        bottoms = []
        for i in range(n):
            left, bottom, right, top = tp.get_charbox(i)
            if right - left < 0.01:
                continue
            anchors.append(round(H - bottom, 2))  # baseline from top
            bottoms.append(round(H - bottom, 2))  # ink bottom from top
        # cluster into lines: sort by baseline desc, group within 3pt
        anchors.sort()
        lines = 0
        prev = None
        for a in anchors:
            if prev is None or a - prev > 3.0:
                lines += 1
            prev = a
        out["lines"].append(lines)
        out["max_ink_bottom"].append(round(max(bottoms), 2) if bottoms else 0.0)
    return out


def main():
    if not TA_BIN.exists():
        print(f"TA binary missing at {TA_BIN}; build first.")
        sys.exit(2)
    work = Path(tempfile.mkdtemp(prefix="core90-linesper-"))
    print(f"probe workdir: {work}\n")
    print(f"content bottom = {CONTENT_BOTTOM}pt from top\n")
    for factor in [1.2, 1.6]:
        html = work / f"prose-{factor}.html"
        html.write_text(patched(factor))
        ta_pdf = work / f"ta-{factor}.pdf"
        pr_pdf = work / f"pr-{factor}.pdf"
        render(html, ta_pdf, False)
        render(html, pr_pdf, True)
        ts = page_stats(ta_pdf)
        ps = page_stats(pr_pdf)
        print(f"===== factor {factor} =====")
        print(f"  pages: TA {ts['pages']}  Pr {ps['pages']}")
        print(f"  lines per page: TA {ts['lines']}")
        print(f"  lines per page: Pr {ps['lines']}")
        print(f"  last-ink-bottom per page (content bottom {CONTENT_BOTTOM}):")
        print(f"    TA {ts['max_ink_bottom']}")
        print(f"    Pr {ps['max_ink_bottom']}")
        print()
    print(f"workdir kept: {work}")


if __name__ == "__main__":
    main()
