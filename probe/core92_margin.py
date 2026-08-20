#!/usr/bin/env python3
"""CORE-92 probe: measure each engine's UA body margin.

Renders a single h1 (15pt, line-height 1.2) in a page at demo geometry
(5in×3in, 0.5in margins → content top = 36pt). Varies body margin:

  A. no body rule (UA default applies)
  B. body { margin: 0 }
  C. body { margin: 8px } (= 6pt)
  D. body { margin: 3pt }
  E. body { margin: 0.25em }

Expected baseline offset (half-leading, CORE-90): 15*0.9053 + (18-15*0.9053-15*0.2119)/2 = 14.2pt
  → first baseline at 36 + 14.2 = 50.2pt (body margin 0).
Any deviation = effective UA body margin applied.

Also measures page 2 (if any) — body margin should NOT apply there.
"""

import subprocess
import tempfile
from pathlib import Path

ROOT = Path("/Users/elijah/workspace/typeanvil.worktrees/core-92-ua-body-margin")
TA_BIN = ROOT / "engine/target/debug/typeanvil"
PRINCE_SH = ROOT / "scripts/render-prince.sh"

GEOM = ["--page-width", "5in", "--page-height", "3in",
        "--margin-top", "0.5in", "--margin-right", "0.5in",
        "--margin-bottom", "0.5in", "--margin-left", "0.5in"]


def measure_first_baseline(pdf: Path) -> float:
    """Baseline of first text char on page 1, y-from-top."""
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


def render(html: Path, out: Path, prince: bool) -> bool:
    if prince:
        r = subprocess.run([str(PRINCE_SH), str(html), *GEOM, "-o", str(out)],
                           capture_output=True, text=True)
    else:
        r = subprocess.run([str(TA_BIN), "render", str(html), *GEOM, "-o", str(out)],
                           capture_output=True, text=True)
    return r.returncode == 0


# The test HTML template: h1 only, vary body margin.
HTML_TMPL = """<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  @page {{ margin: 0.5in; }}
  body {{ font-family: Arial, sans-serif; font-size: 10pt; line-height: 1.2;{body_margin} }}
  h1 {{ font-size: 15pt; line-height: 1.2; margin: 0; }}
</style>
</head>
<body>
<h1>Heading</h1>
</body>
</html>
"""

CASES = {
    "no-rule": "",
    "margin-0": "margin: 0;",
    "margin-8px": "margin: 8px;",
    "margin-3pt": "margin: 3pt;",
    "margin-0_25em": "margin: 0.25em;",
}

work = Path(tempfile.mkdtemp(prefix="core92-margin-"))
print(f"probe workdir: {work}\n")
print(f"{'case':>16} | {'TA first base':>13} | {'Pr first base':>13} | {'delta':>6}")
print("-" * 56)
for name, body_margin in CASES.items():
    html = work / f"{name}.html"
    html.write_text(HTML_TMPL.format(body_margin="\n  " + body_margin if body_margin else ""))
    ta_pdf = work / f"ta-{name}.pdf"
    pr_pdf = work / f"pr-{name}.pdf"
    if not render(html, ta_pdf, False) or not render(html, pr_pdf, True):
        continue
    ta_b = measure_first_baseline(ta_pdf)
    pr_b = measure_first_baseline(pr_pdf)
    d = round(pr_b - ta_b, 3)
    print(f"{name:>16} | {ta_b:>13} | {pr_b:>13} | {d:>6}")
print(f"\nworkdir kept: {work}")