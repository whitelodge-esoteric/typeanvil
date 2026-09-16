#!/usr/bin/env python3
"""Build side-by-side TA | Prince montages for every shared page in the demo.

Reads demo/corpus/out/images/<doc>/page-NNN-{ta,pr}.png and writes
demo/corpus/out/montages/<doc>/page-NNN.png (side-by-side, labeled).
PIL available in the repo venv (.venv/bin/python).
"""
import sys
from pathlib import Path

from PIL import Image, ImageDraw

OUT = Path("demo/out")
IMAGES = OUT / "images"
MONTAGES = OUT / "montages"
LABEL_H = 24
FONT_SCALE = 1.6

def make_montage(doc_dir: Path, page: int):
    ta = IMAGES / doc_dir.name / f"page-{page:03d}-ta.png"
    pr = IMAGES / doc_dir.name / f"page-{page:03d}-pr.png"
    if not (ta.exists() and pr.exists()):
        return None
    im_ta = Image.open(ta).convert("RGB")
    im_pr = Image.open(pr).convert("RGB")
    w = im_ta.width + im_pr.width
    h = max(im_ta.height, im_pr.height) + LABEL_H
    canvas = Image.new("RGB", (w, h), "white")
    draw = ImageDraw.Draw(canvas)
    try:
        font = None  # default bitmap font; small but fine for labels
    except Exception:
        font = None
    draw.rectangle([0, 0, w, LABEL_H], fill="black")
    draw.text((10, 4), "TypeAnvil", fill="white")
    draw.text((im_ta.width + 10, 4), "PrinceXML", fill="white")
    canvas.paste(im_ta, (0, LABEL_H))
    canvas.paste(im_pr, (im_ta.width, LABEL_H))
    out_dir = MONTAGES / doc_dir.name
    out_dir.mkdir(parents=True, exist_ok=True)
    out_path = out_dir / f"page-{page:03d}.png"
    canvas.save(out_path)
    return out_path

def main():
    docs = sorted([d for d in IMAGES.iterdir() if d.is_dir()])
    for doc in docs:
        pages = sorted({
            int(p.stem.split("-")[1])
            for p in doc.glob("page-*-ta.png")
        })
        made = 0
        for page in pages:
            out = make_montage(doc, page)
            if out:
                made += 1
        print(f"{doc.name}: {made} montages")

if __name__ == "__main__":
    main()
