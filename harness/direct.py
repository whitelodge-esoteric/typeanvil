"""Direct assertions on emitted PDFs, independent of test-versus-reference comparison.

The release gate's third evidence layer reads one reviewed manifest of checks and
asserts properties of the emitted PDF directly: page geometry, extracted text,
paint regions, text orientation, and link destinations. These checks do not
depend on two documents being wrong in the same way.

Coordinate convention for ``rect_pt`` (and every reported detail): **points,
top-left origin, x right, y down**, matching the rasterized image. PDF user space
is bottom-left origin; this module converts glyph and annotation boxes to the
top-left convention before comparing.

pypdfium2 is imported lazily so ``fetch``/``score``/``history`` stay importable
without it installed.
"""

from __future__ import annotations

import ctypes
import json
from dataclasses import dataclass
from pathlib import Path

from .rasterize import DEFAULT_DPI

DIRECT_SCHEMA = "typeanvil.harness.direct/1"

_BASE_DPI = 72.0

#: The one check key each expectation must carry.
_CHECK_KEYS = (
    "page_count",
    "page_size_pt",
    "text_present",
    "text_absent",
    "paint_region",
    "text_orientation",
    "link_target",
)

_FPDF_ANNOT_LINK = 2
_FPDF_ACTION_URI = 3


class DirectError(Exception):
    """A direct-check manifest or check could not be loaded or validated."""


@dataclass(frozen=True)
class DirectCheck:
    id: str
    input: str
    input_source: str  # "wpt" | "corpus" | "fixture"
    expectation: dict  # exactly one check key
    provenance: str


@dataclass(frozen=True)
class CheckResult:
    check_id: str
    input: str
    passed: bool
    detail: str


# ---------------------------------------------------------------------------
# Manifest loading
# ---------------------------------------------------------------------------


def load_manifest(path: Path) -> list[DirectCheck]:
    """Load and validate the reviewed direct-check manifest.

    Raises :class:`DirectError` for a missing file, an unsupported schema, an
    empty ``checks`` list, a duplicate id, or empty provenance.
    """
    path = Path(path)
    if not path.exists():
        raise DirectError(f"direct manifest not found: {path}")
    try:
        data = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as exc:
        raise DirectError(f"cannot read direct manifest {path}: {exc}") from exc

    if not isinstance(data, dict):
        raise DirectError(f"malformed direct manifest {path}: not a JSON object")
    if data.get("schema") != DIRECT_SCHEMA:
        raise DirectError(
            f"unsupported direct manifest schema {data.get('schema')!r} (expected {DIRECT_SCHEMA!r})"
        )

    checks = data.get("checks")
    if not isinstance(checks, list) or not checks:
        raise DirectError(f"direct manifest {path}: empty checks list")

    out: list[DirectCheck] = []
    seen: set[str] = set()
    for entry in checks:
        cid = entry.get("id")
        provenance = entry.get("provenance")
        if not isinstance(cid, str) or not cid:
            raise DirectError(f"direct manifest {path}: check missing id")
        if cid in seen:
            raise DirectError(f"direct manifest {path}: duplicate check id {cid!r}")
        seen.add(cid)
        if not provenance or not str(provenance).strip():
            raise DirectError(f"direct manifest {path}: check {cid} has empty provenance")
        out.append(
            DirectCheck(
                id=cid,
                input=entry["input"],
                input_source=entry["input_source"],
                expectation=dict(entry["expectation"]),
                provenance=str(provenance),
            )
        )
    return out


# ---------------------------------------------------------------------------
# Input resolution
# ---------------------------------------------------------------------------


def _resolve_input(check: DirectCheck, wpt_root, corpus_root, fixture_root) -> Path:
    if check.input_source == "wpt":
        return Path(wpt_root) / check.input
    if check.input_source == "corpus":
        return Path(corpus_root) / check.input
    if check.input_source == "fixture":
        return Path(fixture_root) / check.input
    raise DirectError(
        f"check {check.id}: unknown input_source {check.input_source!r}"
    )


