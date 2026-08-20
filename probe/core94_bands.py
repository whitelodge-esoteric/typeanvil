#!/usr/bin/env python3
"""CORE-94 probe E: per-line TEXT via raster-guided bounding boxes.

Rasterize each page, find text-line bands, then for each band use
pypdfium2's get_text_bounded(box) to extract the text in that band — which
avoids the char-order scrambling of per-char iteration.
"""

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

SCALE = 2


def text_bands(pdf: Path, page_idx: int = 0) -> list[str]:
    doc = pdfium.PdfDocument(str(pdf))
    page = doc[page_idx]
    W, H = page.get_size()  # points
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
        # Convert band rows (raster) to PDF points (y from TOP):
        top_pt = y0 / SCALE
        bottom_pt = y1 / SCALE
        # pypdfium2 get_text_bounded(left, bottom, right, top) in points, y from bottom
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
    work = Path(tempfile.mkdtemp(prefix="core94-e-"))
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
    tl = text_bands(ta, 0)
    pl = text_bands(pr, 0)
    print(f"TA page-1: {len(tl)} lines, Pr page-1: {len(pl)} lines\n")
    print(f"{'TA':>60} | {'Pr':>60}")
    for i in range(max(len(tl), len(pl))):
        a = tl[i] if i < len(tl) else ""
        b = pl[i] if i < len(pl) else ""
        print(f"{a[:60]:>60} | {b[:60]}")
    print(f"\nworkdir kept: {work}")


if __name__ == "__main__":
    main()
