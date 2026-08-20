#!/usr/bin/env python3
"""CORE-90 probe v5: isolate the prose.html divergence.

v2-v4 proved line boxes match exactly (all font sizes x factors). The page
counts still cross at 1.2 (TA 9 > Pr 8) and 1.6 (TA 10 < Pr 11). The
divergence must come from: (a) line breaking (justify+hyphens line counts),
(b) block margins, or (c) mixed blocks (h1/h2/blockquote/note).

This probe renders prose.html variants:
  A. full prose (baseline reproduction)
  B. prose minus headings (h1/h2) and blockquote and note -> pure <p> runs
  C. prose paragraphs only, with margins zeroed
  D. paragraphs with hyphens:none and text-align:left (no justification)
and reports TA/Prince page counts per factor. Whichever variant restores
agreement localizes the divergence.
"""

import re
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


def variant(factor: float, kind: str) -> str:
    src = CORPUS.read_text()
    src = re.sub(r"line-height:\s*[\d.]+", f"line-height: {factor}", src, count=1)
    if kind == "no-headings":
        # Remove h1/h2/blockquote/.note elements entirely.
        src = re.sub(r"<h1>.*?</h1>", "", src, flags=re.S)
        src = re.sub(r"<h2>.*?</h2>", "", src, flags=re.S)
        src = re.sub(r"<blockquote>.*?</blockquote>", "", src, flags=re.S)
        src = re.sub(r'<p class="note">.*?</p>', "", src, flags=re.S)
    elif kind == "no-margins":
        src = re.sub(r"<h1>.*?</h1>", "", src, flags=re.S)
        src = re.sub(r"<h2>.*?</h2>", "", src, flags=re.S)
        src = re.sub(r"<blockquote>.*?</blockquote>", "", src, flags=re.S)
        src = re.sub(r'<p class="note">.*?</p>', "", src, flags=re.S)
        src = src.replace("margin: 0 0 7pt 0", "margin: 0")
    elif kind == "no-justify-hyph":
        src = re.sub(r"<h1>.*?</h1>", "", src, flags=re.S)
        src = re.sub(r"<h2>.*?</h2>", "", src, flags=re.S)
        src = re.sub(r"<blockquote>.*?</blockquote>", "", src, flags=re.S)
        src = re.sub(r'<p class="note">.*?</p>', "", src, flags=re.S)
        src = src.replace("text-align: justify; hyphens: auto", "text-align: left; hyphens: none")
    return src


def page_count(pdf: Path) -> int:
    raw = pdf.read_bytes()
    m = re.search(rb"/Count\s+(\d+)", raw)
    return int(m.group(1)) if m else -1


def render(html: Path, out: Path, prince: bool) -> int:
    if prince:
        r = subprocess.run([str(PRINCE_SH), str(html), *GEOM, "-o", str(out)],
                           capture_output=True, text=True)
    else:
        r = subprocess.run([str(TA_BIN), "render", str(html), *GEOM, "-o", str(out)],
                           capture_output=True, text=True)
    if r.returncode != 0:
        print(f"  FAILED: {r.stderr[-500:]}")
    return r.returncode


def main():
    if not TA_BIN.exists():
        print(f"TA binary missing at {TA_BIN}; build first.")
        sys.exit(2)
    work = Path(tempfile.mkdtemp(prefix="core90-variant-"))
    print(f"probe workdir: {work}\n")
    for kind in ["full", "no-headings", "no-margins", "no-justify-hyph"]:
        print(f"=== variant: {kind} ===")
        print(f"{'factor':>6} | {'TA':>3} | {'Pr':>3} | diff")
        for factor in FACTORS:
            html = work / f"{kind}-{factor}.html"
            html.write_text(variant(factor, kind))
            ta_pdf = work / f"ta-{kind}-{factor}.pdf"
            pr_pdf = work / f"pr-{kind}-{factor}.pdf"
            if render(html, ta_pdf, False) != 0 or render(html, pr_pdf, True) != 0:
                continue
            tp, pp = page_count(ta_pdf), page_count(pr_pdf)
            print(f"{factor:>6} | {tp:>3} | {pp:>3} | {'TA+1' if tp>pp else 'Pr+1' if pp>tp else '='}")
        print()
    print(f"workdir kept: {work}")


if __name__ == "__main__":
    main()