# ---------------------------------------------------------------------------
# Check dispatch
# ---------------------------------------------------------------------------


def _detail(check: DirectCheck, key: str, expected, actual) -> str:
    return (
        f"{check.id}: input={check.input} {key} "
        f"expected={expected!r} actual={actual!r}"
    )


def check_pdf(pdf: bytes, check: DirectCheck, *, dpi: int = DEFAULT_DPI) -> CheckResult:
    """Run a single direct check against ``pdf``; never raises for check results.

    Raises :class:`DirectError` when the expectation carries zero or more than one
    recognised check key.
    """
    keys = [k for k in _CHECK_KEYS if k in check.expectation]
    if not keys:
        raise DirectError(
            f"check {check.id}: expectation {sorted(check.expectation)} carries no recognised check key"
        )
    if len(keys) > 1:
        raise DirectError(
            f"check {check.id}: expectation carries multiple check keys {keys}"
        )
    key = keys[0]
    value = check.expectation[key]

    import pypdfium2 as pdfium

    doc = pdfium.PdfDocument(pdf)
    try:
        if key == "page_count":
            return _check_page_count(check, len(doc), value)
        if key == "page_size_pt":
            return _check_page_size(check, doc, value, check.expectation.get("tolerance_pt", 0.5))
        if key == "text_present":
            return _check_text_present(check, doc, value)
        if key == "text_absent":
            return _check_text_absent(check, doc, value)
        if key == "paint_region":
            return _check_paint_region(check, doc, value, dpi)
        if key == "text_orientation":
            return _check_text_orientation(check, doc, value)
        if key == "link_target":
            return _check_link_target(check, doc, value)
        raise DirectError(f"check {check.id}: unrecognised check key {key!r}")
    finally:
        doc.close()


# ---------------------------------------------------------------------------
# Individual checks
# ---------------------------------------------------------------------------


def _check_page_count(check: DirectCheck, actual: int, value) -> CheckResult:
    if isinstance(value, int):
        lo = hi = value
    else:
        lo, hi = value[0], value[1]
    passed = lo <= actual <= hi
    return CheckResult(
        check.id, check.input, passed, _detail(check, "page_count", value, actual)
    )


def _check_page_size(check: DirectCheck, doc, value, tolerance: float) -> CheckResult:
    expected_w, expected_h = float(value[0]), float(value[1])
    sizes = [page.get_size() for page in doc]
    mismatched = [
        (i, [w, h])
        for i, (w, h) in enumerate(sizes)
        if abs(w - expected_w) > tolerance or abs(h - expected_h) > tolerance
    ]
    passed = not mismatched
    actual = [[round(w, 3), round(h, 3)] for w, h in sizes]
    detail = (
        f"{check.id}: input={check.input} page_size_pt "
        f"expected={value!r} (tolerance={tolerance}) actual={actual!r}"
    )
    if mismatched:
        detail += f" mismatched={mismatched!r}"
    return CheckResult(check.id, check.input, passed, detail)


def _extract_text(doc) -> str:
    parts = []
    for page in doc:
        textpage = page.get_textpage()
        try:
            parts.append(textpage.get_text_bounded())
        finally:
            textpage.close()
    return "\n".join(parts)


def _check_text_present(check: DirectCheck, doc, value: list) -> CheckResult:
    full = _extract_text(doc)
    missing = [s for s in value if s not in full]
    return CheckResult(
        check.id,
        check.input,
        not missing,
        _detail(check, "text_present", value, missing),
    )


def _check_text_absent(check: DirectCheck, doc, value: list) -> CheckResult:
    full = _extract_text(doc)
    found = [s for s in value if s in full]
    return CheckResult(
        check.id,
        check.input,
        not found,
        _detail(check, "text_absent", value, found),
    )


