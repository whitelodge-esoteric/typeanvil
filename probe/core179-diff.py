"""CORE-179 probe: per-pixel diff of two pages rendered by the CLI.

Reports the differing-pixel count and the bounding box plus a coarse
row/column histogram, so a residual can be attributed to ONE region instead
of guessed at.

usage: python3 probe/core179-diff.py <a.pdf> <b.pdf>
"""
import pathlib
import sys

import pypdfium2 as pdfium


def raster(path):
    doc = pdfium.PdfDocument(path)
    page = doc[0]
    bm = page.render(scale=1.0)
    return bm.width, bm.height, bm.stride, bm.n_channels, bytes(bm.buffer)


wa, ha, sa, na, ba = raster(sys.argv[1])
wb, hb, sb, nb, bb = raster(sys.argv[2])
assert (wa, ha) == (wb, hb), f"size mismatch {wa}x{ha} vs {wb}x{hb}"

x0, y0, x1, y1 = wa, ha, -1, -1
n = 0
rows, cols = {}, {}
for y in range(ha):
    for x in range(wa):
        oa, ob = y * sa + x * na, y * sb + x * nb
        if ba[oa:oa + 3] != bb[ob:ob + 3]:
            n += 1
            x0, y0 = min(x0, x), min(y0, y)
            x1, y1 = max(x1, x), max(y1, y)
            rows[y] = rows.get(y, 0) + 1
            cols[x] = cols.get(x, 0) + 1

print(f"differing pixels: {n} of {wa*ha}")
if n == 0:
    raise SystemExit(0)
print(f"bbox x[{x0}-{x1}] y[{y0}-{y1}]")


def bands(counts, limit=12):
    """Collapse a histogram into contiguous runs with a total."""
    out, start, total = [], None, 0
    for k in sorted(counts):
        if start is None:
            start, total = k, counts[k]
        elif k == last + 1:
            total += counts[k]
        else:
            out.append((start, last, total))
            start, total = k, counts[k]
        last = k
    if start is not None:
        out.append((start, last, total))
    out.sort(key=lambda t: -t[2])
    return out[:limit]


print("worst pixel-rows (row, count):")
for a, b, t in bands(rows, 6):
    print(f"   rows {a}-{b}: {t}")
print("worst pixel-columns (col, count):")
for a, b, t in bands(cols, 6):
    print(f"   cols {a}-{b}: {t}")
