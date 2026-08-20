#!/usr/bin/env python3
"""CORE-90 probe v7: per-line text + baseline dump of full prose.html page 1
(TA vs Prince at 1.2 and 1.6). Uses pypdfium2 get_text_range per char index
to label lines, so we see the exact vertical stack: margin boxes, h1, h2,
paragraphs, and every line's baseline from page top.
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


def dump_lines(pdf: Path) -> list[dict]:
    """Return ordered list of {text, baseline_from_top} for page 1."""
    import pypdfium2 as pdfium
    doc = pdfium.PdfDocument(str(pdf))
    page = doc[0]
    H = page.get_size()[1]
    tp = page.get_textpage()
    n = tp.count_chars()
    chars = []
    for i in range(n):
        left, bottom, right, top = tp.get_charbox(i)
        if right - left < 0.01:
            continue
        txt = tp.get_text_range(i, i + 1)
        chars.append({"left": left, "bottom": bottom, "right": right,
                      "top": top, "text": txt})
    # Cluster into lines: sort by bottom desc (reading order), then group
    # chars whose bottoms are within 3pt.
    chars.sort(key=lambda c: -c["bottom"])
    lines = []
    for c in chars:
        placed = False
        for ln in lines:
            if abs(ln["baseline"] - c["bottom"]) < 3.0:
                ln["chars"].append(c)
                placed = True
                break
        if not placed:
            lines.append({"baseline": c["bottom"], "chars": [c]})
    out = []
    for ln in lines:
        cs = sorted(ln["chars"], key=lambda c: c["left"])
        text = "".join(c["text"] for c in cs).replace("\n", " ").strip()
        l = min(c["left"] for c in cs)
        r = max(c["right"] for c in cs)
        # tallest charbox as size proxy
        max_h = max(c["top"] - c["bottom"] for c in cs)
        out.append({"text": text[:46], "baseline_top": round(H - ln["baseline"], 2),
                    "x": round(l), "x2": round(r), "fs_proxy": round(max_h / 0.72, 1)})
    out.sort(key=lambda d: d["baseline_top"])
    return out


def main():
    if not TA_BIN.exists():
        print(f"TA binary missing at {TA_BIN}; build first.")
        sys.exit(2)
    work = Path(tempfile.mkdtemp(prefix="core90-lines-"))
    print(f"probe workdir: {work}\n")
    for factor in [1.2, 1.6]:
        html = work / f"prose-{factor}.html"
        html.write_text(patched(factor))
        ta_pdf = work / f"ta-{factor}.pdf"
        pr_pdf = work / f"pr-{factor}.pdf"
        render(html, ta_pdf, False)
        render(html, pr_pdf, True)
        ta_lines = dump_lines(ta_pdf)
        pr_lines = dump_lines(pr_pdf)
        print(f"===== factor {factor} — page 1 lines =====")
        print("--- TypeAnvil ---")
        for d in ta_lines:
            print(f"  y={d['baseline_top']:>7} x={d['x']:>3}-{d['x2']:>3} "
                  f"~{d['fs_proxy']:>4}pt | {d['text']}")
        print("--- Prince ---")
        for d in pr_lines:
            print(f"  y={d['baseline_top']:>7} x={d['x']:>3}-{d['x2']:>3} "
                  f"~{d['fs_proxy']:>4}pt | {d['text']}")
        print()
    print(f"workdir kept: {work}")


if __name__ == "__main__":
    main()