def _relative_luminance_fraction(img, rect_pt, scale: float) -> float:
    """Fraction of pixels in ``rect_pt`` (top-left origin) whose luminance < 0.5."""
    x0, y0, x1, y1 = (float(v) for v in rect_pt)
    px0 = max(0, min(int(round(x0 * scale)), img.width))
    py0 = max(0, min(int(round(y0 * scale)), img.height))
    px1 = max(px0, min(int(round(x1 * scale)), img.width))
    py1 = max(py0, min(int(round(y1 * scale)), img.height))
    crop = img.crop((px0, py0, px1, py1))
    if crop.width == 0 or crop.height == 0:
        return 0.0
    # Histogram bins are exact and avoid the deprecated Image.getdata(): with L
    # mode, bins 0-127 are luminance below 0.5 of full scale.
    dark = sum(crop.convert("L").histogram()[:128])
    return dark / (crop.width * crop.height)


def _check_paint_region(check: DirectCheck, doc, value: dict, dpi: int) -> CheckResult:
    import pypdfium2 as pdfium

    page_idx = int(value["page"])
    rect = value["rect_pt"]
    min_frac = value.get("min_dark_fraction")
    max_frac = value.get("max_dark_fraction")

    page = doc[page_idx]
    bitmap = page.render(scale=dpi / _BASE_DPI)
    try:
        img = bitmap.to_pil().convert("RGB")
    finally:
        bitmap.close()
    page.close()

    frac = _relative_luminance_fraction(img, rect, dpi / _BASE_DPI)
    passed = True
    if min_frac is not None and frac < min_frac:
        passed = False
    if max_frac is not None and frac > max_frac:
        passed = False
    detail = (
        f"{check.id}: input={check.input} paint_region page={page_idx} "
        f"rect={list(rect)!r} dark_fraction={frac:.4f} "
        f"expected min={min_frac} max={max_frac}"
    )
    return CheckResult(check.id, check.input, passed, detail)


def _glyph_boxes(page, page_height: float) -> list[tuple[float, float, float, float]]:
    """Glyph boxes in top-left-origin (x0, y0, x1, y1), x right, y down."""
    textpage = page.get_textpage()
    boxes = []
    try:
        for i in range(textpage.count_chars()):
            left, bottom, right, top = textpage.get_charbox(i)
            # top-left origin: flip y about the page height.
            boxes.append((left, page_height - top, right, page_height - bottom))
    finally:
        textpage.close()
    return boxes


def _intersects(box, rect) -> bool:
    x0, y0, x1, y1 = box
    rx0, ry0, rx1, ry1 = rect
    return x0 < rx1 and x1 > rx0 and y0 < ry1 and y1 > ry0


