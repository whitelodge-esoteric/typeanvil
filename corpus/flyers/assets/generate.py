#!/usr/bin/env python3
"""Generate the product-flyer image grid tiles (CORE-492).

Writes six 600x450 abstract tiles into corpus/flyers/assets/ named
tile-1.png .. tile-6.png. Deterministic: pure math, no randomness. Authored
in-repo under the repository license (AGPL-3.0-only). Needs `pillow`.
"""
from pathlib import Path

from PIL import Image, ImageDraw

OUT = Path(__file__).resolve().parent
W, H = 600, 450
PALETTES = [
    ((14, 116, 144), (240, 253, 250)),
    ((234, 88, 12), (255, 237, 213)),
    ((79, 70, 229), (224, 231, 255)),
    ((22, 163, 74), (220, 252, 231)),
    ((190, 18, 60), (255, 228, 230)),
    ((30, 41, 59), (203, 213, 225)),
]


def tile(n, dark, light):
    img = Image.new("RGB", (W, H), dark)
    d = ImageDraw.Draw(img)
    for y in range(H):
        t = y / (H - 1)
        c = tuple(int(dark[i] + (light[i] - dark[i]) * t * 0.55) for i in range(3))
        d.line([(0, y), (W, y)], fill=c)
    for k in range(1, 6):
        r = 40 * k + 10 * n
        d.ellipse([W // 2 - r, H // 2 - r, W // 2 + r, H // 2 + r],
                  outline=light, width=3 if k % 2 else 8)
    d.rectangle([0, H - 14, W, H], fill=light)
    img.save(OUT / f"tile-{n}.png")


if __name__ == "__main__":
    for i, (dk, lt) in enumerate(PALETTES, start=1):
        tile(i, dk, lt)
    print("tiles written to", OUT)
