"""CORE-201: histogram of colours + bounding boxes per colour.

usage: python3 probe/core201-hist.py <engine-binary> <out-pdf>
"""
import pathlib
import subprocess
import sys
from collections import defaultdict

import pypdfium2 as pdfium

CLI = sys.argv[1]
out = pathlib.Path(sys.argv[2])
src = pathlib.Path("/work/probe/core179-sidecolors.html")

subprocess.run(
    [CLI, "render", str(src), "-o", str(out),
     "--page-width", "5in", "--page-height", "3in",
     "--margin-top", "0.5in", "--margin-right", "0.5in",
     "--margin-bottom", "0.5in", "--margin-left", "0.5in"],
    check=True,
)

doc = pdfium.PdfDocument(out)
print(f"pages: {len(doc)}")
page = doc[0]
print(f"page size: {page.get_size()}")
bm = page.render(scale=1.0)
w, h, stride, nch = bm.width, bm.height, bm.stride, bm.n_channels
print(f"bitmap: {w}x{h} stride={stride} nch={nch}")
buf = bytes(bm.buffer)

# Count opaque, non-white, non-black pixels by colour, with bbox.
counts = defaultdict(int)
bbox = defaultdict(lambda: [10**9, 10**9, -1, -1])
for yy in range(h):
    row = yy * stride
    for xx in range(w):
        o = row + xx * nch
        c = (buf[o], buf[o + 1], buf[o + 2])
        if c[0] < 250 or c[1] < 250 or c[2] < 250:
            counts[c] += 1
            bb = bbox[c]
            if xx < bb[0]:
                bb[0] = xx
            if yy < bb[1]:
                bb[1] = yy
            if xx > bb[2]:
                bb[2] = xx
            if yy > bb[3]:
                bb[3] = yy

for c in sorted(counts, key=lambda c: -counts[c]):
    bb = bbox[c]
    print(f"rgb={c} count={counts[c]} bbox=({bb[0]},{bb[1]})-({bb[2]},{bb[3]})")