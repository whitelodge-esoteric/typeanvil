#!/usr/bin/env python3
"""CORE-94 probe L: exact break positions at 180pt width (worst case).

Renders the lorem paragraph at 180pt content width (3.5in page, 0.5in
margins), extracts per-line text via raster bands, and prints them side by
side. The break-position difference reveals the mechanism.
"""

import re
import subprocess
import tempfile
from pathlib import Path

import pypdfium2 as pdfium

ROOT = Path("/Users/elijah/workspace/typeanvil.worktrees/core-94-line-breaking")
TA_BIN = ROOT / "engine/target/debug/typeanvil"
PRINCE_SH = ROOT / "scripts/render-prince.sh"

WORDS = ("lorem ipsum dolor sit amet consectetur adipiscing elit sed do "
         "eiusmod tempor incididunt ut labore et dolore magna aliqua ut enim "
         "ad minim veniam quis nostrud exercitation ullamco laboris nisi ut "
         "aliquip ex ea commodo consequat duis aute irure dolor in "
         "reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla "
         "pariatur excepteur sint occaecat cupidatat non proident sunt in "
         "culpa qui officia deserunt mollit anim id est laborum " * 4).strip()

TMPL = """<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  @page {{ margin: 0.5in; }}
  body {{ font-family: Arial, sans-serif; font-size: 10pt; line-height: 1.6;
         margin: 0; }}
  p {{ text-align: justify; hyphens: none; margin: 0; }}
</style>
</head>
<body>
<p>{words}</p>
</body>
</html>
"""

SCALE = 2


def text_bands_all_pages(pdf: Path) -> list[str]:
    doc = pdfium.PdfDocument(str(pdf))
    out = []
    for pi in range(len(doc)):
        page = doc[pi]
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


def render(html: Path, out: Path, prince: bool, geom: list[str]) -> bool:
    if prince:
        r = subprocess.run([str(PRINCE_SH), str(html), *geom, "-o", str(out)],
                           capture_output=True, text=True)
    else:
        r = subprocess.run([str(TA_BIN), "render", str(html), *geom, "-o", str(out)],
                           capture_output=True, text=True)
    return r.returncode == 0


def main():
    work = Path(tempfile.mkdtemp(prefix="core94-l-"))
    print(f"probe workdir: {work}\n")
    html = work / "lorem.html"
    html.write_text(TMPL.format(words=WORDS))
    geom = ["--page-width", "3.5in", "--page-height", "3in",
            "--margin-top", "0.5in", "--margin-right", "0.5in",
            "--margin-bottom", "0.5in", "--margin-left", "0.5in"]
    ta = work / "ta.pdf"
    pr = work / "pr.pdf"
    render(html, ta, False, geom)
    render(html, pr, True, geom)
    tl = text_bands_all_pages(ta)
    pl = text_bands_all_pages(pr)
    print(f"TA {len(tl)} lines, Pr {len(pl)} lines\n")
    # Show first 12 lines
    print(f"{'TA':>58} | {'Pr':>58}")
    for i in range(12):
        a = tl[i] if i < len(tl) else ""
        b = pl[i] if i < len(pl) else ""
        print(f"{a[:58]:>58} | {b[:58]}")
    print(f"\nworkdir kept: {work}")


if __name__ == "__main__":
    main()
