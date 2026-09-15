"""Minimal hand-written PDF builder for direct-check tests.

Builds valid PDFs with a standard ``/Helvetica`` font. Content coordinates use
the harness direct-check convention -- points, top-left origin, x right, y down
-- and are converted to PDF's bottom-left user space internally. The builder is
the test fixture; the assertions in ``test_direct.py`` run against real PDFs
through pypdfium2.
"""

from __future__ import annotations


def _esc(s: str) -> str:
    return s.replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)")


def _fmt(v: float) -> str:
    v = float(v)
    return str(int(v)) if v == int(v) else f"{v}"


class Page:
    """One PDF page: width/height plus content fragments and link annotations."""

    def __init__(self, width: float = 360.0, height: float = 216.0):
        self.width = float(width)
        self.height = float(height)
        self.fragments: list[str] = []
        self.links: list[dict] = []

    def text(self, x: float, y: float, s: str, size: float = 12.0) -> None:
        """Horizontal text with baseline at top-left ``(x, y)``."""
        pdf_y = self.height - y
        self.fragments.append(
            f"BT\n/F1 {size} Tf\n{x} {pdf_y} Td\n({_esc(s)}) Tj\nET"
        )

    def rect(self, x: float, y: float, w: float, h: float) -> None:
        """Filled black rectangle, top-left corner at ``(x, y)``."""
        pdf_top = self.height - y
        pdf_bottom = self.height - (y + h)
        self.fragments.append(f"0 0 0 rg\n{x} {pdf_bottom} {w} {h} re f")

    def vertical_text(
        self, x: float, y: float, s: str, size: float = 12.0, line: float = 14.0
    ) -> None:
        """Vertical run: characters stacked downward from top-left ``(x, y)``."""
        parts = [f"BT\n/F1 {size} Tf\n{x} {self.height - y} Td\n({_esc(s[0])}) Tj"]
        for ch in s[1:]:
            parts.append(f"0 -{line} Td\n({_esc(ch)}) Tj")
        parts.append("ET")
        self.fragments.append("\n".join(parts))

    def link(self, uri: str, rect: tuple[float, float, float, float]) -> None:
        """Add a ``/Link`` annotation with a URI, rect in top-left coordinates."""
        self.links.append({"uri": uri, "rect": list(rect)})


def _link_obj(link: dict, page_height: float) -> bytes:
    x0, y0, x1, y1 = link["rect"]
    pdf_bottom = page_height - y1
    pdf_top = page_height - y0
    return (
        b"<< /Type /Annot /Subtype /Link /Rect ["
        + _fmt(x0).encode()
        + b" "
        + _fmt(pdf_bottom).encode()
        + b" "
        + _fmt(x1).encode()
        + b" "
        + _fmt(pdf_top).encode()
        + b"] /Border [0 0 0] /A << /S /URI /URI ("
        + _esc(link["uri"]).encode("latin-1")
        + b") >> >>"
    )


def build_pdf(pages) -> bytes:
    """Assemble one :class:`Page` (or a list of them) into PDF bytes."""
    if isinstance(pages, Page):
        pages = [pages]

    # Object numbering: 1 catalog, 2 pages tree, 3 shared font, then per page
    # (content object, link objects, page object).
    obj_no = 3
    layout = []  # (content_no, link_nos, page_no)
    page_numbers = []
    for p in pages:
        content_no = obj_no
        obj_no += 1
        link_nos = []
        for _ in p.links:
            link_nos.append(obj_no)
            obj_no += 1
        page_no = obj_no
        obj_no += 1
        layout.append((content_no, link_nos, page_no))
        page_numbers.append(page_no)

    kids = b"[" + b" ".join(f"{n} 0 R".encode() for n in page_numbers) + b"]"
    objects = {
        1: b"<< /Type /Catalog /Pages 2 0 R >>",
        2: b"<< /Type /Pages /Kids " + kids + b" /Count " + str(len(pages)).encode() + b" >>",
        3: b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    }

    for page, (content_no, link_nos, page_no) in zip(pages, layout):
        content = "\n".join(page.fragments).encode("latin-1")
        objects[content_no] = (
            b"<< /Length "
            + str(len(content)).encode()
            + b" >>\nstream\n"
            + content
            + b"\nendstream"
        )
        for link, link_no in zip(page.links, link_nos):
            objects[link_no] = _link_obj(link, page.height)
        annot = b" ".join(f"{n} 0 R".encode() for n in link_nos)
        annot_part = b" /Annots [" + annot + b"]" if link_nos else b""
        objects[page_no] = (
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 "
            + _fmt(page.width).encode()
            + b" "
            + _fmt(page.height).encode()
            + b"] /Resources << /Font << /F1 3 0 R >> >> /Contents "
            + str(content_no).encode()
            + b" 0 R"
            + annot_part
            + b" >>"
        )

    out = bytearray(b"%PDF-1.4\n")
    offsets: dict[int, int] = {}
    for num in range(1, max(objects) + 1):
        if num not in objects:
            continue
        offsets[num] = len(out)
        out += f"{num} 0 obj\n".encode() + objects[num] + b"\nendobj\n"

    xref_pos = len(out)
    max_num = max(offsets)
    out += f"xref\n0 {max_num + 1}\n".encode()
    out += b"0000000000 65535 f \n"
    for num in range(1, max_num + 1):
        if num in offsets:
            out += f"{offsets[num]:010d} 00000 n \n".encode()
        else:
            out += b"0000000000 65535 f \n"
    out += (
        f"trailer\n<< /Size {max_num + 1} /Root 1 0 R >>\nstartxref\n{xref_pos}\n%%EOF\n".encode()
    )
    return bytes(out)
