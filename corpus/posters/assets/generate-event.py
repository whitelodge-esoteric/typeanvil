#!/usr/bin/env python3
"""Generate the event-poster artwork (CORE-492).

Writes into corpus/posters/assets/:
  event-poster-art.png - full-bleed night-sky gradient, skyline, spotlight cones

Deterministic: pure math, no randomness, no timestamps. Authored in-repo under
the repository license (AGPL-3.0-only). Needs `pillow`.
"""
from pathlib import Path

from PIL import Image, ImageDraw

OUT = Path(__file__).resolve().parent
W, H = 1275, 1650  # Letter at 150 DPI


def main():
    img = Image.new("RGB", (W, H))
    d = ImageDraw.Draw(img)
    for y in range(H):
        t = y / (H - 1)
        d.line([(0, y), (W, y)],
               fill=(int(12 + 210 * t ** 2.2), int(8 + 60 * t ** 1.5), int(48 + 80 * (1 - t) + 40 * t)))
    # Spotlight cones.
    for i, x0 in enumerate((180, 520, 860, 1120)):
        top = 40 + 30 * (i % 2)
        d.polygon([(x0, H - 330), (x0 + 24, H - 330), (x0 + 24 + (i - 1.5) * 260, top),
                   (x0 + (i - 1.5) * 260 - 120, top)], fill=(70 + 15 * i, 50, 140))
    # Skyline: fixed building table (x, width, height).
    for x, w, h in ((0, 120, 300), (110, 90, 420), (190, 140, 260), (320, 100, 380),
                    (410, 160, 300), (560, 110, 470), (660, 130, 330), (780, 90, 410),
                    (860, 150, 280), (1000, 100, 390), (1090, 185, 320)):
        d.rectangle([x, H - h, x + w, H], fill=(10, 8, 28))
        for wy in range(H - h + 24, H - 20, 44):
            for wx in range(x + 14, x + w - 14, 28):
                if (wx * 7 + wy * 3) % 5 < 2:
                    d.rectangle([wx, wy, wx + 10, wy + 16], fill=(255, 196, 96))
    img.save(OUT / "event-poster-art.png")


if __name__ == "__main__":
    main()
    print("asset written to", OUT)