def _check_text_orientation(check: DirectCheck, doc, value: dict) -> CheckResult:
    """Judge the dominant text axis inside ``rect_pt`` from glyph advance.

    Rule: sum the absolute centre-to-centre delta of consecutive glyphs, along x
    and along y, and report the larger axis. Glyph box SIZE is deliberately not
    used: a horizontal line of text has taller boxes (~12pt) than wide ones
    (~7pt), so comparing summed box dimensions reports horizontal text as
    vertical. Measured evidence for both rules is recorded in
    ``docs/specifications/harness-release-gate.spec.md``.

    A rect holding no glyphs fails the check. An absent measurement is not
    evidence of either axis, and must not pass a ``horizontal`` expectation.
    """
    page_idx = int(value["page"])
    rect = [float(v) for v in value["rect_pt"]]
    expected = value["expected"]

    page = doc[page_idx]
    height = page.get_size()[1]
    boxes = _glyph_boxes(page, height)
    page.close()

    in_rect = [b for b in boxes if _intersects(b, rect)]
    if not in_rect:
        return CheckResult(
            check.id,
            check.input,
            False,
            f"{check.id}: input={check.input} text_orientation page={page_idx} "
            f"rect={rect!r} expected={expected!r} actual='no glyphs in rect'",
        )

    centres = [((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0) for b in in_rect]
    sum_dx = sum(abs(b[0] - a[0]) for a, b in zip(centres, centres[1:]))
    sum_dy = sum(abs(b[1] - a[1]) for a, b in zip(centres, centres[1:]))
    dominant = "horizontal" if sum_dx >= sum_dy else "vertical"

    passed = dominant == expected
    detail = (
        f"{check.id}: input={check.input} text_orientation page={page_idx} "
        f"rect={rect!r} expected={expected!r} actual={dominant!r} "
        f"glyphs={len(in_rect)} advance_x={sum_dx:.1f} advance_y={sum_dy:.1f}"
    )
    return CheckResult(check.id, check.input, passed, detail)


def _link_annotations(doc, page_idx: int) -> list[tuple[str, list[float]]]:
    """Return ``(uri, rect_top_left)`` for URI link annotations on a page."""
    import pypdfium2 as pdfium

    raw = pdfium.raw
    page = doc[page_idx]
    height = page.get_size()[1]
    out = []
    count = raw.FPDFPage_GetAnnotCount(page)
    for i in range(count):
        annot = raw.FPDFPage_GetAnnot(page, i)
        try:
            if raw.FPDFAnnot_GetSubtype(annot) != _FPDF_ANNOT_LINK:
                continue
            rect = raw.FS_RECTF()
            raw.FPDFAnnot_GetRect(annot, ctypes.byref(rect))
            link = raw.FPDFAnnot_GetLink(annot)
            if not link:
                continue
            action = raw.FPDFLink_GetAction(link)
            if not action:
                continue
            if raw.FPDFAction_GetType(action) != _FPDF_ACTION_URI:
                continue
            buf = ctypes.create_string_buffer(2048)
            ulen = raw.FPDFAction_GetURIPath(doc, action, buf, 2048)
            uri = buf.raw[:ulen].rstrip(b"\x00").decode("utf-8", "replace")
            top_left = [
                float(rect.left),
                height - float(rect.top),
                float(rect.right),
                height - float(rect.bottom),
            ]
            out.append((uri, top_left))
        finally:
            raw.FPDFPage_CloseAnnot(annot)
    return out


def _rect_within(actual, expected, tolerance: float) -> bool:
    return all(abs(a - e) <= tolerance for a, e in zip(actual, expected))


def _check_link_target(check: DirectCheck, doc, value: dict) -> CheckResult:
    page_idx = int(value["page"])
    uri = value["uri"]
    rect = value.get("rect_pt")
    tolerance = value.get("tolerance_pt", 0.5)

    links = _link_annotations(doc, page_idx)
    found_uris = [u for u, _ in links]
    if rect is None:
        passed = uri in found_uris
        actual = found_uris
    else:
        expected_rect = [float(v) for v in rect]
        passed = any(
            u == uri and _rect_within(r, expected_rect, tolerance) for u, r in links
        )
        actual = [r for u, r in links if u == uri]

    detail = (
        f"{check.id}: input={check.input} link_target page={page_idx} "
        f"expected={value!r} actual={actual!r}"
    )
    return CheckResult(check.id, check.input, passed, detail)


# ---------------------------------------------------------------------------
# Manifest execution
# ---------------------------------------------------------------------------


def run_manifest(
    manifest_path,
    *,
    wpt_root,
    corpus_root,
    fixture_root,
    engine_cfg,
    spec,
    dpi=DEFAULT_DPI,
) -> list[CheckResult]:
    """Render each manifest input through ``engine_cfg`` and run its checks.

    A render that raises yields a failing :class:`CheckResult` (never aborts the
    manifest run).
    """
    checks = load_manifest(Path(manifest_path))
    engine = engine_cfg.build()
    results: list[CheckResult] = []
    try:
        for check in checks:
            input_path = _resolve_input(check, wpt_root, corpus_root, fixture_root)
            try:
                pdf = engine.render_pdf(input_path, spec)
            except Exception as exc:  # noqa: BLE001 -- a render failure is a failed check
                results.append(
                    CheckResult(
                        check.id,
                        check.input,
                        False,
                        f"{check.id}: input={check.input} render failed: "
                        f"{type(exc).__name__}: {exc}",
                    )
                )
                continue
            results.append(check_pdf(pdf, check, dpi=dpi))
    finally:
        close = getattr(engine, "close", None)
        if close:
            close()
    return results
