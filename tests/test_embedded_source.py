"""CORE-577 AC-1: pypdfium2 reads the embedded source back byte-identical.

Spec: docs/specifications/pdf-embedded-source.spec.md. The test needs a
host-runnable engine binary: set ``TYPEANVIL_BIN``, or it uses
``engine/target/debug/typeanvil`` when that file runs on this host. It skips
otherwise (the container build is a Linux ELF on macOS hosts). The
committed fixture PDF covers the read-back path everywhere.
"""

from __future__ import annotations

import os
import subprocess
from pathlib import Path

import pytest

pdfium = pytest.importorskip("pypdfium2")

ROOT = Path(__file__).resolve().parent.parent
FIXTURES = ROOT / "tests" / "fixtures" / "embedded-source"
SOURCE = FIXTURES / "doc.md"


def _attachments(pdf: Path) -> dict[str, bytes]:
    doc = pdfium.PdfDocument(str(pdf))
    try:
        out = {}
        for i in range(doc.count_attachments()):
            att = doc.get_attachment(i)
            out[att.get_name()] = bytes(att.get_data())
        return out
    finally:
        doc.close()


def test_fixture_pdf_carries_byte_identical_source():
    """The committed engine output holds the exact source bytes."""
    got = _attachments(FIXTURES / "doc.pdf")
    assert got == {"doc.md": SOURCE.read_bytes()}


def _binary() -> str | None:
    env = os.environ.get("TYPEANVIL_BIN")
    if env:
        return env
    cand = ROOT / "engine" / "target" / "debug" / "typeanvil"
    if not cand.exists():
        return None
    try:
        subprocess.run([str(cand), "--version"], capture_output=True, check=True)
    except (OSError, subprocess.CalledProcessError):
        return None
    return str(cand)


def test_live_render_round_trip(tmp_path):
    binary = _binary()
    if binary is None:
        pytest.skip("no host-runnable typeanvil binary")
    out = tmp_path / "o.pdf"
    subprocess.run(
        [binary, "render", str(SOURCE), "--page-width", "5in",
         "--page-height", "3in", "--embed-source", "-o", str(out)],
        check=True,
    )
    assert _attachments(out) == {"doc.md": SOURCE.read_bytes()}
