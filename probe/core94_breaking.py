#!/usr/bin/env python3
"""CORE-94 probe A: isolate the line-BREAKING difference.

Controlled fixture: ONE paragraph of simple words, no hyphenation, no
headings, no margins. Vary:
  - text-align: justify vs left
  - word length / line width
Measure: number of lines (page count × lines/page), and per-line ink width.

If justify-only diverges → glue/justification tolerance difference.
If left (ragged) ALSO diverges → pure breaking (K-P objective vs Prince).
If neither diverges with simple words → the difference needs prose's
hyphenation or mixed content.
"""

import re
import subprocess
import tempfile
from pathlib import Path

ROOT = Path("/Users/elijah/workspace/typeanvil.worktrees/core-94-line-breaking")
TA_BIN = ROOT / "engine/target/debug/typeanvil"
PRINCE_SH = ROOT / "scripts/render-prince.sh"

GEOM = ["--page-width", "5in", "--page-height", "3in",
        "--margin-top", "0.5in", "--margin-right", "0.5in",
        "--margin-bottom", "0.5in", "--margin-left", "0.5in"]

WORDS = ("alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu "
         "nu xi omicron pi rho sigma tau upsilon phi chi psi omega " * 6).strip()

TMPL = """<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  @page {{ margin: 0.5in; }}
  body {{ font-family: Arial, sans-serif; font-size: 10pt; line-height: 1.6;
         margin: 0; }}
  p {{ text-align: {align}; hyphens: none; margin: 0; }}
</style>
</head>
<body>
<p>{words}</p>
</body>
</html>
"""


def page_count(pdf: Path) -> int:
    raw = pdf.read_bytes()
    m = re.search(rb"/Count\s+(\d+)", raw)
    return int(m.group(1)) if m else -1


def line_count(pdf: Path) -> int:
    """Total text lines across all pages via baseline clustering."""
    import pypdfium2 as pdfium
    doc = pdfium.PdfDocument(str(pdf))
    total = 0
    for pi in range(len(doc)):
        page = doc[pi]
        H = page.get_size()[1]
        tp = page.get_textpage()
        n = tp.count_chars()
        baselines = []
        for i in range(n):
            l, b, r, t = tp.get_charbox(i)
            if r - l < 0.01:
                continue
            baselines.append(round(H - b, 2))
        baselines.sort()
        lines = 0
        prev = None
        for bl in baselines:
            if prev is None or bl - prev > 2.5:
                lines += 1
            prev = bl
        total += lines
    return total


def render(html: Path, out: Path, prince: bool) -> bool:
    if prince:
        r = subprocess.run([str(PRINCE_SH), str(html), *GEOM, "-o", str(out)],
                           capture_output=True, text=True)
    else:
        r = subprocess.run([str(TA_BIN), "render", str(html), *GEOM, "-o", str(out)],
                           capture_output=True, text=True)
    return r.returncode == 0


def main():
    if not TA_BIN.exists():
        print(f"TA binary missing at {TA_BIN}; build first.")
        return
    work = Path(tempfile.mkdtemp(prefix="core94-a-"))
    print(f"probe workdir: {work}\n")
    print(f"{'align':>8} | {'TA lines':>8} | {'Pr lines':>8} | {'Pr-TA':>6}")
    print("-" * 42)
    for align in ["justify", "left"]:
        html = work / f"{align}.html"
        html.write_text(TMPL.format(align=align, words=WORDS))
        ta = work / f"ta-{align}.pdf"
        pr = work / f"pr-{align}.pdf"
        if not render(html, ta, False) or not render(html, pr, True):
            print(f"{align:>8}: render failed")
            continue
        tl = line_count(ta)
        pl = line_count(pr)
        print(f"{align:>8} | {tl:>8} | {pl:>8} | {pl-tl:>6}")
    print(f"\nworkdir kept: {work}")


if __name__ == "__main__":
    main()
