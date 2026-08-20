#!/usr/bin/env python3
"""CORE-90 probe v3: reproduce the ticket evidence on the REAL prose fixture.

Renders demo/corpus/prose.html through TA + Prince at line-height factors
{1.0, 1.2, 1.5, 1.6, 2.0} (body line-height patched per factor) at the demo
geometry, records page counts, and measures per-page line baseline deltas to
localize WHERE the line-box divergence lives (body text vs headings vs
margins).

Also measures the y of every distinct font-size line on page 1, so we can see
if the divergence tracks the 10pt body, the 15pt h1, or the 11pt h2.
"""

import re
import statistics
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path("/Users/elijah/workspace/typeanvil.worktrees/core-90-line-box-height")
TA_BIN = ROOT / "engine/target/debug/typeanvil"
PRINCE_SH = ROOT / "scripts/render-prince.sh"
CORPUS = Path("/Users/elijah/workspace/typeanvil/demo/corpus/prose.html")

FACTORS = [1.0, 1.2, 1.5, 1.6, 2.0]
GEOM = ["--page-width", "5in", "--page-height", "3in",
        "--margin-top", "0.5in", "--margin-right", "0.5in",
        "--margin-bottom", "0.5in", "--margin-left", "0.5in"]


def patch_factor(factor: float) -> str:
    """Return prose.html source with body line-height set to factor."""
    src = CORPUS.read_text()
    return re.sub(r"line-height:\s*[\d.]+", f"line-height: {factor}", src, count=1)


def render_ta(html: Path, out: Path) -> int:
    r = subprocess.run([str(TA_BIN), "render", str(html), *GEOM, "-o", str(out)],
                       capture_output=True, text=True)
    if r.returncode != 0:
        print(f"  TA FAILED: {r.stderr[-500:]}")
    return r.returncode


def render_prince(html: Path, out: Path) -> int:
    r = subprocess.run([str(PRINCE_SH), str(html), *GEOM, "-o", str(out)],
                       capture_output=True, text=True)
    if r.returncode != 0:
        print(f"  PRINCE FAILED: {r.stderr[-500:]}")
    return r.returncode


def page_count(pdf: Path) -> int:
    raw = pdf.read_bytes()
    m = re.search(rb"/Count\s+(\d+)", raw)
    return int(m.group(1)) if m else -1


def line_heights_pg1(pdf: Path) -> list[float]:
    """Line-box heights on page 1: cluster chars by baseline (charbox bottom
    in PDF coords, y from bottom), delta consecutive baselines. Returns the
    sorted unique deltas (most common = body text)."""
    import pypdfium2 as pdfium
    doc = pdfium.PdfDocument(str(pdf))
    page = doc[0]
    tp = page.get_textpage()
    n = tp.count_chars()
    anchors = []
    for i in range(n):
        left, bottom, right, top = tp.get_charbox(i)
        # skip zero-width junk
        if right - left < 0.01:
            continue
        anchors.append(round(bottom, 2))
    anchors.sort(reverse=True)
    # consecutive delta between distinct anchors (dedupe within 0.01)
    deltas = []
    prev = None
    for a in anchors:
        if prev is not None:
            d = prev - a
            if d > 0.5:  # real line spacing, not intra-line glyph jitter
                deltas.append(round(d, 2))
        prev = a
    return deltas


def main():
    if not TA_BIN.exists():
        print(f"TA binary missing at {TA_BIN}; build first.")
        sys.exit(2)
    work = Path(tempfile.mkdtemp(prefix="core90-prose-"))
    print(f"probe workdir: {work}\n")
    print(f"{'factor':>6} | {'TA pgs':>6} | {'Pr pgs':>6} | "
          f"{'TA Δ modes':>40} | {'Pr Δ modes':>40}")
    print("-" * 110)
    for factor in FACTORS:
        html = work / f"prose-{factor}.html"
        html.write_text(patch_factor(factor))
        ta_pdf = work / f"ta-{factor}.pdf"
        pr_pdf = work / f"pr-{factor}.pdf"
        if render_ta(html, ta_pdf) != 0 or render_prince(html, pr_pdf) != 0:
            continue
        ta_pages = page_count(ta_pdf)
        pr_pages = page_count(pr_pdf)
        ta_d = line_heights_pg1(ta_pdf)
        pr_d = line_heights_pg1(pr_pdf)

        def modes(xs, k=4):
            from collections import Counter
            c = Counter(xs)
            return [v for v, _ in c.most_common(k)]

        print(f"{factor:>6} | {ta_pages:>6} | {pr_pages:>6} | "
              f"{str(modes(ta_d)):>40} | {str(modes(pr_d)):>40}")
    print(f"\nworkdir kept: {work}")


if __name__ == "__main__":
    main()
