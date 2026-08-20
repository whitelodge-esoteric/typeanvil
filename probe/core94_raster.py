#!/usr/bin/env python3
"""CORE-94 probe D: line-ink width distributions (raster, no text parsing).

Renders prose (margin 0, 1.6) through both engines, rasterizes each page,
and for every text line measures the horizontal ink extent (leftmost to
rightmost dark pixel in the text band). This avoids pypdfium2's char-order
scrambling entirely.

Key question: are Prince's justified lines consistently FULL (reaching the
right margin) while TA's fall short — or vice versa?
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

SCALE = 2  # 144 DPI


def line_ink_widths(pdf: Path, page_idx: int = 0) -> list[int]:
    """Ink width in px of each text-line band on a page (raster-based)."""
    doc = pdfium.PdfDocument(str(pdf))
    page = doc[page_idx]
    bmp = page.render(scale=SCALE).to_pil()
    w, h = bmp.size
    px = bmp.load()
    # Find text rows: scan for rows with dark pixels (exclude page margin areas)
    # Content box: 0.5in margins = 0.5*72*2 = 72px at 144dpi.
    m = int(0.5 * 72 * SCALE)
    # Measure per contiguous band of dark rows in the content area.
    row_has_ink = []
    for y in range(m, h - m):
        ink = False
        for x in range(m, w - m):
            if px[x, y][0] < 128:
                ink = True
                break
        row_has_ink.append(ink)
    # Group into bands (lines) — a band is a run of ink rows.
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
    widths = []
    for y0, y1 in bands:
        if y1 - y0 < 3:  # skip noise
            continue
        left = w
        right = 0
        for y in range(y0, y1 + 1):
            for x in range(m, w - m):
                if px[x, y][0] < 128:
                    left = min(left, x)
                    right = max(right, x)
        widths.append(right - left)
    return widths


def render(html: Path, out: Path, prince: bool) -> bool:
    if prince:
        r = subprocess.run([str(PRINCE_SH), str(html), *GEOM, "-o", str(out)],
                           capture_output=True, text=True)
    else:
        r = subprocess.run([str(TA_BIN), "render", str(html), *GEOM, "-o", str(out)],
                           capture_output=True, text=True)
    return r.returncode == 0


def main():
    work = Path(tempfile.mkdtemp(prefix="core94-d-"))
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

    tw = line_ink_widths(ta, 0)
    pw = line_ink_widths(pr, 0)
    content_px = int((5.0 - 1.0) * 72 * SCALE)  # 4in content = 576px at 144dpi

    def stats(widths):
        if not widths:
            return "no lines"
        full = sum(1 for w in widths if w >= content_px - 8)
        return (f"n={len(widths)} full={full} "
                f"median={sorted(widths)[len(widths)//2]} "
                f"min={min(widths)} max={max(widths)}")

    print(f"content width = {content_px}px (at 144dpi)")
    print(f"TA  page-1 text bands: {stats(tw)}")
    print(f"Pr  page-1 text bands: {stats(pw)}")
    print("\nTA band widths:", sorted(tw))
    print("Pr band widths:", sorted(pw))
    print(f"\nworkdir kept: {work}")


if __name__ == "__main__":
    main()
