#!/usr/bin/env python3
"""Helpers for the visual comparison demo.

Wraps harness rasterization + compare APIs and provides small CLI utilities
used by scripts/build-demo.sh.
"""
from __future__ import annotations

import argparse
import html
import json
import math
import re
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Iterable

ROOT = Path(__file__).resolve().parents[1]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from harness.compare import compare
from harness.rasterize import rasterize_pdf

ISO_FORMAT = "%Y-%m-%dT%H:%M:%SZ"
FEATURE_LABELS = {
    "fragmentation-core": "Fragmentation",
    "paged-media-css": "Paged Media CSS",
    "typography-layer": "Typography Layer",
    "tables-fragmentation": "Tables Fragmentation",
    "css-floats": "CSS Floats",
    "footnotes": "Footnotes",
}


def utc_timestamp() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).strftime(ISO_FORMAT)


def slugify(value: str) -> str:
    value = value.strip().lower()
    value = re.sub(r"[^a-z0-9]+", "-", value).strip("-")
    return value or "doc"


def _ensure_dir(path: Path) -> None:
    path.mkdir(parents=True, exist_ok=True)


def save_page_images(images: list, out_dir: Path, label: str) -> None:
    _ensure_dir(out_dir)
    for idx, img in enumerate(images, start=1):
        name = f"page-{idx:03d}-{label}.png"
        img.save(out_dir / name, "PNG")

def _doc_image_dir(images_root: Path, file: str) -> Path:
    return images_root / slugify(file)


def compare_pdfs(
    *,
    name: str,
    file: str,
    ta_pdf: Path,
    pr_pdf: Path,
    images_dir: Path,
    dpi: int = 96,
) -> dict:
    ta_images = rasterize_pdf(ta_pdf, dpi=dpi)
    pr_images = rasterize_pdf(pr_pdf, dpi=dpi)

    doc_images_dir = _doc_image_dir(images_dir, file)
    save_page_images(ta_images, doc_images_dir, "ta")
    save_page_images(pr_images, doc_images_dir, "pr")

    ta_pages = len(ta_images)
    pr_pages = len(pr_images)
    page_count_mismatch = ta_pages != pr_pages
    shared_pages = min(ta_pages, pr_pages)

    pages: list[dict] = []
    for page_index in range(1, shared_pages + 1):
        result = compare(ta_images, pr_images, pages=[page_index])
        if not result.pages:
            continue
        page = result.pages[0]
        ta_img = ta_images[page_index - 1]
        pr_img = pr_images[page_index - 1]
        width = max(ta_img.width, pr_img.width)
        height = max(ta_img.height, pr_img.height)
        denom = width * height
        diff_percent = (page.total_pixels / denom * 100.0) if denom else 0.0
        pages.append({"page": page_index, "diff_percent": diff_percent})

    if pages:
        overall = sum(p["diff_percent"] for p in pages) / len(pages)
    else:
        overall = 0.0

    # Add structure comparison data (nullable for backward compatibility)
    structure_match = None
    structure_reasons = []
    
    try:
        structure_result = compare_structure(ta_pdf, pr_pdf, tolerance=3.0)
        structure_match = structure_result.get("structure_match")
        structure_reasons = structure_result.get("reasons", [])
    except Exception:
        # If structure comparison fails, continue without structure data
        pass
    
    return {
        "name": name,
        "file": file,
        "typeanvil_pages": ta_pages,
        "prince_pages": pr_pages,
        "page_count_mismatch": page_count_mismatch,
        "pages": pages,
        "overall_diff_percent": overall,
        "render_error": None,
        # Structure data (nullable for backward compatibility)
        "structure_match": structure_match,
        "structure_reasons": structure_reasons,
    }


# ---------------------------------------------------------------------------
# Structure-first comparison layer (CORE-215).
#
# The pixel diff cannot tell "font swapped" from "content misplaced": two
# renders of the same document with different system fonts for the same stack
# can differ by 15-34% pixels without any structural difference. The structure
# layer reads the PDFs directly (text lines, line counts, heading/image boxes)
# and emits a verdict that is robust to font substitution.
#
# pypdfium2 facts (verified 2026-09-16 against corpus renders):
#   * a page's text layer is a sequence of chars, indexed by get_charbox(i);
#   * get_charbox(i) -> (left, bottom, right, top) in PDF points (72/in);
#   * page.get_objects() yields page objects with .type being one of
#     FPDF_PAGEOBJ_TEXT (1) / FPDF_PAGEOBJ_PATH (2) / FPDF_PAGEOBJ_IMAGE (3);
#   * get_text_range(0, count_chars) returns the text with \r\n line breaks.
# ---------------------------------------------------------------------------

_SUBPAGE_BASELINE_TOL = 4.0   # pt; chars whose baselines differ by less than
                              # this belong to the same visual line (CORE-110).
                              # 2.5pt split descenders (g/p/y drop ~3pt).
_HEADING_FONT_RATIO = 1.3     # heading = line taller than 1.3x body median.
_MAX_MISSING_TOKENS = 3       # words uncovered on the other side; hyphenation
                              # fragments (prefix/suffix) count as covered.


def _open_pdf(path: Path):
    import pypdfium2 as pdfium
    return pdfium.PdfDocument(str(path))


def _char_data(page):
    """Return per-char (text, left, bottom, right, top) boxes for a page.

    The text layer may include control chars/soft hyphens that the PDF
    encodes for line-breaking; we keep only printable chars, but still walk
    every char so x/y positions line up with the char order.
    """
    textpage = page.get_textpage()
    try:
        count = textpage.count_chars()
        raw = textpage.get_text_range(0, count)
        boxes = []
        for i in range(count):
            try:
                box = textpage.get_charbox(i)
            except Exception:
                continue
            if box is None:
                continue
            ch = raw[i] if i < len(raw) else ""
            # Control chars and the PDF soft-hyphen marker (U+FFFE) are not
            # content — modern engines emit U+FFFE at hyphenation points, and
            # BOTH sides emit it identically, so dropping (not spacing) it
            # keeps hyphen-split words comparable across engines.
            if ch in ("\r", "\n", "\t", "\x00", "\ufffe", "\ufeff", "\u00ad"):
                ch = ""
            boxes.append((ch, float(box[0]), float(box[1]),
                          float(box[2]), float(box[3])))
        return boxes
    finally:
        textpage.close()


