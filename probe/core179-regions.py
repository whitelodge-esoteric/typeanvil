"""CORE-179 probe: where do a fixture's coloured boxes actually land?

Renders one HTML file with the CLI at the harness geometry and reports, for
each of the paint-order fixture's colours, the pixel count and bounding box.
Used to tell "the boxes are in the wrong place / stacked" (layout) from "the
boxes are in the right place but overlap wrongly" (paint order).

usage: python3 probe/core179-regions.py <name-in-probe-dir>
"""
import pathlib
import subprocess
import sys

import pypdfium2 as pdfium

CLI = "/work/engine/target/debug/typeanvil"
name = sys.argv[1]
SRC = pathlib.Path(f"/work/probe/{name}.html")
OUT = pathlib.Path(f"/work/probe/{name}.pdf")

subprocess.run(
    [CLI, "render", str(SRC), "-o", str(OUT),
     "--page-width", "5in", "--page-height", "3in",
     "--margin-top", "0.5in", "--margin-right", "0.5in",
     "--margin-bottom", "0.5in", "--margin-left", "0.5in"],
    check=True,
)

doc = pdfium.PdfDocument(OUT)
print(f"{name}: {len(doc)} page(s)")
page = doc[0]
bm = page.render(scale=1.0)
w, h, stride, nch = bm.width, bm.height, bm.stride, bm.n_channels
buf = bytes(bm.buffer)

# CSS colours used by the paint-order fixtures (opaque).
PALETTE = {
    "pink": (255, 192, 203),
    "hotpink": (255, 105, 180),
    "cyan": (0, 255, 255),
    "yellow": (255, 255, 0),
    "green": (0, 128, 0),
    "red": (255, 0, 0),
    "gray": (221, 221, 221),
}


def classify(r, g, b):
    # pypdfium2 render arrays are BGR in this environment (CORE-153).
    for label, (cr, cg, cb) in PALETTE.items():
        if (r, g, b) == (cr, cg, cb) or (b, g, r) == (cr, cg, cb):
            return label
    return None


stats = {}
for y in range(h):
    row = y * stride
    for x in range(w):
        off = row + x * nch
        label = classify(buf[off], buf[off + 1], buf[off + 2])
        if label is None:
            continue
        s = stats.setdefault(label, [0, w, h, 0, 0])
        s[0] += 1
        s[1] = min(s[1], x)
        s[2] = min(s[2], y)
        s[3] = max(s[3], x)
        s[4] = max(s[4], y)

print(f"  raster {w}x{h}")
for label in sorted(stats, key=lambda k: -stats[k][0]):
    n, x0, y0, x1, y1 = stats[label]
    print(f"  {label:8} {n:7} px  bbox x[{x0}-{x1}] y[{y0}-{y1}]  ({x1-x0+1}x{y1-y0+1})")
