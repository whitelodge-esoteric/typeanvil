#!/usr/bin/env python3
"""CORE-94 probe C: hyphenated breaks in the REAL prose fixture.

Counts lines ending in a hyphen per engine, and total lines, at 1.6.
Extraction via pypdfium2 textpage (get_text_range per char).
"""

import re
import subprocess
import tempfile
from pathlib import Path

ROOT = Path("/Users/elijah/workspace/typeanvil.worktrees/core-94-line-breaking")
TA_BIN = ROOT / "engine/target/debug/typeanvil"
PRINCE_SH = ROOT / "scripts/render-prince.sh"
CORPUS = Path("/Users/elijah/workspace/typeanvil/demo/corpus/prose.html")

GEOM = ["--page-width", "5in", "--page-height", "3in",
        "--margin-top", "0.5in", "--margin-right", "0.5in",
        "--margin-bottom", "0.5in", "--margin-left", "0.5in"]


def extract_lines(pdf: Path) -> list[dict]:
    import pypdfium2 as pdfium
    doc = pdfium.PdfDocument(str(pdf))
    out = []
    for pi in range(len(doc)):
        page = doc[pi]
        H = page.get_size()[1]
        tp = page.get_textpage()
        n = tp.count_chars()
        chars = []
        for i in range(n):
            l, b, r, t = tp.get_charbox(i)
            if r - l < 0.01:
                continue
            chars.append({"l": l, "b": b, "r": r, "s": tp.get_text_range(i, i + 1)})
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
        for ln in lines:
            cs = sorted(ln["chars"], key=lambda c: c["l"])
            text = "".join(c["s"] for c in cs).replace("\n", " ")
            text = re.sub(r"\s+", " ", text).strip()
            if text:
                out.append({"page": pi, "text": text})
    return out


def render(html: Path, out: Path, prince: bool) -> bool:
    if prince:
        r = subprocess.run([str(PRINCE_SH), str(html), *GEOM, "-o", str(out)],
                           capture_output=True, text=True)
    else:
        r = subprocess.run([str(TA_BIN), "render", str(html), *GEOM, "-o", str(out)],
                           capture_output=True, text=True)
    return r.returncode == 0


def main():
    work = Path(tempfile.mkdtemp(prefix="core94-c-"))
    src = CORPUS.read_text()
    # force body margin 0 to remove the UA-margin confound (already fixed in
    # CORE-92, but pin explicitly for a clean line comparison)
    src = src.replace(
        'body { font-family: Arial, Helvetica, "Liberation Sans", sans-serif; font-size: 10pt; line-height: 1.6;',
        'body { font-family: Arial, Helvetica, "Liberation Sans", sans-serif; font-size: 10pt; line-height: 1.6; margin: 0;')
    html = work / "prose.html"
    html.write_text(src)
    ta = work / "ta.pdf"
    pr = work / "pr.pdf"
    render(html, ta, False)
    render(html, pr, True)
    tl = extract_lines(ta)
    pl = extract_lines(pr)

    def hyphen_stats(lines):
        hyph = [l for l in lines if l["text"].endswith("-")]
        return len(lines), len(hyph), [l["text"][-30:] for l in hyph[:10]]

    tn, th, tex = hyphen_stats(tl)
    pn, ph, pex = hyphen_stats(pl)
    print(f"TA:  {tn} lines, {th} hyphenated line-ends")
    print(f"Pr:  {pn} lines, {ph} hyphenated line-ends")
    print("\nTA hyphenated line-ends (last 30 chars):")
    for t in tex:
        print(f"  …{t}")
    print("\nPr hyphenated line-ends (last 30 chars):")
    for t in pex:
        print(f"  …{t}")
    print(f"\nworkdir kept: {work}")


if __name__ == "__main__":
    main()
