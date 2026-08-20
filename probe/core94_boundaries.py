#!/usr/bin/env python3
"""CORE-94 probe G: exact line-break boundaries, controlled paragraph.

One paragraph of LOREM-style prose (real words, no hyphens), left-aligned,
body margin 0, no margin boxes. Extract line text per engine via
get_text_bounded on raster bands (probe E technique) and compare where each
line ends. This shows the EXACT break-point difference.

Also runs justify mode for contrast.
"""

import re
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

WORDS = ("The quick brown fox jumps over the lazy dog while the sun sets "
         "behind the distant mountains and the river flows gently through "
         "the valley where ancient trees stand watch over the sleeping "
         "village that has known centuries of quiet change. " * 5).strip()

TMPL = """<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  @page {{ margin: 0.5in; }}
  body {{ font-family: Arial, sans-serif; font-size: 10pt; line-height: 1.6;
         margin: 0; }}
  p {{ text-align: {align}; hyphens: none; margin: 0; }}
</style>
</head>
<body>
<p>{words}</p>
</body>
</html>
"""

SCALE = 2


def text_bands(pdf: Path, page_idx: int = 0) -> list[str]:
    doc = pdfium.PdfDocument(str(pdf))
    page = doc[page_idx]
    W, H = page.get_size()
    bmp = page.render(scale=SCALE).to_pil()
    w, h = bmp.size
    px = bmp.load()
    m = int(0.5 * 72 * SCALE)
    row_has_ink = []
    for y in range(m, h - m):
        ink = any(px[x, y][0] < 128 for x in range(m, w - m))
        row_has_ink.append(ink)
    bands = []
    in_band = False
    for y, ink in enumerate(row_has_ink):
        if ink and not in_band:
            in_band = True
            start = y
        elif not ink and in_band:
            in_band = False
            bands.append((start, y - 1))
    if in_band:
        bands.append((start, len(row_has_ink) - 1))
    out = []
    tp = page.get_textpage()
    for y0, y1 in bands:
        if y1 - y0 < 3:
            continue
        top_pt = y0 / SCALE
        bottom_pt = y1 / SCALE
        txt = tp.get_text_bounded(36.0, H - bottom_pt, W - 36.0, H - top_pt).replace("\n", " ").strip()
        if txt:
            out.append(txt)
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
    work = Path(tempfile.mkdtemp(prefix="core94-g-"))
    print(f"probe workdir: {work}\n")
    for align in ["left", "justify"]:
        html = work / f"{align}.html"
        html.write_text(TMPL.format(align=align, words=WORDS))
        ta = work / f"ta-{align}.pdf"
        pr = work / f"pr-{align}.pdf"
        if not render(html, ta, False) or not render(html, pr, True):
            print(f"{align}: render failed")
            continue
        tl = text_bands(ta, 0)
        pl = text_bands(pr, 0)
        print(f"===== {align}: TA {len(tl)} lines, Pr {len(pl)} lines (page 1) =====")
        print(f"{'TA':>58} | {'Pr':>58}")
        for i in range(max(len(tl), len(pl))):
            a = tl[i] if i < len(tl) else ""
            b = pl[i] if i < len(pl) else ""
            print(f"{a[:58]:>58} | {b[:58]}")
        print()
    print(f"workdir kept: {work}")


if __name__ == "__main__":
    main()
