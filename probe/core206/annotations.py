#!/usr/bin/env python3
"""Probe link-annotation and glyph APIs on real engine output (CORE-206).

    scripts/dev-container.sh bash -c 'python3 /work/probe/core206/annotations.py'

pypdfium2 5.13.0 has no `PdfPage.get_links()` (removed after v4) and binds PDFium
with ctypes, not cffi. `PdfPage` implements `_as_parameter_`, so it passes
directly wherever a page handle is expected. This probe proves the calls the gate
needs for `link_target`:

* `FPDFPage_GetAnnotCount` / `FPDFPage_GetAnnot` / `FPDFAnnot_GetSubtype`
* `FPDFAnnot_GetLink` -> `FPDFLink_CountWebLinks` / `FPDFLink_GetURL`
* `FPDFLink_GetAnnotRect` (PDF space, bottom-left origin)
* `page.get_textpage()` glyph boxes for the orientation axis signal

The URL comes back as UTF-16LE without a BOM.
"""

from __future__ import annotations

import ctypes
import subprocess
import tempfile
from pathlib import Path

import pypdfium2 as pdfium
from pypdfium2 import raw

CLI = "/work/engine/target/debug/typeanvil"
FIXTURES = Path("/work/harness/direct_fixtures")
WPT = Path("/main/.wpt")


def render(html: Path, out: Path) -> None:
    subprocess.run(
        [
            CLI, "render", str(html),
            "--page-width", "5in", "--page-height", "3in",
            "--margin-top", "0.5in", "--margin-right", "0.5in",
            "--margin-bottom", "0.5in", "--margin-left", "0.5in",
            "-o", str(out),
        ],
        check=True, capture_output=True, timeout=120,
    )


def page_links(
    page, doc
) -> list[tuple[str, tuple[float, float, float, float] | None]]:
    """Return (uri, rect_pdf) for every link annotation with a URI action.

    `FPDFAnnot_GetLink` yields an `FPDF_LINK` (annotation link), which is a
    different handle type from the `FPDF_PAGELINK` that `FPDFLink_GetURL`
    expects. The URI of an annotation link comes from its action instead:
    `FPDFLink_GetAction` -> `FPDFAction_GetType == PDFACTION_URI` ->
    `FPDFAction_GetURIPath`.
    """
    out: list[tuple[str, tuple[float, float, float, float] | None]] = []
    count = raw.FPDFPage_GetAnnotCount(page)
    for i in range(count):
        annot = raw.FPDFPage_GetAnnot(page, i)
        if not annot:
            continue
        try:
            if raw.FPDFAnnot_GetSubtype(annot) != raw.FPDF_ANNOT_LINK:
                continue
            link = raw.FPDFAnnot_GetLink(annot)
            if not link:
                continue
            rect_obj = raw.FS_RECTF()
            rect = None
            if raw.FPDFLink_GetAnnotRect(link, ctypes.byref(rect_obj)):
                rect = (rect_obj.left, rect_obj.top, rect_obj.right, rect_obj.bottom)
            action = raw.FPDFLink_GetAction(link)
            if action and raw.FPDFAction_GetType(action) == raw.PDFACTION_URI:
                needed = raw.FPDFAction_GetURIPath(doc, action, None, 0)
                buf = ctypes.create_string_buffer(needed)
                raw.FPDFAction_GetURIPath(doc, action, buf, needed)
                # FPDFAction_GetURIPath returns UTF-8 (FPDFLink_GetURL, a
                # different call, returns UTF-16LE).
                uri = buf.raw.decode("utf-8", errors="replace").rstrip("\x00")
                out.append((uri, rect))
        finally:
            raw.FPDFPage_CloseAnnot(annot)
    return out


def glyph_axis(page) -> tuple[int, float, float, str, float, float, float, float]:
    """Report glyph metrics for three candidate axis rules.

    Returns (sampled, sum_box_w, sum_box_h, box_rule, sum_dx, sum_dy,
    center_rule_dx, center_rule_dy) where the center rule sums the absolute
    centre-to-centre deltas of consecutive glyphs (a run advancing in y is
    vertical, a run advancing in x is horizontal).
    """
    textpage = page.get_textpage()
    try:
        nchars = textpage.count_chars()
        boxes = []
        for ci in range(min(nchars, 400)):
            box = textpage.get_charbox(ci)
            if not box:
                continue
            x0, y0, x1, y1 = box
            if x1 - x0 <= 0 or y1 - y0 <= 0:
                continue
            boxes.append((x0, y0, x1, y1))
        if not boxes:
            return 0, 0.0, 0.0, "none", 0.0, 0.0, 0.0, 0.0
        sum_w = sum(b[2] - b[0] for b in boxes)
        sum_h = sum(b[3] - b[1] for b in boxes)
        box_rule = "horizontal" if sum_w >= sum_h else "vertical"
        sum_dx = sum_dy = 0.0
        for a, b in zip(boxes, boxes[1:]):
            ax, ay = (a[0] + a[2]) / 2.0, (a[1] + a[3]) / 2.0
            bx, by = (b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0
            sum_dx += abs(bx - ax)
            sum_dy += abs(by - ay)
        return len(boxes), sum_w, sum_h, box_rule, sum_dx, sum_dy, sum_dx, sum_dy
    finally:
        textpage.close()


def probe(label: str, html: Path) -> None:
    print(f"=== {label} ===")
    with tempfile.TemporaryDirectory() as td:
        pdf = Path(td) / "out.pdf"
        render(html, pdf)
        doc = pdfium.PdfDocument(str(pdf))
        try:
            for i, page in enumerate(doc):
                w, h = page.get_size()
                links = page_links(page, doc)
                print(f"  page {i}: size=({w:.1f},{h:.1f}) links={len(links)}")
                for uri, rect in links:
                    print(f"    uri={uri!r}")
                    if rect is None:
                        print("    rect=None")
                        continue
                    x0, top, x1, bottom = rect
                    print(f"    rect_pdf={tuple(round(v, 2) for v in rect)}")
                    print(
                        "    rect_topleft="
                        f"({round(x0, 2)}, {round(h - top, 2)}, "
                        f"{round(x1, 2)}, {round(h - bottom, 2)})"
                    )
                (
                    sampled, tw, th, box_rule, sdx, sdy, _, _
                ) = glyph_axis(page)
                center_rule = "horizontal" if sdx >= sdy else "vertical"
                print(f"    glyphs={sampled} sum_box_w={tw:.1f} sum_box_h={th:.1f}")
                print(
                    f"    box_rule={box_rule}  sum_dx={sdx:.1f} sum_dy={sdy:.1f}"
                    f"  center_rule={center_rule}"
                )
        finally:
            doc.close()
    print()


def main() -> int:
    probe("report-links (external link)", FIXTURES / "report-links.html")
    probe("invoice (no link)", Path("/work/demo/corpus/invoice.html"))
    probe(
        "orthogonal-writing-003 (wpt, vertical-rl subtree)",
        WPT / "css/css-page/page-name-orthogonal-writing-003-print.html",
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
