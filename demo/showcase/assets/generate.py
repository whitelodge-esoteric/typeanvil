#!/usr/bin/env python3
"""Generate deterministic showcase assets (CORE-148).

Writes into demo/showcase/assets/:
  report-throughput.png   — quarterly throughput bar chart (print-res)
  report-latency.png      — latency trend line chart (print-res)
  poster-gradient.png     — full-bleed abstract gradient for rich media
  ofl.txt                 — SIL OFL 1.1 license text for DejaVu fonts

Deterministic: pure math, no randomness, no timestamps. Re-running on the
same tree produces byte-identical PNGs (fixed zlib level via PIL defaults).
"""
from pathlib import Path

from PIL import Image, ImageDraw

OUT = Path(__file__).resolve().parent  # this script lives in demo/showcase/assets/

# 300 DPI print sizes: full text width is 7in => 2100px; figures at 1800px
# wide keep headroom for captions.
CHART_W, CHART_H = 1800, 1050
POSTER_W, POSTER_H = 2550, 3300  # full Letter page bleed

TA_BLUE = (23, 84, 122)
TA_ACCENT = (232, 119, 34)
GRID = (208, 213, 218)
INK = (40, 44, 52)


def xcoord(i, n, left, right):
    return left + (right - left) * i / max(n - 1, 1)


def bar_chart():
    """Quarterly throughput: 8 quarters, ascending with one dip."""
    img = Image.new("RGB", (CHART_W, CHART_H), "white")
    d = ImageDraw.Draw(img)
    left, right, top, bottom = 180, CHART_W - 90, 100, CHART_H - 160
    vals = [412, 448, 431, 502, 558, 547, 630, 704]
    labels = ["Q1'25", "Q2'25", "Q3'25", "Q4'25", "Q1'26", "Q2'26", "Q3'26", "Q4'26"]
    ymax = 800
    # gridlines + y labels
    for gy in range(0, ymax + 1, 200):
        y = bottom - (bottom - top) * gy / ymax
        d.line([(left, y), (right, y)], fill=GRID, width=2)
        d.text((left - 150, y - 18), str(gy), fill=INK)
    bw = (right - left) / len(vals) * 0.55
    for i, v in enumerate(vals):
        x0 = xcoord(i, len(vals), left, right) - bw / 2
        y0 = bottom - (bottom - top) * v / ymax
        color = TA_ACCENT if i == len(vals) - 1 else TA_BLUE
        d.rectangle([x0, y0, x0 + bw, bottom], fill=color)
        d.text((x0 + bw / 2 - 55, y0 - 44), str(v), fill=INK)
        d.text((x0 + bw / 2 - 60, bottom + 24), labels[i], fill=INK)
    d.line([(left, top), (left, bottom)], fill=INK, width=3)
    d.line([(left, bottom), (right, bottom)], fill=INK, width=3)
    OUT.mkdir(parents=True, exist_ok=True)
    img.save(OUT / "report-throughput.png")


def line_chart():
    """P95 latency trend: two series, log-ish decline then plateau."""
    img = Image.new("RGB", (CHART_W, CHART_H), "white")
    d = ImageDraw.Draw(img)
    left, right, top, bottom = 180, CHART_W - 90, 100, CHART_H - 160
    series = [
        ([182, 154, 131, 104, 88, 76, 71, 69], TA_BLUE),
        ([140, 121, 106, 89, 77, 68, 63, 62], TA_ACCENT),
    ]
    ymax = 200
    for gy in range(0, ymax + 1, 50):
        y = bottom - (bottom - top) * gy / ymax
        d.line([(left, y), (right, y)], fill=GRID, width=2)
        d.text((left - 150, y - 18), str(gy), fill=INK)
    for vals, color in series:
        pts = [
            (xcoord(i, len(vals), left, right),
             bottom - (bottom - top) * v / ymax)
            for i, v in enumerate(vals)
        ]
        d.line(pts, fill=color, width=6, joint="curve")
        for (px, py) in pts:
            d.ellipse([px - 9, py - 9, px + 9, py + 9], fill="white",
                      outline=color, width=5)
    for i, lab in enumerate(["W1", "W2", "W3", "W4", "W5", "W6", "W7", "W8"]):
        d.text((xcoord(i, 8, left, right) - 24, bottom + 24), lab, fill=INK)
    d.line([(left, top), (left, bottom)], fill=INK, width=3)
    d.line([(left, bottom), (right, bottom)], fill=INK, width=3)
    img.save(OUT / "report-latency.png")


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
    bar_chart()
    line_chart()
    poster_gradient()
    print("assets written to", OUT)
