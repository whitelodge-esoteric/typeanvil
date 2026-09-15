#!/usr/bin/env python3
"""Check render determinism, which the capture fingerprints depend on (CORE-206).

    scripts/dev-container.sh bash -c 'python3 /work/probe/core206/determinism.py'

A capture fingerprints each rendered page. If the engine is not deterministic,
every gate run would report spurious changes and the gate would be useless. This
probe renders the same inputs repeatedly and compares PDF bytes and page
fingerprints.
"""

from __future__ import annotations

import hashlib
import subprocess
import tempfile
from pathlib import Path

import pypdfium2 as pdfium

CLI = "/work/engine/target/debug/typeanvil"
DPI = 96
BASE_DPI = 72.0
REPEATS = 3

INPUTS = [
    ("fixture abspos", Path("/work/harness/direct_fixtures/abspos-margins.html")),
    ("fixture report-links", Path("/work/harness/direct_fixtures/report-links.html")),
    ("invoice corpus", Path("/work/demo/corpus/invoice.html")),
    (
        "wpt orthogonal-003",
        Path("/main/.wpt/css/css-page/page-name-orthogonal-writing-003-print.html"),
    ),
]


def render(html: Path, out: Path) -> None:
    subprocess.run(
        [
            CLI, "render", str(html),
            "--page-width", "5in", "--page-height", "3in",
            "--margin-top", "0.5in", "--margin-right", "0.5in",
            "--margin-bottom", "0.5in", "--margin-left", "0.5in",
            "-o", str(out),
        ],
        check=True, capture_output=True, timeout=120,
    )


def fingerprints(pdf_bytes: bytes) -> list[str]:
    doc = pdfium.PdfDocument(pdf_bytes)
    out: list[str] = []
    try:
        for page in doc:
            bitmap = page.render(scale=DPI / BASE_DPI)
            try:
                img = bitmap.to_pil().convert("RGB")
            finally:
                bitmap.close()
            h = hashlib.sha256()
            h.update(img.tobytes())
            h.update(f"{img.width}x{img.height}x{img.mode}".encode())
            out.append(h.hexdigest()[:16])
    finally:
        doc.close()
    return out


def main() -> int:
    worst = 0
    for label, html in INPUTS:
        if not html.exists():
            print(f"{label}: MISSING {html}")
            continue
        bytes_hashes: list[str] = []
        fp_sets: list[list[str]] = []
        for _ in range(REPEATS):
            with tempfile.TemporaryDirectory() as td:
                pdf = Path(td) / "out.pdf"
                render(html, pdf)
                data = pdf.read_bytes()
                bytes_hashes.append(hashlib.sha256(data).hexdigest()[:16])
                fp_sets.append(fingerprints(data))
        bytes_stable = len(set(bytes_hashes)) == 1
        fps_stable = all(s == fp_sets[0] for s in fp_sets)
        print(
            f"{label}: pages={len(fp_sets[0])} pdf_bytes_stable={bytes_stable}"
            f" fingerprints_stable={fps_stable}"
        )
        print(f"    pdf sha256[:16] = {sorted(set(bytes_hashes))}")
        print(f"    page fingerprints = {fp_sets[0]}")
        if not (bytes_stable and fps_stable):
            worst = 1
    print()
    print("DETERMINISTIC: no" if worst else "DETERMINISTIC: yes")
    return worst


if __name__ == "__main__":
    raise SystemExit(main())
