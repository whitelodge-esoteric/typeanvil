#!/usr/bin/env python3
"""Count navy pixels (#17547A = 23,84,122) per page of a Typeanvil PDF."""
import sys
import pypdfium2 as pdfium
from PIL import Image

NAVY = (23, 84, 122)
TOL = 12  # tolerance per channel


def close(c, t):
    return all(abs(c[i] - t[i]) <= TOL for i in range(3))


pdf = pdfium.PdfDocument(sys.argv[1])
for i in range(len(pdf)):
    page = pdf[i]
    # 120 DPI
    scale = 120.0 / 72.0
    bmp = page.render(scale=scale)
    img = bmp.to_pil().convert("RGB")
    w, h = img.size
    px = img.load()
    count = 0
    minx, miny, maxx, maxy = w, h, -1, -1
    for y in range(h):
        for x in range(w):
            if close(px[x, y], NAVY):
                count += 1
                if x < minx: minx = x
                if x > maxx: maxx = x
                if y < miny: miny = y
                if y > maxy: maxy = y
    print(f"page {i}: {w}x{h}px navy={count} bbox=({minx},{miny})-({maxx},{maxy})")
print(f"pages: {len(pdf)}")