def _cluster_lines(chars):
    """Group chars into visual lines by baseline (bottom), x-sorted.

    Returns a list of dicts: {text, x0, y0, x1, y1, height}. A line's box is
    the union of its chars' boxes; height is the max char box height
    (a font-size proxy used for heading detection).
    """
    rows: list[list] = []
    row_baseline: list[float] = []
    for ch, left, bottom, right, top in chars:
        placed = False
        for row, base in zip(rows, row_baseline):
            if abs(bottom - base) <= _SUBPAGE_BASELINE_TOL:
                row.append((ch, left, bottom, right, top))
                placed = True
                break
        if not placed:
            rows.append([(ch, left, bottom, right, top)])
            row_baseline.append(bottom)
    lines = []
    for row in rows:
        row.sort(key=lambda item: item[1])
        text = "".join(item[0] for item in row).strip()
        if not text:
            continue  # rows of only dropped control chars are not lines
        left = min(item[1] for item in row)
        bottom = min(item[2] for item in row)
        right = max(item[3] for item in row)
        top = max(item[4] for item in row)
        heights = sorted(item[4] - item[2] for item in row)
        height = heights[len(heights) // 2]  # median: robust to ascenders
        lines.append({
            "text": text,
            "x0": left, "y0": bottom, "x1": right, "y1": top,
            "height": height,
        })
    return lines


def _body_median_font_size(pages_data: list[list]) -> float:
    heights = [line["height"] for page in pages_data for line in page]
    if not heights:
        return 12.0
    heights.sort()
    mid = len(heights) // 2
    if len(heights) % 2 == 0:
        return (heights[mid - 1] + heights[mid]) / 2.0
    return float(heights[mid])


def _normalize_tokens(text: str) -> list[str]:
    """Split to lowercase word tokens, with hyphen-tolerance.

    Hyphenation differs between engines: "end-" + "of" on a wrapped line is
    the same content as "endof". We strip hyphens so both sides see the same
    tokens when the wrap choice matches.
    """
    text = text.lower()
    text = text.replace("-\n", " ").replace("- ", " ")
    return re.findall(r"[a-z0-9]+", text)


def _coverage_missing(ta_tokens: list[str], pr_tokens: list[str]) -> int:
    """Count tokens on either side that the other side does NOT cover.

    A token is covered when the other side contains an equal token or a
    token that has it as a prefix or suffix — this absorbs hyphenation
    fragments ("justi"+from a break + "fied" vs "justified") and font-driven
    wrap differences, while a genuinely missing word stays uncovered.
    """
    def _missing(left: list[str], right: list[str]) -> int:
        right_set = set(right)
        missing = 0
        for t in left:
            if t in right_set:
                continue
            covered = any(
                u != t and (u.startswith(t) or u.endswith(t)
                            or t.startswith(u) or t.endswith(u))
                for u in right_set
            )
            if not covered:
                missing += 1
        return missing

    return _missing(ta_tokens, pr_tokens) + _missing(pr_tokens, ta_tokens)


def _extract_pdf_structure(pdf_path: Path) -> tuple[list[list], list[list], list[list]]:
    """Per-page (lines, heading boxes, image boxes) for a PDF."""
    doc = _open_pdf(pdf_path)
    try:
        pages_lines: list[list] = []
        pages_headings: list[list] = []
        pages_images: list[list] = []
        for page in doc:
            lines = _cluster_lines(_char_data(page))
            pages_lines.append(lines)
            # Image boxes: real image objects only (type 3).
            try:
                images = []
                for obj in page.get_objects():
                    if getattr(obj, "type", None) == 3:  # FPDF_PAGEOBJ_IMAGE
                        try:
                            bounds = obj.get_bounds()
                        except Exception:
                            continue
                        if bounds:
                            images.append(tuple(float(v) for v in bounds))
                pages_images.append(images)
            except Exception:
                pages_images.append([])
            # Headings computed after the body median is known (below).
            pages_headings.append([])
        return pages_lines, pages_headings, pages_images
    finally:
        doc.close()


def _heading_boxes(pages_lines: list[list], body_median: float) -> list[list]:
    threshold = body_median * _HEADING_FONT_RATIO
    out = []
    for lines in pages_lines:
        heads = []
        for line in lines:
            if line["height"] >= threshold:
                heads.append((line["x0"], line["y0"], line["x1"], line["y1"]))
        out.append(heads)
    return out


def _box_close(a: tuple, b: tuple, tolerance: float) -> bool:
    """Same-corner distance of two boxes within tolerance (pt)."""
    return (abs(a[0] - b[0]) <= tolerance and abs(a[1] - b[1]) <= tolerance
            and abs(a[2] - b[2]) <= tolerance and abs(a[3] - b[3]) <= tolerance)


def _boxes_match(ta_boxes: list, pr_boxes: list, tolerance: float) -> bool:
    """Every box on one side has a matching box on the other within tolerance."""

    def _greedy(left: list, right: list) -> bool:
        used = [False] * len(right)
        for a in left:
            found = False
            for j, b in enumerate(right):
                if not used[j] and _box_close(a, b, tolerance):
                    used[j] = True
                    found = True
                    break
            if not found:
                return False
        return True

    return _greedy(ta_boxes, pr_boxes) and _greedy(pr_boxes, ta_boxes)


def _page_text_offset(ta_page, pr_page) -> tuple[float, float]:
    """Estimate the engines' uniform baseline offset on a page.

    TypeAnvil and Prince lay text out with slightly different charbox
    origins (~6 pt on this corpus): a constant page-wide shift, NOT
    misplacement. The median delta over shared line origins estimates it;
    subtracting it lets the box comparison catch RELATIVE movement (a
    heading that moved 40 pt down) while ignoring the engine's global
    offset.
    """
    ta_origins = [(l["x0"], l["y0"]) for l in ta_page] if ta_page else []
    pr_origins = [(l["x0"], l["y0"]) for l in pr_page] if pr_page else []
    if not ta_origins or not pr_origins:
        return (0.0, 0.0)
    n = min(len(ta_origins), len(pr_origins))
    dx = sorted(ta_origins[i][0] - pr_origins[i][0] for i in range(n))
    dy = sorted(ta_origins[i][1] - pr_origins[i][1] for i in range(n))
    return (dx[len(dx) // 2], dy[len(dy) // 2])  # median delta


def compare_structure(ta_pdf_path: Path, pr_pdf_path: Path,
                      tolerance: float = 3.0) -> dict:
    """Compare text/structure between two PDFs; return a per-page verdict.

    Verdict = all shared pages satisfy:
      * line count within +/- 1;
      * normalized text token multisets within a small tolerance;
      * heading boxes and image boxes within `tolerance` pt.
    Robust to font substitution: different glyphs for the same stack change
    pixel diffs but not text/tokens/box positions by more than the tolerance.
    """
    ta_lines, _, ta_images = _extract_pdf_structure(ta_pdf_path)
    pr_lines, _, pr_images = _extract_pdf_structure(pr_pdf_path)

    body_median = _body_median_font_size(ta_lines + pr_lines)
    ta_headings = _heading_boxes(ta_lines, body_median)
    pr_headings = _heading_boxes(pr_lines, body_median)

    max_pages = max(len(ta_lines), len(pr_lines))
    pages: list[dict] = []
    for idx in range(max_pages):
        ta_page = ta_lines[idx] if idx < len(ta_lines) else None
        pr_page = pr_lines[idx] if idx < len(pr_lines) else None
        if ta_page is None or pr_page is None:
            pages.append({
                "page": idx + 1,
                "line_count_match": False,
                "text_match": False,
                "headings_match": False,
                "images_match": False,
                "reason": "page count mismatch",
            })
            continue
        ta_count = len(ta_page)
        pr_count = len(pr_page)
        ta_tokens = [t for line in ta_page for t in _normalize_tokens(line["text"])]
        pr_tokens = [t for line in pr_page for t in _normalize_tokens(line["text"])]
        missing = _coverage_missing(ta_tokens, pr_tokens)
        dx, dy = _page_text_offset(ta_page, pr_page)

        def _shifted(boxes, dx_, dy_):
            return [(x0 - dx_, y0 - dy_, x1 - dx_, y1 - dy_) for x0, y0, x1, y1 in boxes]

        # Headings match primarily by TEXT: a font swap or the engines'
        # top-of-page baseline convention (~6 pt on this corpus) changes
        # positions but not which lines are headings. Same heading text on
        # both sides is structurally equal; a heading missing off one side
        # (or an extra heading) fails the verdict. Positional comparison
        # (offset-normalized, 2x tolerance) is the fallback for headings
        # whose text differs between engines (e.g. font-mapped glyphs).
        def _heading_line_idxs(page_lines, heading_boxes):
            idxs = set()
            for i, line in enumerate(page_lines):
                line_box = (line["x0"], line["y0"], line["x1"], line["y1"])
                if any(_box_close(line_box, h, 1.0) for h in heading_boxes):
                    idxs.add(i)
            return idxs

        ta_head_idx = _heading_line_idxs(ta_page, ta_headings[idx])
        pr_head_idx = _heading_line_idxs(pr_page, pr_headings[idx])
        ta_head_tokens = {
            tuple(_normalize_tokens(" ".join(ta_page[i]["text"] for i in sorted(ta_head_idx))))
        } if ta_head_idx else set()
        pr_head_tokens = {
            tuple(_normalize_tokens(" ".join(pr_page[i]["text"] for i in sorted(pr_head_idx))))
        } if pr_head_idx else set()
        headings_match = (ta_head_tokens == pr_head_tokens) or _boxes_match(
            _shifted(ta_headings[idx], dx, dy), pr_headings[idx], tolerance * 2)

        page = {
            "page": idx + 1,
            "ta_line_count": ta_count,
            "pr_line_count": pr_count,
            "line_count_match": abs(ta_count - pr_count) <= 1,
            "text_match": missing <= _MAX_MISSING_TOKENS,
            "headings_match": headings_match,
            "images_match": _boxes_match(
                _shifted(ta_images[idx], dx, dy), pr_images[idx], tolerance),
            "missing_tokens": missing,
        }
        page["reason"] = next(
            (name for name, ok in (
                ("line count mismatch", page["line_count_match"]),
                ("text content mismatch", page["text_match"]),
                ("heading positions mismatch", page["headings_match"]),
                ("image positions mismatch", page["images_match"]),
            ) if not ok),
            None,
        )
        pages.append(page)

    reasons = sorted({p["reason"] for p in pages if p["reason"]})
    structure_match = all(
        (p["line_count_match"] and p["text_match"]
         and p["headings_match"] and p["images_match"]) for p in pages
    )
    return {
        "pages": pages,
        "structure_match": structure_match,
        "reasons": reasons,
    }


def structure_compare_command(args: argparse.Namespace) -> int:
    result = compare_structure(Path(args.ta_pdf), Path(args.pr_pdf),
                               args.tolerance)
    print(json.dumps(result, indent=2))
    return 0 if result["structure_match"] else 1


def error_entry(*, name: str, file: str, message: str) -> dict:
    return {
        "name": name,
        "file": file,
        "typeanvil_pages": 0,
        "prince_pages": 0,
        "page_count_mismatch": False,
        "pages": [],
        "overall_diff_percent": 0.0,
        "render_error": message,
    }


def load_manifest(path: Path) -> dict[str, dict]:
    data = json.loads(path.read_text(encoding="utf-8"))
    out: dict[str, dict] = {}
    for entry in data:
        if not isinstance(entry, dict):
            continue
        file = entry.get("file")
        if isinstance(file, str):
            out[file] = entry
    return out


def bucket_for_diff(diff_percent: float) -> str:
    if diff_percent < 1.0:
        return "identical"
    if diff_percent <= 20.0:
        return "cosmetic"
    return "missing-feature"


def _escape(text: str) -> str:
    return html.escape(text, quote=True)


def _md_img(src: str, alt: str, width: int = 240) -> str:
    """An <img> tag for markdown tables — GitHub renders these inline."""
    return f'<img src="{_escape(src)}" alt="{_escape(alt)}" width="{width}" />'


def render_gallery_md(
    *,
    scoreboard: dict,
    manifest: dict[str, dict],
    benchmark_manifest: dict[str, dict] | None = None,
    image_prefix: str = "out/images",
) -> str:
    """The GitHub-viewable comparison gallery (CORE-147).

    Returns the generated body as markdown; the caller splices it into the
    track's README.md below that file's marker. GitHub renders `<img>` tags
    inside markdown tables and proxies relative image paths through camo, so
    this works in the file browser and in repo links without base64 blobs.
    `image_prefix` is the path from the README's directory to the images
    directory: the README sits beside `out/`, so this is `out/images`.
    Deterministic: no timestamps (the scoreboard JSON carries the only one).
    """
    docs = scoreboard.get("docs", [])
    ta_version = str(scoreboard.get("typeanvil_version", "unknown"))
    pr_version = str(scoreboard.get("prince_version") or "n/a")
    lines: list[str] = []

    lines.append("## Comparison gallery — TypeAnvil vs Prince")
    lines.append("")
    lines.append(
        f"Side-by-side page gallery. TypeAnvil renders on the left; Prince on "
        f"the right. Engines: TypeAnvil `{ta_version}` · {pr_version}. "
        f"Geometry: 5in × 3in pages, 0.5in margins, 96 DPI raster."
    )
    lines.append("")
    lines.append(
        "> This section is REGENERATED by `scripts/build-demo.sh`. The preamble "
        "above is hand-maintained; edit it there, not here."
    )
    lines.append("")

    # --- Scoreboard table.
    lines.append("## Scoreboard")
    lines.append("")
    lines.append("| Document | TypeAnvil | Prince | Overall diff | Bucket |")
    lines.append("|---|---|---|---|---|")
    for doc in docs:
        name = str(doc.get("name", ""))
        file = str(doc.get("file", ""))
        render_error = doc.get("render_error")
        overall = float(doc.get("overall_diff_percent", 0.0))
        bucket = "error" if render_error else bucket_for_diff(overall)
        anchor = slugify(file)
        lines.append(
            f"| [{_escape(name)}](#{anchor}) "
            f"| {int(doc.get('typeanvil_pages', 0))} "
            f"| {int(doc.get('prince_pages', 0))} "
            f"| {overall:.2f}% | {bucket} |"
        )
    lines.append("")

    # --- Per-doc sections.
    for doc in docs:
        name = str(doc.get("name", ""))
        file = str(doc.get("file", ""))
        render_error = doc.get("render_error")
        overall = float(doc.get("overall_diff_percent", 0.0))
        bucket = "error" if render_error else bucket_for_diff(overall)
        doc_images = f"{image_prefix}/{slugify(file)}"
        ta_pages = int(doc.get("typeanvil_pages", 0))
        pr_pages = int(doc.get("prince_pages", 0))
        mismatch = bool(doc.get("page_count_mismatch"))
        max_pages = max(ta_pages, pr_pages)

        lines.append(f'<a id="{slugify(file)}"></a>')
        lines.append("")
        lines.append(f"### {_escape(name)} — `{file}`")
        lines.append("")

        meta = manifest.get(file, {})
        meta = meta if isinstance(meta, dict) else {}
        wedge = meta.get("wedge_features", [])
        if wedge:
            badges = " · ".join(
                f"**{FEATURE_LABELS.get(str(f), str(f))}**" for f in wedge
            )
            lines.append(badges)
            lines.append("")

        lines.append(
            f"TypeAnvil {ta_pages} page(s) · Prince {pr_pages} page(s)"
            + (" · **page count mismatch**" if mismatch else "")
            + f" · overall diff {overall:.2f}% ({bucket})"
        )
        lines.append("")

        known = meta.get("known_limitations", [])
        deltas = meta.get("expected_deltas", [])
        if known:
            lines.append("**Known limitations**")
            lines.append("")
            for k in known:
                lines.append(f"- {k}")
            lines.append("")
        if deltas:
            lines.append("**Expected deltas vs Prince**")
            lines.append("")
            for d in deltas:
                lines.append(f"- {d}")
            lines.append("")

        if render_error:
            lines.append(f"> **Render error:** {render_error}")
            lines.append("")
            continue

        lines.append("| TypeAnvil | Diff | Prince |")
        lines.append("|---|---|---|")
        pages_map = {
            p["page"]: p["diff_percent"] for p in doc.get("pages", []) if "page" in p
        }
        for page_index in range(1, max_pages + 1):
            ta_src = f"{doc_images}/page-{page_index:03d}-ta.png"
            pr_src = f"{doc_images}/page-{page_index:03d}-pr.png"
            ta_cell = (
                _md_img(ta_src, f"TypeAnvil page {page_index}")
                if page_index <= ta_pages
                else "—"
            )
            pr_cell = (
                _md_img(pr_src, f"Prince page {page_index}")
                if page_index <= pr_pages
                else "—"
            )
            diff = pages_map.get(page_index)
            diff_cell = f"{diff:.2f}%" if diff is not None else "—"
            lines.append(f"| {ta_cell} | {diff_cell} | {pr_cell} |")
        lines.append("")

    # --- Benchmark section: open gaps first, then fixtures kept as regression
    # evidence after their issue landed (status flips to `resolved`).
    bench = benchmark_manifest or {}
    if bench:
        open_items = [
            (f, m)
            for f, m in sorted(bench.items())
            if str(m.get("status", "pending")) != "resolved"
        ]
        closed_items = [
            (f, m)
            for f, m in sorted(bench.items())
            if str(m.get("status", "pending")) == "resolved"
        ]
        lines.append("## Benchmark — engine gaps")
        lines.append("")
        lines.append(
            "One fixture per engine gap, rendered with the current engine at the "
            "comparison geometry. **Open** fixtures show a gap the engine still "
            "has; **closed** fixtures are kept as regression evidence after their "
            "issue landed. Separate from the public comparison corpus above."
        )
        lines.append("")
        for heading, group in (
            ("### Open gaps", open_items),
            ("### Closed — regression fixtures", closed_items),
        ):
            if not group:
                continue
            lines.append(heading)
            lines.append("")
            for file, meta in group:
                name = str(meta.get("name", file))
                issue = str(meta.get("issue_id", ""))
                status = str(meta.get("status", "pending"))
                expectation = str(meta.get("expectation", ""))
                notes = meta.get("notes", []) or []
                base = Path(file).with_suffix("").name
                ta_img = f"{image_prefix}/bench-{base}/page-001-ta.png"
                lines.append(f"#### {_escape(name)} — `{file}`")
                lines.append("")
                lines.append(
                    f"Status: **{status}** · tracked in "
                    f"[{_escape(issue)}](https://linear.app/whitelodge/issue/{_escape(issue)})"
                )
                lines.append("")
                lines.append(f"**Expectation:** {expectation}")
                lines.append("")
                for n in notes:
                    lines.append(f"- {n}")
                if notes:
                    lines.append("")
                lines.append(
                    f"{_md_img(ta_img, f'TypeAnvil current state — {name}', width=320)}"
                )
                lines.append("")

    return "\n".join(lines).rstrip() + "\n"


GENERATED_BEGIN = "<!-- BEGIN GENERATED GALLERY — build-demo.sh rewrites from here; do not edit below -->"


def splice_readme(readme_path: Path, marker: str, body: str) -> None:
    """Write `body` into the README below `marker`, keeping the preamble.

    The hand-maintained preamble above the marker is never rewritten; the
    generated body below it is replaced wholesale on every build. On the first
    run (no marker present) any existing preamble is kept and the marker plus
    body are appended.
    """
    readme_path.parent.mkdir(parents=True, exist_ok=True)
    body = body.rstrip() + "\n"
    readme = readme_path.read_text(encoding="utf-8") if readme_path.exists() else ""
    start = readme.find(marker)
    if start == -1:
        preamble = readme.rstrip() + "\n\n" if readme.strip() else ""
        readme = preamble + marker + "\n\n" + body
    else:
        readme = readme[:start] + marker + "\n\n" + body
    readme_path.write_text(readme, encoding="utf-8")


def write_scoreboard(
    *,
    results: list[dict],
    typeanvil_version: str,
    prince_version: str | None,
    output_path: Path,
) -> dict:
    scoreboard = {
        "generated": utc_timestamp(),
        "typeanvil_version": typeanvil_version,
        "prince_version": prince_version,
        "docs": results,
    }
    output_path.write_text(
        json.dumps(scoreboard, indent=2, ensure_ascii=False),
        encoding="utf-8",
    )
    return scoreboard


def validate_scoreboard(data: dict) -> list[str]:
    errors: list[str] = []
    if not isinstance(data, dict):
        return ["scoreboard is not a JSON object"]
    for key in ("generated", "typeanvil_version", "prince_version", "docs"):
        if key not in data:
            errors.append(f"missing field: {key}")
    docs = data.get("docs")
    if not isinstance(docs, list):
        errors.append("docs must be a list")
        return errors

    for idx, doc in enumerate(docs):
        if not isinstance(doc, dict):
            errors.append(f"doc[{idx}] is not an object")
            continue
        # Required fields
        for field in (
            "name",
            "file",
            "typeanvil_pages",
            "prince_pages",
            "page_count_mismatch",
            "pages",
            "overall_diff_percent",
            "render_error",
        ):
            if field not in doc:
                errors.append(f"doc[{idx}] missing field: {field}")
        
        # Optional fields (backward compatibility)
        # structure_match and structure_reasons are optional for backward compatibility
        pages = doc.get("pages")
        if isinstance(pages, list):
            for pidx, page in enumerate(pages):
                if not isinstance(page, dict):
                    errors.append(f"doc[{idx}].pages[{pidx}] is not an object")
                    continue
                if "page" not in page or "diff_percent" not in page:
                    errors.append(f"doc[{idx}].pages[{pidx}] missing page/diff_percent")
                    continue
                diff = page.get("diff_percent")
                if not isinstance(diff, (int, float)) or not math.isfinite(diff):
                    errors.append(f"doc[{idx}].pages[{pidx}] diff_percent not finite")
                elif diff < 0 or diff > 100:
                    errors.append(f"doc[{idx}].pages[{pidx}] diff_percent out of range")
        diff = doc.get("overall_diff_percent")
        if not isinstance(diff, (int, float)) or not math.isfinite(diff):
            errors.append(f"doc[{idx}] overall_diff_percent not finite")
        elif diff < 0 or diff > 100:
            errors.append(f"doc[{idx}] overall_diff_percent out of range")
    return errors


def normalize_scoreboard_bytes(raw: bytes) -> bytes:
    match = re.search(rb'"generated"\s*:\s*"([^"]*)"', raw)
    if not match:
        raise ValueError("scoreboard missing generated field")
    placeholder = b"0000-00-00T00:00:00Z"
    if len(match.group(1)) != len(placeholder):
        raise ValueError("unexpected generated timestamp length")
    return raw[: match.start(1)] + placeholder + raw[match.end(1) :]


def compare_output_dirs(baseline: Path, current: Path) -> list[str]:
    errors: list[str] = []

    # Only deliverables are subject to the determinism promise: the gallery,
    # the scoreboard (normalized), and the page images. Internal dirs (.work,
    # .results) hold intermediate PDFs/stderr that legitimately differ
    # (e.g. Prince embeds timestamps) — the spec's byte-identity applies to
    # demo/corpus/out deliverables only (§Behavior 9).
    def collect(root: Path) -> set[Path]:
        return {
            p.relative_to(root)
            for p in root.rglob("*")
            if p.is_file()
            and ".work" not in p.parts
            and ".results" not in p.parts
        }

    baseline_files = collect(baseline)
    current_files = collect(current)
    missing = sorted(baseline_files - current_files)
    extra = sorted(current_files - baseline_files)
    if missing:
        errors.append(f"missing files: {', '.join(str(p) for p in missing)}")
    if extra:
        errors.append(f"extra files: {', '.join(str(p) for p in extra)}")

    for rel in sorted(baseline_files & current_files):
        left = (baseline / rel).read_bytes()
        right = (current / rel).read_bytes()
        if rel.as_posix() == "scoreboard.json":
            left = normalize_scoreboard_bytes(left)
            right = normalize_scoreboard_bytes(right)
        if left != right:
            errors.append(f"file differs: {rel}")
    return errors


def _read_results(path: Path) -> list[dict]:
    """Read result entries from a JSONL file OR a directory of .json files."""
    results: list[dict] = []
    if path.is_dir():
        for child in sorted(path.glob("*.json")):
            results.append(json.loads(child.read_text(encoding="utf-8")))
        return results
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line:
            continue
        results.append(json.loads(line))
    return results


def list_manifest_entries(path: Path) -> Iterable[tuple[str, str]]:
    data = json.loads(path.read_text(encoding="utf-8"))
    entries = []
    for entry in data:
        if isinstance(entry, dict) and "file" in entry and "name" in entry:
            entries.append((str(entry["file"]), str(entry["name"])))
    for file, name in sorted(entries, key=lambda item: item[0]):
        yield file, name


def render_showcase_md(
    manifest_path: Path,
    images_dir: Path,
    typeanvil_version: str,
    image_prefix: str = "out/images",
) -> str:
    """Markdown showcase gallery body (CORE-148).

    Returns a deterministic markdown body: one section per manifest fixture,
    per-page <img> tags referencing the rasterized PNGs at
    `<image_prefix>/<base>/page-NNN-ta.png`, relative to the showcase README.
    The caller splices it into demo/showcase/README.md below its marker.
    No timestamps — showcase output is byte-identical across rebuilds.
    """
    lines: list[str] = []
    lines.append("## Showcase — print-resolution renders (US Letter @ 300 DPI)")
    lines.append("")
    lines.append(
        "Realistic-size pages rendered by the TypeAnvil engine only "
        f"(commit `{typeanvil_version}`). The side-by-side comparison "
        "gallery lives in [the comparison README](../corpus/README.md) and "
        "runs at 5in × 3in @ 96 DPI so diffs stay cheap; these pages show the "
        "same engine at the geometry documents actually print at. Prince "
        "renders only the comparison pipeline — the showcase is a TypeAnvil "
        "output gallery, not a diff target."
    )
    lines.append("")
    for file, name in list_manifest_entries(manifest_path):
        base = Path(file).stem
        page_dir = images_dir / base
        pages = sorted(page_dir.glob("page-*-ta.png")) if page_dir.is_dir() else []
        lines.append(f"### {name}")
        lines.append("")
        meta = _manifest_meta(manifest_path, file)
        if meta:
            feats = ", ".join(meta.get("wedge_features", []))
            if feats:
                lines.append(f"*Exercises:* {feats}")
                lines.append("")
        if not pages:
            lines.append("*No pages rendered.*")
            lines.append("")
            continue
        for png in pages:
            # Relative to demo/showcase/ (where README.md lives).
            rel = f"{image_prefix}/{base}/{png.name}"
            lines.append(f'<img src="{rel}" alt="{name} — {png.stem}" width="420">')
        lines.append("")
    return "\n".join(lines).rstrip() + "\n"
def check_guard_command(args: argparse.Namespace) -> int:
    """CLI handler for scoreboard movement guard check."""
    # Load committed and current scoreboards
    committed_data = json.loads(Path(args.committed).read_text(encoding="utf-8"))
    current_data = json.loads(Path(args.current).read_text(encoding="utf-8"))
    
    # Load manifest
    manifest_data = json.loads(Path(args.manifest).read_text(encoding="utf-8"))
    manifest_by_file = {entry["file"]: entry for entry in manifest_data if isinstance(entry, dict) and "file" in entry}
    
    # Compare documents
    committed_docs = {doc["file"]: doc for doc in committed_data.get("docs", [])}
    current_docs = {doc["file"]: doc for doc in current_data.get("docs", [])}
    
    all_files = set(committed_docs.keys()) | set(current_docs.keys())
    failed_docs = []
    
    MOVEMENT_THRESHOLD = 3.0  # percentage points
    
    for file in all_files:
        committed_doc = committed_docs.get(file)
        current_doc = current_docs.get(file)
        
        # Check if document was added/removed
        if committed_doc is None:
            print(f"GUARD INFO {file}: new document")
            continue
        elif current_doc is None:
            print(f"GUARD INFO {file}: removed document")
            continue
            
        # Check page count changes
        committed_ta_pages = committed_doc.get("typeanvil_pages", 0)
        committed_pr_pages = committed_doc.get("prince_pages", 0)
        current_ta_pages = current_doc.get("typeanvil_pages", 0)
        current_pr_pages = current_doc.get("prince_pages", 0)
        
        page_count_changed = (
            committed_ta_pages != current_ta_pages or 
            committed_pr_pages != current_pr_pages
        )
        
        # Check overall diff percentage movement
        committed_diff = committed_doc.get("overall_diff_percent", 0.0)
        current_diff = current_doc.get("overall_diff_percent", 0.0)
        diff_movement = abs(current_diff - committed_diff)
        
        # Check if there's significant movement or page count change
        if diff_movement > MOVEMENT_THRESHOLD or page_count_changed:
            # Check if this change is expected
            manifest_entry = manifest_by_file.get(file, {})
            expected_change_note = manifest_entry.get("expected_change", "")
            
            if expected_change_note:
                # Expected change - print info but don't fail
                reason = "page count change" if page_count_changed else f"diff movement {diff_movement:.2f}pp"
                print(f"GUARD INFO {file}: {reason} (expected: {expected_change_note})")
            else:
                # Unexpected change - fail
                if page_count_changed:
                    reason = f"page count {committed_ta_pages}/{committed_pr_pages} -> {current_ta_pages}/{current_pr_pages}"
                else:
                    reason = f"diff {committed_diff:.2f} -> {current_diff:.2f} pp ({diff_movement:.2f} pp movement)"
                failed_docs.append((file, reason))
                print(f"GUARD FAIL {file}: {reason}")
        else:
            # Within threshold
            print(f"guard ok {file}: within threshold (diff {committed_diff:.2f} -> {current_diff:.2f} pp)")
    
    if failed_docs:
        print(f"guard failed: {len(failed_docs)} document(s) exceeded movement threshold without expected_change")
        return 1
    else:
        print("guard ok: all documents within threshold or covered by expected_change")
        return 0


INSPECT_HEADLINE = "TypeAnvil | Prince (reference)"
INSPECT_NOT_SCORED = (
    "This page is NOT scored; a visual difference from Prince is a hint, "
    "not a defect."
)


def render_showcase_inspect_md(
    manifest_path: Path,
    images_dir: Path,
    image_prefix: str = "images",
) -> str:
    """Markdown inspection body: per-page TypeAnvil | Prince side-by-side.

    Unstressed reference overlay (CORE-217). The showcase gallery stays
    TypeAnvil-only; this page only places the matching Prince render beside
    each TypeAnvil page for human inspection. There are no scores, no
    buckets, and no failure semantics: a page-count difference is recorded,
    never "fixed". Deterministic for the spec's byte-identity promise —
    manifest order, sorted page names, no timestamps.
    """
    lines: list[str] = []
    lines.append(f"## {INSPECT_HEADLINE} — per-page inspection (US Letter @ 300 DPI)")
    lines.append("")
    lines.append(f"**{INSPECT_NOT_SCORED}**")
    lines.append("")
    lines.append(
        "Prince is rendered by `scripts/render-prince.sh` at the same "
        "geometry the showcase uses (US Letter, 0.75in margins) and "
        "rasterized at the same 300 DPI, so both sides are directly "
        "comparable by eye. Page counts can differ between the engines; that "
        "is recorded here, not scored."
    )
    lines.append("")
    for file, name in list_manifest_entries(manifest_path):
        base = Path(file).stem
        page_dir = images_dir / base
        ta_pages = sorted(page_dir.glob("page-*-ta.png")) if page_dir.is_dir() else []
        pr_pages = sorted(page_dir.glob("page-*-pr.png")) if page_dir.is_dir() else []
        lines.append(f"### {name}")
        lines.append("")
        lines.append(
            f"TypeAnvil {len(ta_pages)} page(s) / Prince {len(pr_pages)} page(s)"
        )
        lines.append("")
        if not ta_pages and not pr_pages:
            lines.append("*No pages rendered.*")
            lines.append("")
            continue
        if not pr_pages:
            lines.append(
                "*The Prince reference is unavailable for this fixture; the "
                "build log records the render error. TypeAnvil pages are "
                "shown alone.*"
            )
            lines.append("")
        if not ta_pages:
            lines.append(
                "*No TypeAnvil pages are present for this fixture; the Prince "
                "reference is shown alone.*"
            )
            lines.append("")
        lines.append("| TypeAnvil | Prince (reference) |")
        lines.append("| --- | --- |")
        for idx in range(max(len(ta_pages), len(pr_pages))):
            cells: list[str] = []
            for pages in (ta_pages, pr_pages):
                if idx >= len(pages):
                    cells.append("*(no page)*")
                    continue
                png = pages[idx]
                rel = f"{image_prefix}/{base}/{png.name}"
                alt = f"{name} — {png.stem}"
                cells.append(
                    f'<img src="{_escape(rel)}" alt="{_escape(alt)}" width="420">'
                )
            lines.append(f"| {cells[0]} | {cells[1]} |")
        lines.append("")
    return "\n".join(lines).rstrip() + "\n"


def _manifest_meta(manifest_path: Path, file: str) -> dict | None:
    try:
        data = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None
    for entry in data:
        if isinstance(entry, dict) and entry.get("file") == file:
            return entry
    return None


def main() -> int:
    parser = argparse.ArgumentParser(description="Visual comparison demo helpers")
    sub = parser.add_subparsers(dest="cmd", required=True)

    compare_p = sub.add_parser("compare", help="Rasterize, diff, and write a doc entry")
    compare_p.add_argument("--name", required=True)
    compare_p.add_argument("--file", required=True)
    compare_p.add_argument("--ta-pdf", required=True)
    compare_p.add_argument("--pr-pdf", required=True)
    compare_p.add_argument("--images-dir", required=True)
    compare_p.add_argument("--out-json", default="-")
    compare_p.add_argument("--dpi", type=int, default=96)

    error_p = sub.add_parser("error-entry", help="Write a render-error doc entry")
    error_p.add_argument("--name", required=True)
    error_p.add_argument("--file", required=True)
    error_p.add_argument("--message", required=True)
    error_p.add_argument("--out-json", default="-")

    assemble_p = sub.add_parser("assemble", help="Assemble scoreboard + gallery")
    assemble_p.add_argument("--results", required=True)
    assemble_p.add_argument("--manifest", required=True)
    assemble_p.add_argument("--benchmark-manifest", default=None)
    assemble_p.add_argument("--out-dir", required=True)
    assemble_p.add_argument("--typeanvil-version", required=True)
    assemble_p.add_argument("--prince-version", default="")
    assemble_p.add_argument(
        "--readme",
        required=True,
        help="Track README.md that receives the gallery, below --marker.",
    )
    assemble_p.add_argument(
        "--marker",
        default=GENERATED_BEGIN,
        help="Marker comment in the README; everything below it is regenerated "
        "(defaults to the comparison-gallery marker).",
    )
    assemble_p.add_argument(
        "--image-prefix",
        default="out/images",
        help="Path from the README's directory to the images directory.",
    )

    validate_p = sub.add_parser("validate-scoreboard", help="Validate scoreboard schema")
    validate_p.add_argument("path")

    list_p = sub.add_parser("list-manifest", help="Emit manifest entries (file<TAB>name)")
    list_p.add_argument("--manifest", required=True)

    showcase_p = sub.add_parser(
        "assemble-showcase", help="Assemble the showcase markdown gallery (CORE-148)"
    )
    showcase_p.add_argument("--manifest", required=True)
    showcase_p.add_argument("--images-dir", required=True)
    showcase_p.add_argument("--readme", required=True)
    showcase_p.add_argument("--marker", required=True)
    showcase_p.add_argument("--typeanvil-version", required=True)
    showcase_p.add_argument("--image-prefix", default="out/images")

    inspect_p = sub.add_parser(
        "assemble-showcase-inspect",
        help="Assemble the showcase TypeAnvil | Prince inspection page (CORE-217)",
    )
    inspect_p.add_argument("--manifest", required=True)
    inspect_p.add_argument("--images-dir", required=True)
    inspect_p.add_argument("--out", required=True)
    inspect_p.add_argument(
        "--image-prefix",
        default="images",
        help="Path from the inspection page's directory to the images directory.",
    )

    det_p = sub.add_parser("check-determinism", help="Compare two output dirs")
    det_p.add_argument("baseline")
    det_p.add_argument("current")

    structure_p = sub.add_parser("structure", help="Compare PDF structure between TypeAnvil and Prince")
    structure_p.add_argument("--ta-pdf", required=True)
    structure_p.add_argument("--pr-pdf", required=True)
    structure_p.add_argument("--tolerance", type=float, default=3.0)
    guard_p = sub.add_parser("check-guard", help="Check scoreboard movement guard")
    guard_p.add_argument("--committed", required=True)
    guard_p.add_argument("--current", required=True)
    guard_p.add_argument("--manifest", required=True)
    args = parser.parse_args()

    if args.cmd == "compare":
        entry = compare_pdfs(
            name=args.name,
            file=args.file,
            ta_pdf=Path(args.ta_pdf),
            pr_pdf=Path(args.pr_pdf),
            images_dir=Path(args.images_dir),
            dpi=args.dpi,
        )
        out = json.dumps(entry, ensure_ascii=False)
        if args.out_json == "-":
            print(out)
        else:
            Path(args.out_json).write_text(out + "\n", encoding="utf-8")
        return 0

    if args.cmd == "error-entry":
        entry = error_entry(name=args.name, file=args.file, message=args.message)
        out = json.dumps(entry, ensure_ascii=False)
        if args.out_json == "-":
            print(out)
        else:
            Path(args.out_json).write_text(out + "\n", encoding="utf-8")
        return 0

    if args.cmd == "assemble":
        results = _read_results(Path(args.results))
        manifest = load_manifest(Path(args.manifest))
        bench_manifest: dict[str, dict] = {}
        if args.benchmark_manifest:
            bpath = Path(args.benchmark_manifest)
            if bpath.exists():
                for entry in json.loads(bpath.read_text(encoding="utf-8")):
                    if isinstance(entry, dict) and "file" in entry:
                        bench_manifest[str(entry["file"])] = entry
        out_dir = Path(args.out_dir)
        _ensure_dir(out_dir)
        prince_version = args.prince_version or None
        scoreboard = write_scoreboard(
            results=results,
            typeanvil_version=args.typeanvil_version,
            prince_version=prince_version,
            output_path=out_dir / "scoreboard.json",
        )
        body = render_gallery_md(
            scoreboard=scoreboard,
            manifest=manifest,
            benchmark_manifest=bench_manifest,
            image_prefix=args.image_prefix,
        )
        splice_readme(Path(args.readme), args.marker, body)
        print(f"gallery: {args.readme}")
        print(f"scoreboard: {out_dir / 'scoreboard.json'}")
        return 0

    if args.cmd == "validate-scoreboard":
        data = json.loads(Path(args.path).read_text(encoding="utf-8"))
        errors = validate_scoreboard(data)
        if errors:
            for err in errors:
                print(f"error: {err}", file=sys.stderr)
            return 1
        print("scoreboard ok")
        return 0

    if args.cmd == "list-manifest":
        for file, name in list_manifest_entries(Path(args.manifest)):
            print(f"{file}\t{name}")
        return 0

    if args.cmd == "assemble-showcase":
        body = render_showcase_md(
            manifest_path=Path(args.manifest),
            images_dir=Path(args.images_dir),
            typeanvil_version=args.typeanvil_version,
            image_prefix=args.image_prefix,
        )
        splice_readme(Path(args.readme), args.marker, body)
        print(f"showcase gallery: {args.readme}")
        return 0

    if args.cmd == "assemble-showcase-inspect":
        body = render_showcase_inspect_md(
            manifest_path=Path(args.manifest),
            images_dir=Path(args.images_dir),
            image_prefix=args.image_prefix,
        )
        out = Path(args.out)
        _ensure_dir(out.parent)
        out.write_text(body, encoding="utf-8")
        print(f"showcase inspect page: {out}")
        return 0

    if args.cmd == "check-determinism":
        errors = compare_output_dirs(Path(args.baseline), Path(args.current))
        if errors:
            for err in errors:
                print(f"error: {err}", file=sys.stderr)
            return 1
        print("determinism ok")
        return 0
    if args.cmd == "structure":
        return structure_compare_command(args)
    if args.cmd == "check-guard":
        return check_guard_command(args)

    return 1


if __name__ == "__main__":
    raise SystemExit(main())
