"""CORE-201: which border bands paint cyan?

Renders probe/core179-sidecolors.html with the given engine binary and
reports the colour of each of the four border bands (top/right/bottom/left)
by sampling the band centers.

usage: python3 probe/core201-bandcheck.py <engine-binary> <out-pdf>
"""
import pathlib
import subprocess
import sys

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
if len(doc) == 0:
    sys.exit(1)
pg = doc[0]
print(f"page size: {pg.get_size()}")
page = doc[0]
# Render at 1.0 = 72 dpi; page is 200px x 100px CSS = 150x75 pt.
bm = page.render(scale=2.0)
w, h, stride, nch = bm.width, bm.height, bm.stride, bm.n_channels
buf = bytes(bm.buffer)

def px(x, y):
    """Pixel colour at (x, y) in PDF pt -> scaled coords (top-left origin)."""
    sx = int(x * 2.0)
    sy = int(y * 2.0)
    if sx < 0 or sy < 0 or sx >= w or sy >= h:
        return None
    o = sy * stride + sx * nch
    return (buf[o], buf[o + 1], buf[o + 2])

def is_cyan(c):
    return c is not None and c[0] < 60 and c[1] > 200 and c[2] > 200

# The box: .box is 100x40px starting at body margin 0 (page margin 0).
# Box border box: x 0..100px(75pt), y 0..40px(30pt), border 10px(7.5pt).
x0, y0, bw, bh = 0.0, 0.0, 75.0, 30.0
b = 7.5  # border width in pt

# Sampled band centres:
# top: y = b/2, x = bw/2  (inside the top band)
# bottom: y = bh - b/2, x = bw/2
# left: x = b/2, y = bh/2
# right: x = bw - b/2, y = bh/2
# Interior (content centre): x = bw/2, y = bh/2
for name, (sx, sy) in {
    "top": (bw / 2, b / 2),
    "bottom": (bw / 2, bh - b / 2),
    "left": (b / 2, bh / 2),
    "right": (bw - b / 2, bh / 2),
    "interior": (bw / 2, bh / 2),
}.items():
    c = px(x0 + sx, y0 + sy)
    print(f"{name}: {c} {'CYAN' if is_cyan(c) else ''}")