#!/usr/bin/env python3
"""Generate the deterministic poster asset (CORE-148, trimmed in CORE-488).

Writes into library/posters/assets/:
  poster-gradient.png     — full-bleed abstract gradient for the rich media poster

The chart and invoice-mark generators that lived here belonged to the showcase
report/journal/invoice fixtures dropped in the CORE-488 dedupe (their
corpus twins use assets/report-q3-revenue.png); they remain in git history.

Deterministic: pure math, no randomness, no timestamps. Re-running on the
same tree produces byte-identical PNGs (fixed zlib level via PIL defaults).
"""
from pathlib import Path

from PIL import Image, ImageDraw

OUT = Path(__file__).resolve().parent  # this script lives in library/posters/assets/

POSTER_W, POSTER_H = 2550, 3300  # full Letter page bleed


def poster_gradient():
    """Full-bleed diagonal gradient with concentric arcs — rich media hero."""
    img = Image.new("RGB", (POSTER_W, POSTER_H))
    px = img.load()
    # Diagonal gradient between two brand-ish tones; pure per-pixel math.
    for y in range(POSTER_H):
        for x in range(0, POSTER_W, 8):
            t = (x * 0.6 + y * 0.4) / (POSTER_W + POSTER_H)
            r = int(16 + (250 - 16) * t)
            g = int(28 + (120 - 28) * t * t)
            b = int(64 + (60 - 64) * (1 - t))
            for dx in range(8):
                if x + dx < POSTER_W:
                    px[x + dx, y] = (r, g, b)
    d = ImageDraw.Draw(img)
    # Concentric rings anchored bottom-right.
    for i in range(12):
        rad = 260 + i * 210
        w = 26 if i % 2 == 0 else 8
        d.arc([POSTER_W - rad, POSTER_H - rad, POSTER_W + rad, POSTER_H + rad],
              start=180, end=270, fill=(255, 255, 255, 220), width=w)
    img.save(OUT / "poster-gradient.png")


if __name__ == "__main__":
    poster_gradient()
    print("assets written to", OUT)
