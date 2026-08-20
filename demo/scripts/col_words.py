#!/usr/bin/env python3
"""Dump words (text + char boxes) from a PDF page, grouped into lines.

For demo triage / parity measurement (CORE-79, CORE-81): reading column
boundaries and detecting wrapped cells from both engines' PDFs via char-box
geometry. Run with the repo venv:  .venv/bin/python demo/scripts/col_words.py <pdf> [page_idx]

Usage: col_words.py <pdf> [page_idx=0] [max_lines=16]
  - Words are printed bottom-up by default (PDF charbox y is bottom-anchored);
    pass a negative max_lines (e.g. -20) to get the TOP of the page instead.
"""
import sys
import pypdfium2 as pdfium


def words_by_line(pdf_path, page_idx=0, max_lines=16, top_down=False):
    pdf = pdfium.PdfDocument(pdf_path)
    page = pdf[page_idx]
    tp = page.get_textpage()
    n = tp.count_chars()
    items = []  # (ytop, xleft, xright, text)
    for i in range(n):
        box = tp.get_charbox(i)
        if box is None:
            continue
        x0, x1, x2, x3 = box
        ch = tp.get_text_range(i, 1)
        ytop = min(x1, x3)
        items.append((ytop, x0, x2, ch))
    items.sort(key=lambda t: (t[0], t[1]))
    lines = []
    for ytop, xl, xr, ch in items:
        if lines and abs(ytop - lines[-1][0]) <= 3.0:
            lines[-1][1].append((xl, xr, ch))
        else:
            lines.append([ytop, [(xl, xr, ch)]])
    out = []
    for ytop, chars in lines:
        chars.sort()
        words = []
        cur = ""
        cx0 = cx1 = None
        for xl, xr, ch in chars:
            if ch.isspace() or (cur and xl - cx1 > 2.0):
                if cur:
                    words.append((cx0, cx1, cur))
                cur = ""
                cx0 = cx1 = None
                if ch.isspace():
                    continue
            if cur == "":
                cx0 = xl
            cur += ch
            cx1 = xr
        if cur:
            words.append((cx0, cx1, cur))
        out.append((round(ytop, 1), words))
    if top_down:
        out.reverse()
    return out[: abs(max_lines)]


if __name__ == "__main__":
    path = sys.argv[1]
    page = int(sys.argv[2]) if len(sys.argv) > 2 else 0
    nlines = int(sys.argv[3]) if len(sys.argv) > 3 else 16
    top_down = nlines < 0
    print(f"== {path} page {page + 1} ({'top-down' if top_down else 'bottom-up'}) ==")
    for ytop, words in words_by_line(path, page, nlines, top_down):
        seg = " | ".join(f"{w}[{xl:6.1f}-{xr:6.1f}]" for xl, xr, w in words)
        print(f"  y={ytop:7.1f}  {seg}")
