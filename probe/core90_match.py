#!/usr/bin/env python3
"""CORE-90 probe v12: cumulative baseline divergence, line-matched.

For full prose at 1.2 and 1.6: extract every text line (text + baseline from
top) per page from both engines, match lines by their text content, and print
the per-line y-delta (Pr - TA). If TA's baselines sit consistently LOWER
(positive delta at low factors) and Prince's lower at high factors, that's
the half-leading slope-cross made visible line by line.
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

PAGE_H = 216.0


def patched(factor: float) -> str:
    src = CORPUS.read_text()
    return re.sub(r"line-height:\s*[\d.]+", f"line-height: {factor}", src, count=1)


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


def extract_lines(pdf: Path) -> list[dict]:
    """All lines in the doc: {page, y_from_top, text}."""
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
            left, bottom, right, top = tp.get_charbox(i)
            if right - left < 0.01:
                continue
            txt = tp.get_text_range(i, i + 1)
            chars.append({"left": left, "bottom": bottom, "right": right,
                          "text": txt})
        chars.sort(key=lambda c: (-c["bottom"], c["left"]))
        lines = []
        for c in chars:
            placed = False
            for ln in lines:
                if abs(ln["baseline"] - c["bottom"]) < 3.0:
                    ln["chars"].append(c)
                    placed = True
                    break
            if not placed:
                lines.append({"baseline": c["bottom"], "chars": [c]})
        for ln in lines:
            cs = sorted(ln["chars"], key=lambda c: c["left"])
            text = "".join(c["text"] for c in cs).replace("\n", " ")
            text = re.sub(r"\s+", " ", text).strip()
            out.append({"page": pi, "y": round(H - ln["baseline"], 2), "text": text})
    out.sort(key=lambda d: (d["page"], d["y"]))
    return out


def norm(t: str) -> str:
    return re.sub(r"[^a-z0-9]", "", t.lower())


def main():
    if not TA_BIN.exists():
        print(f"TA binary missing at {TA_BIN}; build first.")
        sys.exit(2)
    work = Path(tempfile.mkdtemp(prefix="core90-match-"))
    print(f"probe workdir: {work}\n")
    for factor in [1.2, 1.6]:
        html = work / f"prose-{factor}.html"
        html.write_text(patched(factor))
        ta_pdf = work / f"ta-{factor}.pdf"
        pr_pdf = work / f"pr-{factor}.pdf"
        render(html, ta_pdf, False)
        render(html, pr_pdf, True)
        ta = extract_lines(ta_pdf)
        pr = extract_lines(pr_pdf)
        # index Prince lines by normalized text (first 24 chars)
        pr_by_text = {}
        for d in pr:
            k = norm(d["text"])[:24]
            pr_by_text.setdefault(k, []).append(d)
        print(f"===== factor {factor}: TA {len(ta)} lines, Pr {len(pr)} lines =====")
        deltas = []
        for t in ta:
            k = norm(t["text"])[:24]
            cands = pr_by_text.get(k, [])
            if not cands:
                continue
            best = min(cands, key=lambda c: abs(c["page"] - t["page"]))
            delta = round(best["y"] - t["y"], 2)  # Pr - TA
            deltas.append(delta)
            if abs(delta) >= 1.0 or len(deltas) < 3:
                print(f"  p{t['page']} y={t['y']:>6}  Pr-TA={delta:>6.2f}  | {t['text'][:44]}")
        if deltas:
            print(f"  >>> median Pr-TA over {len(deltas)} matched lines: "
                  f"{sorted(deltas)[len(deltas)//2]:.2f}")
        print()
    print(f"workdir kept: {work}")


if __name__ == "__main__":
    main()
