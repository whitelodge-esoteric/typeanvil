#!/usr/bin/env python3
"""CORE-90 probe v10: bisect prose.html to find what pushes TA's h1 first
baseline from 51 (isolated) to 57 (full prose). Variants strip the file down
piece by piece; the first variant whose TA h1 baseline drops back to ~51
names the culprit.
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

GEOM = ["--page-width", "5in", "--page-height", "3in",
        "--margin-top", "0.5in", "--margin-right", "0.5in",
        "--margin-bottom", "0.5in", "--margin-left", "0.5in"]

FACTOR = 1.2


def base_variant():
    """Full prose.html, patched to line-height 1.2 (already is)."""
    return CORPUS.read_text()


def variants():
    full = base_variant()
    out = {"full": full}
    # v: no headings
    s = full
    s = re.sub(r"<h1>.*?</h1>", "", s, flags=re.S)
    s = re.sub(r"<h2>.*?</h2>", "", s, flags=re.S)
    s = re.sub(r"<blockquote>.*?</blockquote>", "", s, flags=re.S)
    s = re.sub(r'<p class="note">.*?</p>', "", s, flags=re.S)
    out["no-headings"] = s
    # w: keep h1 only, drop h2/blockquote/note
    s = full
    s = re.sub(r"<h2>.*?</h2>", "", s, flags=re.S)
    s = re.sub(r"<blockquote>.*?</blockquote>", "", s, flags=re.S)
    s = re.sub(r'<p class="note">.*?</p>', "", s, flags=re.S)
    out["h1-only-rest"] = s
    # x: keep h1 + one p, drop the rest
    s = full
    s = re.sub(r"<h2>.*?</h2>", "", s, flags=re.S)
    s = re.sub(r"<blockquote>.*?</blockquote>", "", s, flags=re.S)
    s = re.sub(r'<p class="note">.*?</p>', "", s, flags=re.S)
    # keep only the first <p>...</p> after the h1
    first_p = re.search(r"(<p>.*?</p>)", s, flags=re.S)
    s = re.sub(r"<p>.*?</p>", "", s, flags=re.S)
    if first_p:
        s = s.replace("</h1>", "</h1>" + first_p.group(1))
    out["h1-one-p"] = s
    # y: only the h1 element and its CSS, with the real @page
    s = re.search(r"(<!DOCTYPE html>.*?<style>.*?</style>)", full, flags=re.S).group(1)
    s += "\n</head>\n<body>\n<h1>Prose Showcase: what an engine owes a sentence</h1>\n</body>\n</html>\n"
    out["h1-css-full"] = s
    # z: minimal — the v9 probe with h1 margin 0 0 6pt 0 (prose's h1 rule)
    s = """<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  @page { margin: 0.5in;
    @top-center { content: "Prose Showcase — Knuth-Plass Justification"; font-family: Arial, Helvetica, sans-serif; font-size: 10pt; }
  }
  body { font-family: Arial, Helvetica, "Liberation Sans", sans-serif; font-size: 10pt; line-height: 1.2; color: #111; margin: 0; }
  h1 { font-size: 15pt; margin: 0 0 6pt 0; }
</style>
</head>
<body>
<h1>Prose Showcase: what an engine owes a sentence</h1>
</body>
</html>
"""
    out["h1-v9-style"] = s
    return out


def render(html: Path, out: Path, prince: bool) -> int:
    if prince:
        r = subprocess.run([str(PRINCE_SH), str(html), *GEOM, "-o", str(out)],
                           capture_output=True, text=True)
    else:
        r = subprocess.run([str(TA_BIN), "render", str(html), *GEOM, "-o", str(out)],
                           capture_output=True, text=True)
    if r.returncode != 0:
        print(f"  FAILED: {r.stderr[-300:]}")
    return r.returncode


def first_baseline(pdf: Path) -> float:
    import pypdfium2 as pdfium
    doc = pdfium.PdfDocument(str(pdf))
    page = doc[0]
    H = page.get_size()[1]
    tp = page.get_textpage()
    n = tp.count_chars()
    for i in range(n):
        left, bottom, right, top = tp.get_charbox(i)
        if right - left > 0.01:
            return round(H - bottom, 3)
    return float("nan")


def main():
    if not TA_BIN.exists():
        print(f"TA binary missing at {TA_BIN}; build first.")
        sys.exit(2)
    work = Path(tempfile.mkdtemp(prefix="core90-bisect-"))
    print(f"probe workdir: {work}\n")
    print(f"{'variant':>14} | {'TA h1 base':>10} | {'Pr h1 base':>10}")
    print("-" * 44)
    for name, src in variants().items():
        html = work / f"{name}.html"
        html.write_text(src)
        ta_pdf = work / f"ta-{name}.pdf"
        pr_pdf = work / f"pr-{name}.pdf"
        if render(html, ta_pdf, False) != 0 or render(html, pr_pdf, True) != 0:
            continue
        ta_b = first_baseline(ta_pdf)
        pr_b = first_baseline(pr_pdf)
        print(f"{name:>14} | {ta_b:>10} | {pr_b:>10}")
    print(f"\nworkdir kept: {work}")


if __name__ == "__main__":
    main()
