#!/usr/bin/env python3
"""CORE-94 probe B: hyphenation effect + per-line break comparison.

Same controlled paragraph, now vary hyphens: none vs auto, and dump the
actual line TEXT so we can see WHERE Prince wraps earlier than TA.
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
  p {{ text-align: left; hyphens: {hyph}; margin: 0; }}
</style>
</head>
<body>
<p>{words}</p>
</body>
</html>
"""


def extract_lines(pdf: Path) -> list[str]:
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
            out.append(text)
    out.sort()
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
    work = Path(tempfile.mkdtemp(prefix="core94-b-"))
    print(f"probe workdir: {work}\n")
    for hyph in ["none", "auto"]:
        html = work / f"hyph-{hyph}.html"
        html.write_text(TMPL.format(hyph=hyph, words=WORDS))
        ta = work / f"ta-{hyph}.pdf"
        pr = work / f"pr-{hyph}.pdf"
        if not render(html, ta, False) or not render(html, pr, True):
            print(f"hyph={hyph}: render failed")
            continue
        tl = extract_lines(ta)
        pl = extract_lines(pr)
        print(f"===== hyphens: {hyph} — TA {len(tl)} lines, Pr {len(pl)} lines =====")
        print(f"{'TA':>42} | {'Prince':>42}")
        for i in range(max(len(tl), len(pl))):
            a = tl[i] if i < len(tl) else ""
            b = pl[i] if i < len(pl) else ""
            print(f"{a[:42]:>42} | {b[:42]}")
        print()
    print(f"workdir kept: {work}")


if __name__ == "__main__":
    main()
