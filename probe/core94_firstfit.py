#!/usr/bin/env python3
"""CORE-94 probe H: first-fit vs total-fit discriminator.

Engineered paragraph where greedy first-fit and K-P total-fit produce
DIFFERENT line counts. Pattern: line 1 greedy can take 4 words but that
leaves 3 words spilling awkwardly; total-fit takes 3 words on line 1 to
get 4 on line 2 → same total but different texture. To force a LINE-COUNT
difference, use a text where greedy's early choice wastes a line.

Simple known case: if words are all ~same width, greedy == optimal. The
difference shows when a long word at a boundary changes everything. Use a
paragraph of mostly-short words with a few long ones at strategic spots.
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

# Short words + a couple of long ones near the wrap boundary.
WORDS = ("aa bb cc dd ee ff gg hh ii jj kk ll mm nn oo pp qq rr ss tt uu vv "
         "ww xx yy zz " * 8).strip()

TMPL = """<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  @page {{ margin: 0.5in; }}
  body {{ font-family: Arial, sans-serif; font-size: 10pt; line-height: 1.6;
         margin: 0; }}
  p {{ text-align: left; hyphens: none; margin: 0; }}
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
    work = Path(tempfile.mkdtemp(prefix="core94-h-"))
    print(f"probe workdir: {work}\n")
    for width_pt in [200.0, 180.0, 160.0, 150.0, 140.0]:
        # vary page width to change the wrap boundary
        geom = ["--page-width", f"{width_pt/72:.2f}in", "--page-height", "3in",
                "--margin-top", "0.5in", "--margin-right", "0.5in",
                "--margin-bottom", "0.5in", "--margin-left", "0.5in"]
        html = work / f"w{int(width_pt)}.html"
        html.write_text(TMPL.format(words=WORDS))
        ta = work / f"ta-w{int(width_pt)}.pdf"
        pr = work / f"pr-w{int(width_pt)}.pdf"

        def rnd(cmd):
            r = subprocess.run(cmd, capture_output=True, text=True)
            return r.returncode == 0

        if not rnd([str(TA_BIN), "render", str(html), *geom, "-o", str(ta)]) or \
           not rnd([str(PRINCE_SH), str(html), *geom, "-o", str(pr)]):
            print(f"w={width_pt}: render failed")
            continue
        tl = text_bands(ta, 0)
        pl = text_bands(pr, 0)
        print(f"w={width_pt:>5}: TA {len(tl)} lines | Pr {len(pl)} lines | "
              f"{'SAME' if len(tl)==len(pl) else 'DIFF'}")
        if len(tl) != len(pl):
            print(f"  TA: {tl[0][:40]}…")
            print(f"  Pr: {pl[0][:40]}…")
    print(f"\nworkdir kept: {work}")


if __name__ == "__main__":
    main()
