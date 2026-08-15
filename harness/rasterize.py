"""Rasterize PDF bytes into per-page PIL images at a fixed DPI.

WPT print-reftests are compared page-by-page as pixels. Chromium prints at 96 CSS
px/in; both the test and reference PDFs must therefore be rasterized at the *same* DPI
so that a 5in x 3in page becomes an identically-sized bitmap on both sides (research
brief section 2: "rasterize both sides at the same DPI").

This module uses ``pypdfium2`` (PDFium bindings) rather than ``pdftoppm``/Poppler,
which is unavailable in the target environment (task constraints).
"""

from __future__ import annotations

from pathlib import Path

from PIL import Image

# Chromium prints at 96 CSS px per inch. Rasterizing at 96 DPI makes a 5in x 3in page
# a 480 x 288 px bitmap.
DEFAULT_DPI = 96
_BASE_DPI = 72.0  # PDF user-space unit is 1/72 inch.


def rasterize_pdf(pdf: bytes | Path, *, dpi: int = DEFAULT_DPI) -> list[Image.Image]:
    """Rasterize every page of ``pdf`` to an RGB :class:`PIL.Image` at ``dpi``.

    ``pdf`` may be raw PDF bytes or a path to a PDF file.
    """
    import pypdfium2 as pdfium

    if isinstance(pdf, Path):
        data: bytes = pdf.read_bytes()
    else:
        data = pdf

    scale = dpi / _BASE_DPI
    images: list[Image.Image] = []
    doc = pdfium.PdfDocument(data)
    try:
        for page in doc:
            bitmap = page.render(scale=scale)
            try:
                img = bitmap.to_pil().convert("RGB")
            finally:
                bitmap.close()
            images.append(img)
            page.close()
    finally:
        doc.close()
    return images
