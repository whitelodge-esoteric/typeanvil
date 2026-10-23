#!/usr/bin/env python3
"""Generate the deterministic concert-ticket assets (CORE-492).

Writes into corpus/tickets/assets/:
  ticket-qr.svg    - QR code (vector) for the ticket, made with segno
  ticket-qr.png    - the same code as PNG, for the raster fallback probe
  ticket-art.png   - stylized stage artwork (gradient + beams + sun disc)

Needs `segno` and `pillow`. Pure math, no randomness, no timestamps, so a
re-run on the same tree gives byte-identical files. Assets are authored
in-repo under the repository license (AGPL-3.0-only).
"""
from pathlib import Path

import segno
from PIL import Image, ImageDraw

OUT = Path(__file__).resolve().parent
PAYLOAD = "https://example.com/tickets/NLT-2026-A-014-0388"


def qr():
    code = segno.make(PAYLOAD, error="m", micro=False)
    code.save(OUT / "ticket-qr.svg", scale=4, border=1, dark="#111111",
              light="#ffffff", xmldecl=False, svgns=True, nl=False)
    code.save(OUT / "ticket-qr.png", scale=8, border=1, dark="#111111",
              light="#ffffff")


def art():
    w, h = 900, 1200
    img = Image.new("RGB", (w, h))
    px = img.load()
    for y in range(h):
        t = y / (h - 1)
        r = int(24 + 200 * t * t)
        g = int(10 + 40 * t)
        b = int(70 + 60 * (1 - t))
        for x in range(w):
            px[x, y] = (r, g, b)
    d = ImageDraw.Draw(img)
    # Light beams fanning from the bottom centre.
    for i in range(-6, 7):
        x2 = w // 2 + i * 150
        d.polygon([(w // 2 - 6, h), (w // 2 + 6, h), (x2 + 40, 0), (x2 - 40, 0)],
                  fill=(60 + abs(i) * 8, 40, 120 + abs(i) * 6))
    # Sun disc with rings.
    cx, cy = w // 2, 430
    for k, rad in enumerate(range(300, 0, -30)):
        c = (255, 120 + k * 12, 60 + k * 10)
        d.ellipse([cx - rad, cy - rad, cx + rad, cy + rad], fill=c)
    img.save(OUT / "ticket-art.png")


if __name__ == "__main__":
    qr()
    art()
    print("assets written to", OUT)
