#!/usr/bin/env python3
"""Generate the deterministic Q3 revenue chart PNG for report.html (CORE-146).

Hand-rolled with Pillow primitives so the bytes depend only on this script —
no matplotlib font caches. Run once; the PNG is committed.
"""
from PIL import Image, ImageDraw

W, H = 640, 320
SCALE = 2  # render at 2x for crispness, embed at half size

img = Image.new("RGB", (W * SCALE, H * SCALE), "#ffffff")
d = ImageDraw.Draw(img)

months = ["Jul", "Aug", "Sep"]
values = [412, 461, 515]  # kUSD
max_v = 600
left, bottom = 70 * SCALE, 270 * SCALE
top, right = 30 * SCALE, 610 * SCALE

# axes
d.line([(left, top), (left, bottom), (right, bottom)], fill="#333333", width=SCALE)

# gridlines + y labels
for v in range(0, max_v + 1, 150):
    y = bottom - int((v / max_v) * (bottom - top))
    d.line([(left, y), (right, y)], fill="#dddddd", width=SCALE)
    d.text((10 * SCALE, y - 5 * SCALE), str(v), fill="#555555")

# bars
bar_w = 90 * SCALE
gap = (right - left - 3 * bar_w) // 4
for i, (m, v) in enumerate(zip(months, values)):
    x = left + gap + i * (bar_w + gap)
    y = bottom - int((v / max_v) * (bottom - top))
    d.rectangle([x, y, x + bar_w, bottom], fill="#1d3557")
    d.text((x + bar_w // 2 - 12 * SCALE, y - 14 * SCALE), str(v), fill="#1d3557")
    d.text((x + bar_w // 2 - 8 * SCALE, bottom + 8 * SCALE), m, fill="#333333")

img = img.resize((W, H), Image.Resampling.LANCZOS)
img.save("demo/corpus/assets/report-q3-revenue.png", optimize=True)
print("chart written")
