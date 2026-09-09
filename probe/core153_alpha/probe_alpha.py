"""Raster probe for CSS color alpha preservation (CORE-153).

Renders page-box-002-shaped fixtures through the engine CLI and asserts
COMPOSITED pixel colors (what the color math predicts), not a match against
a second PDF:

* opaque blue @page + #f008 body  -> violet-ish (blue x red at ~53% alpha)
* opaque red  @page + #f008 blue  -> blue-violet over red
* opaque background + alpha 0 body -> page color unchanged
* opaque background + nested semitransparent box -> box color composited

Predicted composite: 8-bit round((fg*a + bg*(1-a))), a = 0x88/255.

Usage (repo root):
  /Users/elijah/workspace/typeanvil/.venv/bin/python \
      probe/core153_alpha/probe_alpha.py <engine-binary>

Exit code: 0 all cases OK, 1 any failure. Temp HTML/PDF artifacts live in a
TemporaryDirectory; the probe directory itself stays fixture-free.
"""

import subprocess
import sys
import tempfile
from pathlib import Path

import pypdfium2 as pdfium

ENGINE = sys.argv[1]
CASES = [
    # (name, css body line, page bg, body bg, expected composited RGB)
    (
        "violet_red_over_blue_page",
        "body { background: #f008; }",
        (0, 0, 255), (255, 0, 0), 0x88,
    ),
    (
        "blue_over_red_page",
        "body { background: rgba(0, 0, 255, 0.5); }",
        (255, 0, 0), (0, 0, 255), 128,
    ),
    (
        "alpha_zero_paints_nothing",
        "body { background: rgba(255, 140, 0, 0); }",
        (0, 128, 0), (255, 140, 0), 0,
    ),
]


def composite(fg, bg, a):
    af = a / 255.0
    return tuple(round(f * af + b * (1 - af)) for f, b in zip(fg, bg))


def render(html: str, out: Path):
    src = out.with_suffix(".html")
    src.write_text(html)
    subprocess.run(
        [ENGINE, "render", str(src),
         "--page-width", "5in", "--page-height", "3in",
         "--margin-top", "0in", "--margin-right", "0in",
         "--margin-bottom", "0in", "--margin-left", "0in",
         "-o", str(out)],
        check=True, capture_output=True,
    )
    return out.read_bytes()


def center_pixel(pdf: bytes):
    doc = pdfium.PdfDocument(pdf)
    try:
        page = doc[0]
        img = page.render(scale=2).to_pil().convert("RGB")
        w, h = img.size
        return img.load()[w // 2, h // 2]
    finally:
        doc.close()


def main():
    failures = 0
    with tempfile.TemporaryDirectory() as td:
        td = Path(td)
        for name, body_css, page_bg, body_bg, alpha in CASES:
            html = (
                "<html><head><style>"
                "@page { margin: 0; background: "
                f"rgb({page_bg[0]}, {page_bg[1]}, {page_bg[2]}); "
                "}} "
                + body_css +
                " </style></head><body></body></html>"
            ).replace("}}", "}")
            want = composite(body_bg, page_bg, alpha)
            got = center_pixel(render(html, td / f"{name}.pdf"))
            ok = all(abs(g - w) <= 1 for g, w in zip(got, want))
            print(f"{name}: want={want} got={got} {'OK' if ok else 'FAIL'}")
            if not ok:
                failures += 1
        # Nested overlap: opaque page, semitransparent box inside a
        # semitransparent box. Inner composite: yellow over the ALREADY
        # composited outer fill.
        html = (
            "<html><head><style>"
            "@page { margin: 0; background: white; }"
            "body { margin: 0; }"
            ".outer { width: 400px; height: 200px; background: rgba(0, 0, 255, 0.5); }"
            ".inner { width: 200px; height: 100px; background: rgba(255, 255, 0, 0.5); }"
            "</style></head><body>"
            '<div class="outer"><div class="inner"></div></div>'
            "</body></html>"
        )
        pdf = render(html, td / "nested.pdf")
        doc = pdfium.PdfDocument(pdf)
        try:
            img = doc[0].render(scale=2).to_pil().convert("RGB")
            w, h = img.size
            px = img.load()
        finally:
            doc.close()
        # outer spans x 0..400 of 960 (scale 2, 480pt page), inner 0..200;
        # sample inside both, inside outer only.
        outer_only = px[300, 100]
        inner = px[100, 50]
        over_blue = composite((255, 255, 0), (0, 0, 255), 128)
        want_inner = composite((255, 255, 0), composite((0, 0, 255), (255, 255, 255), 128), 128)
        ok1 = all(abs(g - w) <= 1 for g, w in zip(outer_only, composite((0, 0, 255), (255, 255, 255), 128)))
        ok2 = all(abs(g - w) <= 1 for g, w in zip(inner, want_inner))
        print(f"nested outer: want={composite((0, 0, 255), (255, 255, 255), 128)} got={outer_only} {'OK' if ok1 else 'FAIL'}")
        print(f"nested inner: want={want_inner} got={inner} {'OK' if ok2 else 'FAIL'}")
        failures += (not ok1) + (not ok2)
    print("PASS" if failures == 0 else f"{failures} FAILURES")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
