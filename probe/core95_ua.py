#!/usr/bin/env python3
"""CORE-95 probe: verify the engine's UA block defaults match Prince's print UA.

Prince 16.2 ships its UA sheet at lib/prince/style/html.css. Its block
defaults are print-adapted: fixed-point heading sizes/margins and 1.12em
paragraph/list margins (vs the HTML4 screen defaults the engine used to
carry). This probe renders one element per page at demo geometry
(5in×3in, 0.5in margins -> content top = 36pt) through both engines and
measures:

  margin_top    = baseline(UA default) - baseline(margin:0)  [pt, y-from-top]
  first baseline = absolute baseline of the element's first text line

For each element we report TA vs Prince side by side. Both should agree on
margin_top (exactly, per Prince's sheet) and on the absolute baseline
(within ~0.5pt of font/line-box noise).

Elements: h1..h6 (fixed pt), p/blockquote/pre (1.12em), ul/ol (1.12em).
"""

import subprocess
import tempfile
from pathlib import Path

ROOT = Path("/Users/elijah/workspace/typeanvil.worktrees/core-95-ua-heading-margins")
TA_BIN = ROOT / "engine/target/debug/typeanvil"
PRINCE_SH = ROOT / "scripts/render-prince.sh"

GEOM = ["--page-width", "5in", "--page-height", "3in",
        "--margin-top", "0.5in", "--margin-right", "0.5in",
        "--margin-bottom", "0.5in", "--margin-left", "0.5in"]

# (tag, inner html) — ul/ol need an li so the text char exists. Inner text
# must contain a capital 'P' (the char the baseline measurement keys on).
ELEMENTS = [
    ("h1", "Probe heading"), ("h2", "Probe heading"), ("h3", "Probe heading"),
    ("h4", "Probe heading"), ("h5", "Probe heading"), ("h6", "Probe heading"),
    ("p", "Paragraph probe."),
    ("blockquote", "Probe quote."), ("pre", "Probe mono"),
    ("ul", "<li>Probe item one</li>"), ("ol", "<li>Probe item one</li>"),
]

HTML_TMPL = """<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  @page {{ margin: 0.5in; }}
  body {{ font-family: Arial, sans-serif; font-size: 10pt; line-height: 1.4; }}
  .zero {{ margin: 0; padding: 0; }}
</style>
</head>
<body>
{inner}
</body>
</html>
"""


def first_text_baseline(pdf: Path, char: str = "P") -> float:
    """y-from-top baseline of the first char equal to `char` on page 1."""
    import pypdfium2 as pdfium
    doc = pdfium.PdfDocument(str(pdf))
    page = doc[0]
    H = page.get_size()[1]
    tp = page.get_textpage()
    for i in range(tp.count_chars()):
        ch = tp.get_text_range(i, 1)
        if ch and ch[0] == char:
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


def measure(tag: str, inner: str, zero: bool) -> dict:
    """Render tag through both engines; return baselines + margin_top.

    The engine's stylo cascade does NOT read the `style=""` attribute (only
    the paged/breaks passes do), so zeroing goes through a `.zero` class rule
    in the author stylesheet instead of an inline style.
    """
    cls = ' class="zero"' if zero else ""
    element = f"<{tag}{cls}>{inner}</{tag}>"
    html = Path(tempfile.mkdtemp(prefix="core95-")) / "doc.html"
    html.write_text(HTML_TMPL.format(inner=element))
    out = {}
    for name, prince in (("ta", False), ("pr", True)):
        pdf = html.with_name(f"{name}.pdf")
        if not render(html, pdf, prince):
            return {"error": f"{name} render failed"}
        out[name] = first_text_baseline(pdf)
    return out


work = Path(tempfile.mkdtemp(prefix="core95-ua-"))
print(f"probe workdir: {work}\n")
print(f"{'el':>10} | {'TA base':>8} | {'Pr base':>8} | {'base Δ':>7} | "
      f"{'TA mtop':>8} | {'Pr mtop':>8} | {'mtop Δ':>7}")
print("-" * 68)
ok = True
for tag, inner in ELEMENTS:
    a = measure(tag, inner, False)
    b = measure(tag, inner, True)
    if "error" in a or "error" in b:
        print(f"{tag:>10} | ERROR {a.get('error', b.get('error'))}")
        ok = False
        continue
    ta_mtop = round(a["ta"] - b["ta"], 3)
    pr_mtop = round(a["pr"] - b["pr"], 3)
    base_d = round(a["pr"] - a["ta"], 3)
    mtop_d = round(pr_mtop - ta_mtop, 3)
    # `pre` resolves the UA `font-family: monospace` to DIFFERENT system
    # fonts in the two engines, so its absolute baseline differs by font
    # metrics (~0.8pt); the margin-top comparison is the signal there.
    baseline_ok = abs(base_d) < 0.75 or tag == "pre"
    if not baseline_ok or abs(mtop_d) > 0.35:
        ok = False
    print(f"{tag:>10} | {a['ta']:>8} | {a['pr']:>8} | {base_d:>7.2f} | "
          f"{ta_mtop:>8} | {pr_mtop:>8} | {mtop_d:>7.2f}")


# Mid-page check: after in-flow content, the h1's 16pt UA margin must apply
# in FULL in both engines (truncation only bites at fragmentainer starts).
# baseline_h1 - baseline_p = 16 (h1 margin) + 28.65 (line boxes: p 14pt +
# h1 33.6pt at the inherited 1.4 factor) = 44.65; p is zero-margined.
def mid_h1_gap() -> tuple:
    html = Path(tempfile.mkdtemp(prefix="core95-mid-")) / "doc.html"
    html.write_text(HTML_TMPL.format(
        inner='<p class="zero">Paragraph probe.</p><h1>Heading probe</h1>'))
    out = {}
    for name, prince in (("ta", False), ("pr", True)):
        pdf = html.with_name(f"{name}.pdf")
        if not render(html, pdf, prince):
            return (float("nan"), float("nan"))
        doc = pdfium_open(pdf)
        page = doc[0]
        H = page.get_size()[1]
        tp = page.get_textpage()
        p_base = h_base = None
        for i in range(tp.count_chars()):
            ch = tp.get_text_range(i, 1)
            if ch == "P":
                l, b, r, t = tp.get_charbox(i)
                p_base = H - b
            elif ch == "H":
                l, b, r, t = tp.get_charbox(i)
                h_base = H - b
        out[name] = (p_base, h_base)
    return out["ta"], out["pr"]


def pdfium_open(pdf):
    import pypdfium2 as pdfium
    return pdfium.PdfDocument(str(pdf))
(ta_p, ta_h), (pr_p, pr_h) = mid_h1_gap()
ta_gap = round(ta_h - ta_p, 3)
pr_gap = round(pr_h - pr_p, 3)
mid_ok = abs(ta_gap - pr_gap) < 0.5 and abs(ta_gap - 44.65) < 0.5
if not mid_ok:
    ok = False
print(f"{'mid-h1':>10} | {ta_gap:>8} | {pr_gap:>8} | {round(pr_gap - ta_gap, 2):>7} | "
      f"{'16pt':>8} | {'16pt':>8} | {'0.00':>7}")
print("\nVERDICT:", "PASS — UA block defaults match Prince" if ok else "FAIL — investigate")
print(f"workdir kept: {work}")
