#!/usr/bin/env python3
"""CORE-94 verification: prose.html page counts at all factors, TA vs Prince,
body margin 0 (CORE-92), after the justify fill fix."""
import re, subprocess, tempfile
from pathlib import Path
ROOT = Path("/Users/elijah/workspace/typeanvil.worktrees/core-94-line-breaking")
TA_BIN = ROOT / "engine/target/debug/typeanvil"
PRINCE_SH = ROOT / "scripts/render-prince.sh"
CORPUS = Path("/Users/elijah/workspace/typeanvil/demo/corpus/prose.html")
GEOM = ["--page-width", "5in", "--page-height", "3in",
        "--margin-top", "0.5in", "--margin-right", "0.5in",
        "--margin-bottom", "0.5in", "--margin-left", "0.5in"]
FACTORS = [1.0, 1.2, 1.5, 1.6, 2.0]
def patched(factor):
    src = CORPUS.read_text()
    src = re.sub(r"line-height:\s*[\d.]+", f"line-height: {factor}", src, count=1)
    src = src.replace('body { font-family: Arial, Helvetica, "Liberation Sans", sans-serif; font-size: 10pt;',
                      'body { font-family: Arial, Helvetica, "Liberation Sans", sans-serif; font-size: 10pt; margin: 0;')
    return src
def page_count(pdf):
    raw = pdf.read_bytes()
    m = re.search(rb"/Count\s+(\d+)", raw)
    return int(m.group(1)) if m else -1
work = Path(tempfile.mkdtemp(prefix="core94-prose-"))
print(f"{'factor':>6} | {'TA':>3} | {'Pr':>3} | diff")
for factor in FACTORS:
    html = work / f"prose-{factor}.html"
    html.write_text(patched(factor))
    ta = work / f"ta-{factor}.pdf"; pr = work / f"pr-{factor}.pdf"
    subprocess.run([str(TA_BIN), "render", str(html), *GEOM, "-o", str(ta)], capture_output=True)
    subprocess.run([str(PRINCE_SH), str(html), *GEOM, "-o", str(pr)], capture_output=True)
    tp, pp = page_count(ta), page_count(pr)
    print(f"{factor:>6} | {tp:>3} | {pp:>3} | {'=' if tp==pp else f'{tp-pp:+d}'}")
print(f"\nworkdir kept: {work}")
