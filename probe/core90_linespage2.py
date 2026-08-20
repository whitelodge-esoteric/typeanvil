#!/usr/bin/env python3
"""CORE-90 probe v13: precise per-page line count + last baseline for prose
with body margin 0 at 1.2/1.6, TA vs Prince. Uses baseline-anchored clustering
with a small tolerance (glyph tops vary; baselines are stable)."""
import re, subprocess, sys, tempfile
from pathlib import Path

ROOT = Path("/Users/elijah/workspace/typeanvil.worktrees/core-90-line-box-height")
TA_BIN = ROOT / "engine/target/debug/typeanvil"
PRINCE_SH = ROOT / "scripts/render-prince.sh"
CORPUS = Path("/Users/elijah/workspace/typeanvil/demo/corpus/prose.html")
GEOM = ["--page-width", "5in", "--page-height", "3in",
        "--margin-top", "0.5in", "--margin-right", "0.5in",
        "--margin-bottom", "0.5in", "--margin-left", "0.5in"]

def patched(factor):
    src = CORPUS.read_text()
    src = re.sub(r"line-height:\s*[\d.]+", f"line-height: {factor}", src, count=1)
    src = src.replace("body { font-family: Arial, Helvetica, \"Liberation Sans\", sans-serif; font-size: 10pt;",
                      "body { font-family: Arial, Helvetica, \"Liberation Sans\", sans-serif; font-size: 10pt; margin: 0;")
    return src

def render(html, out, prince):
    if prince:
        r = subprocess.run([str(PRINCE_SH), str(html), *GEOM, "-o", str(out)], capture_output=True, text=True)
    else:
        r = subprocess.run([str(TA_BIN), "render", str(html), *GEOM, "-o", str(out)], capture_output=True, text=True)
    return r.returncode

def per_page_lines(pdf):
    import pypdfium2 as pdfium
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
            if prev is None or bl - prev > 2.5:
                lines += 1
            prev = bl
        out.append(lines)
    return out

work = Path(tempfile.mkdtemp(prefix="core90-lp-"))
print("probe workdir:", work)
for factor in [1.2, 1.6, 2.0]:
    html = work / f"prose-{factor}.html"
    html.write_text(patched(factor))
    ta = work / f"ta-{factor}.pdf"
    pr = work / f"pr-{factor}.pdf"
    render(html, ta, False)
    render(html, pr, True)
    ta_l = per_page_lines(ta)
    pr_l = per_page_lines(pr)
    print(f"factor {factor}:")
    print(f"  TA lines/page: {ta_l}  total {sum(ta_l)}")
    print(f"  Pr lines/page: {pr_l}  total {sum(pr_l)}")
