"""CORE-179 probe: does `border-bottom-color` colour only the bottom side?

Renders the probe fixture with the CLI and samples the middle of each border
band. pypdfium2 render arrays are BGR in this environment (CORE-153), so each
sample prints the raw triple plus both channel interpretations.
"""
import subprocess
import sys
import pathlib

import pypdfium2 as pdfium

CLI = "/work/engine/target/debug/typeanvil"
SRC = pathlib.Path("/work/probe/core179-sidecolors.html")
OUT = pathlib.Path("/work/probe/core179-sidecolors.pdf")

subprocess.run(
    [CLI, "render", str(SRC), "-o", str(OUT),
     "--page-width", "200px", "--page-height", "100px",
     "--margin-top", "0", "--margin-right", "0",
     "--margin-bottom", "0", "--margin-left", "0"],
    check=True,
)

doc = pdfium.PdfDocument(OUT)
page = doc[0]
bitmap = page.render(scale=1.0)
w, h = bitmap.width, bitmap.height
stride = bitmap.stride
nch = bitmap.n_channels
buf = bytes(bitmap.buffer)


def px(x, y):
    off = y * stride + x * nch
    return buf[off], buf[off + 1], buf[off + 2]

# Page 200x100px -> 150x75pt (1px = 0.75pt). The box is 100px wide + 10px
# borders each side => 90pt x 45pt at the page origin; each band is 7.5pt.
samples = {
    "top": (45, 3),
    "bottom": (45, 42),
    "left": (3, 22),
    "right": (87, 22),
}


def name(t):
    r, g, b = int(t[0]), int(t[1]), int(t[2])
    named = {(0, 0, 0): "BLACK", (255, 255, 0): "CYAN(raw)", (0, 255, 255): "CYAN(rgb)"}
    hits = []
    if (r, g, b) in named:
        hits.append(named[(r, g, b)])
    if (b, g, r) in named:
        hits.append(named[(b, g, r)].replace("(", "(as r,g,b=").replace(")", ")"))
    return f"rgb={r},{g},{b} (bgr={b},{g},{r}) {'/'.join(hits)}"


print(f"page {w}x{h}")
for side, (x, y) in samples.items():
    print(f"  {side:7} at ({x:3},{y:3}) -> {name(px(x, y))}")
