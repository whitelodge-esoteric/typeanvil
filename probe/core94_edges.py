#!/usr/bin/env python3
"""CORE-94 probe N: actual content box edges per engine.

Renders a full-width justified line and measures the RIGHT edge of the ink.
If Prince's right edge is consistently LEFT of TA's, Prince's content box
(or its justification target) is narrower.
"""

import subprocess
import tempfile
from pathlib import Path

import pypdfium2 as pdfium

ROOT = Path("/Users/elijah/workspace/typeanvil.worktrees/core-94-line-breaking")
TA_BIN = ROOT / "engine/target/debug/typeanvil"
PRINCE_SH = ROOT / "scripts/render-prince.sh"

TMPL = """<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  @page {{ margin: 0.5in; }}
  body {{ font-family: Arial, sans-serif; font-size: 10pt; line-height: 1.6;
         margin: 0; }}
  p {{ text-align: justify; hyphens: none; margin: 0; }}
</style>
</head>
<body>
<p>{words}</p>
</body>
</html>
"""

WORDS = ("The quick brown fox jumps over the lazy dog while the morning sun "
         "rises slowly above the hills and the birds begin to sing their "
         "gentle songs to welcome the new day that stretches endlessly "
         "before us all. " * 2).strip()


def right_edges(pdf: Path) -> list[float]:
    """Right edge of the last char on each text line, page 1."""
    doc = pdfium.PdfDocument(str(pdf))
    page = doc[0]
    H = page.get_size()[1]
    tp = page.get_textpage()
    n = tp.count_chars()
    chars = []
    for i in range(n):
        l, b, r, t = tp.get_charbox(i)
        if r - l < 0.01:
            continue
        chars.append({"l": l, "b": b, "r": r})
    chars.sort(key=lambda c: (-c["b"], c["l"]))
    lines = []
    for c in chars:
        placed = False
        for ln in lines:
            if abs(ln["b"] - c["b"]) < 3.0:
                ln["chars"].append(c)
                placed = True
                break
        if not placed:
            lines.append({"b": c["b"], "chars": [c]})
    out = []
    for ln in lines:
        cs = sorted(ln["chars"], key=lambda c: c["l"])
        if len(cs) >= 4:
            out.append(round(cs[-1]["r"], 2))
    return out


def render(html: Path, out: Path, prince: bool) -> bool:
    if prince:
        r = subprocess.run([str(PRINCE_SH), str(html), *GEOM, "-o", str(out)],
                           capture_output=True, text=True)
    else:
        r = subprocess.run([str(TA_BIN), "render", str(html), *GEOM, "-o", str(out)],
                           capture_output=True, text=True)
    return r.returncode == 0


GEOM = ["--page-width", "5in", "--page-height", "3in",
        "--margin-top", "0.5in", "--margin-right", "0.5in",
        "--margin-bottom", "0.5in", "--margin-left", "0.5in"]


def main():
    work = Path(tempfile.mkdtemp(prefix="core94-n-"))
    print(f"probe workdir: {work}\n")
    html = work / "edge.html"
    html.write_text(TMPL.format(words=WORDS))
    ta = work / "ta.pdf"
    pr = work / "pr.pdf"
    render(html, ta, False)
    render(html, pr, True)
    te = right_edges(ta)
    pe = right_edges(pr)
    print(f"content right edge should be {36 + 288:.0f}pt (0.5in margin, 5in page)")
    print(f"TA  right edges: {te}")
    print(f"Pr  right edges: {pe}")
    print(f"Pr−TA per line:  {[round(b-a, 2) for a, b in zip(te, pe)]}")
    print(f"\nworkdir kept: {work}")


if __name__ == "__main__":
    main()
