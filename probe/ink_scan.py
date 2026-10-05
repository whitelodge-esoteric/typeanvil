#!/usr/bin/env python3
"""Per-page ink: non-white pixel count + bbox, and dominant colors."""
import sys
import pypdfium2 as pdfium
from PIL import Image

pdf = pdfium.PdfDocument(sys.argv[1])
for i in range(len(pdf)):
    bmp = pdf[i].render(scale=120.0 / 72.0)
    img = bmp.to_pil().convert("RGB")
    w, h = img.size
    px = img.load()
    count = 0
    minx, miny, maxx, maxy = w, h, -1, -1
    colors = {}
    for y in range(h):
        for x in range(w):
            r, g, b = px[x, y]
            if r < 245 or g < 245 or b < 245:  # not near-white
                count += 1
                if x < minx: minx = x
                if x > maxx: maxx = x
                if y < miny: miny = y
                if y > maxy: maxy = y
                key = (r // 32 * 32, g // 32 * 32, b // 32 * 32)
                colors[key] = colors.get(key, 0) + 1
    top = sorted(colors.items(), key=lambda kv: -kv[1])[:5]
    print(f"page {i}: ink={count} bbox=({minx},{miny})-({maxx},{maxy}) top-colors={top}")
print(f"pages: {len(pdf)}")