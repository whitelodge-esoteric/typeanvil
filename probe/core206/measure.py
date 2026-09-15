#!/usr/bin/env python3
"""Measure emitted PDFs directly, for CORE-206's reviewed direct-check manifest.

Parent-owned probe. Runs inside the dev container:

    scripts/dev-container.sh bash -c 'python3 /work/probe/core206/measure.py'

It renders each input with the worktree's CLI binary at the WPT print geometry
(5in x 3in, 0.5in margins) and reports, per page: page size in points and the
dark-pixel fraction inside named rectangles. Rectangles use points with a
top-left origin, the same convention as `harness/direct.py`.

Purpose: confirm each expectation in `harness/direct_manifest.json` against a
real render BEFORE the gate is trusted. Measured truth goes in the manifest;
guessed truth does not.
"""

from __future__ import annotations

import subprocess
import sys
import tempfile
from pathlib import Path

import pypdfium2 as pdfium
from PIL import Image

CLI = "/work/engine/target/debug/typeanvil"
WPT = Path("/main/.wpt")
WORKTREE = Path("/work")
DPI = 96
BASE_DPI = 72.0

# (label, input path, [(rect name, x0, y0, x1, y1)])
CASES = [
    (
        "abspos-margins (fixture)",
        WORKTREE / "harness/direct_fixtures/abspos-margins.html",
        [
            ("expected box (54,54)-(126,90)", 54.0, 54.0, 126.0, 90.0),
            ("margin corner (0,0)-(50,50)", 0.0, 0.0, 50.0, 50.0),
            ("margins-ignored box (18,18)-(90,54)", 18.0, 18.0, 90.0, 54.0),
        ],
    ),
    (
        "orthogonal-writing-003 (wpt)",
        WPT / "css/css-page/page-name-orthogonal-writing-003-print.html",
        [],
    ),
    (
        "orthogonal-writing-003 ref (wpt)",
        WPT / "css/css-page/page-name-orthogonal-writing-003-print-ref.html",
        [],
    ),
    (
        "invoice (corpus)",
        WORKTREE / "demo/corpus/invoice.html",
        [],
    ),
    (
        "report (corpus)",
        WORKTREE / "demo/corpus/report.html",
        [],
    ),
    (
        "report-links (fixture)",
        WORKTREE / "harness/direct_fixtures/report-links.html",
        [],
    ),
]


def render(html: Path, out: Path) -> None:
    args = [
        CLI,
        "render",
        str(html),
        "--page-width", "5in",
        "--page-height", "3in",
        "--margin-top", "0.5in",
        "--margin-right", "0.5in",
        "--margin-bottom", "0.5in",
        "--margin-left", "0.5in",
        "-o", str(out),
    ]
    subprocess.run(args, check=True, capture_output=True, timeout=120)


def dark_fraction(img: Image.Image, rect, dpi: int) -> float:
    """Fraction of pixels below 0.5 relative luminance inside a top-left rect."""
    x0, y0, x1, y1 = rect
    scale = dpi / BASE_DPI
    box = (
        max(0, round(x0 * scale)),
        max(0, round(y0 * scale)),
        min(img.width, round(x1 * scale)),
        min(img.height, round(y1 * scale)),
    )
    if box[2] <= box[0] or box[3] <= box[1]:
        return float("nan")
    crop = img.crop(box).convert("L")
    # Histogram bins are exact: with L mode, 0-127 is luminance below 0.5.
    dark = sum(crop.histogram()[:128])
    return dark / (crop.width * crop.height)


def main() -> int:
    for label, html, rects in CASES:
        if not html.exists():
            print(f"{label}: MISSING {html}")
            continue
        with tempfile.TemporaryDirectory() as td:
            pdf_path = Path(td) / "out.pdf"
            try:
                render(html, pdf_path)
            except subprocess.CalledProcessError as exc:
                print(f"{label}: render FAILED rc={exc.returncode} {exc.stderr[:300]!r}")
                continue
            doc = pdfium.PdfDocument(str(pdf_path))
            try:
                pages = list(doc)
                print(f"{label}: {len(pages)} page(s)")
                for i, page in enumerate(pages):
                    w, h = page.get_size()
                    print(f"    page {i}: size_pt=({w:.2f}, {h:.2f})")
                    bitmap = page.render(scale=DPI / BASE_DPI)
                    try:
                        img = bitmap.to_pil().convert("RGB")
                    finally:
                        bitmap.close()
                    print(f"      raster={img.width}x{img.height}px")
                    for name, x0, y0, x1, y1 in rects:
                        frac = dark_fraction(img, (x0, y0, x1, y1), DPI)
                        print(f"      dark_fraction[{name}] = {frac:.4f}")
                    # ink rows: a cheap text/side count for orientation questions
                    print()
            finally:
                doc.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
