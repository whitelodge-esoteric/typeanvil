#!/usr/bin/env python3
"""CORE-94 probe M: per-line glue stretch (inter-word gap) at 180pt.

Measures the average inter-word gap on each justified line via charbox
positions. If TA's gaps are much larger than Prince's on some lines, TA is
stretching glue harder (packing more words per line) — the K-P total-fit
signature.
"""

import subprocess
import tempfile
from pathlib import Path

import pypdfium2 as pdfium

ROOT = Path("/Users/elijah/workspace/typeanvil.worktrees/core-94-line-breaking")
TA_BIN = ROOT / "engine/target/debug/typeanvil"
PRINCE_SH = ROOT / "scripts/render-prince.sh"

WORDS = ("lorem ipsum dolor sit amet consectetur adipiscing elit sed do "
         "eiusmod tempor incididunt ut labore et dolore magna aliqua ut enim "
         "ad minim veniam quis nostrud exercitation ullamco laboris nisi ut "
         "aliquip ex ea commodo consequat duis aute irure dolor in "
         "reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla "
         "pariatur excepteur sint occaecat cupidatat non proident sunt in "
         "culpa qui officia deserunt mollit anim id est laborum " * 2).strip()

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


def line_gap_stats(pdf: Path, page_idx: int = 0) -> list[float]:
    """Median inter-word gap per text line on a page (charbox-based)."""
    doc = pdfium.PdfDocument(str(pdf))
    page = doc[page_idx]
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
        if len(cs) < 5:
            continue
        gaps = []
        for a, b in zip(cs, cs[1:]):
            g = b["l"] - a["r"]
            if g > 0:
                gaps.append(g)
        if gaps:
            gaps.sort()
            out.append(round(gaps[len(gaps) // 2], 2))
    return out


def render(html: Path, out: Path, prince: bool, geom: list[str]) -> bool:
    if prince:
        r = subprocess.run([str(PRINCE_SH), str(html), *geom, "-o", str(out)],
                           capture_output=True, text=True)
    else:
        r = subprocess.run([str(TA_BIN), "render", str(html), *geom, "-o", str(out)],
                           capture_output=True, text=True)
    return r.returncode == 0


def main():
    work = Path(tempfile.mkdtemp(prefix="core94-m-"))
    print(f"probe workdir: {work}\n")
    html = work / "lorem.html"
    html.write_text(TMPL.format(words=WORDS))
    for w_in, label in [(5.0, "288pt"), (3.5, "180pt")]:
        geom = ["--page-width", f"{w_in}in", "--page-height", "3in",
                "--margin-top", "0.5in", "--margin-right", "0.5in",
                "--margin-bottom", "0.5in", "--margin-left", "0.5in"]
        ta = work / f"ta-{label}.pdf"
        pr = work / f"pr-{label}.pdf"
        render(html, ta, False, geom)
        render(html, pr, True, geom)
        tg = line_gap_stats(ta)
        pg = line_gap_stats(pr)
        import statistics
        tm = statistics.median(tg) if tg else 0
        pm = statistics.median(pg) if pg else 0
        print(f"{label}: TA median gap {tm}pt (max {max(tg) if tg else 0}) | "
              f"Pr median gap {pm}pt (max {max(pg) if pg else 0})")
        print(f"  TA gaps: {tg[:8]}")
        print(f"  Pr gaps: {pg[:8]}")
    print(f"\nworkdir kept: {work}")


if __name__ == "__main__":
    main()
