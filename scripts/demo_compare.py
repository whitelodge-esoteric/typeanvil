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

    return {
        "name": name,
        "file": file,
        "typeanvil_pages": ta_pages,
        "prince_pages": pr_pages,
        "page_count_mismatch": page_count_mismatch,
        "pages": pages,
        "overall_diff_percent": overall,
        "render_error": None,
    }


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


def render_gallery(
    *,
    scoreboard: dict,
    manifest: dict[str, dict],
    output_path: Path,
    benchmark_manifest: dict[str, dict] | None = None,
) -> None:
    docs = scoreboard.get("docs", [])
    out_dir = output_path.parent
    image_rel_root = Path("images")
    benchmark_section = render_benchmark_section(
        benchmark_manifest=benchmark_manifest or {},
        images_dir=out_dir / "images",
    )

    rows = []
    sections = []
    for doc in docs:
        name = str(doc.get("name", ""))
        file = str(doc.get("file", ""))
        render_error = doc.get("render_error")
        overall = float(doc.get("overall_diff_percent", 0.0))
        bucket = "error" if render_error else bucket_for_diff(overall)

        rows.append(
            "<tr>"
            f"<td><a href=\"#doc-{slugify(file)}\">{_escape(name)}</a></td>"
            f"<td class=\"num\">{overall:.2f}%</td>"
            f"<td class=\"bucket bucket-{bucket}\">{bucket}</td>"
            "</tr>"
        )

        meta = manifest.get(file, {})
        wedge_features = meta.get("wedge_features", []) if isinstance(meta, dict) else []
        known_limitations = meta.get("known_limitations", []) if isinstance(meta, dict) else []
        expected_deltas = meta.get("expected_deltas", []) if isinstance(meta, dict) else []

        feature_badges = []
        for feat in wedge_features:
            label = FEATURE_LABELS.get(str(feat), str(feat))
            feature_badges.append(f"<span class=\"badge\">{_escape(label)}</span>")

        limitations_html = (
            "<ul>" + "".join(f"<li>{_escape(str(item))}</li>" for item in known_limitations) + "</ul>"
            if known_limitations
            else "<p class=\"muted\">None.</p>"
        )
        deltas_html = (
            "<ul>" + "".join(f"<li>{_escape(str(item))}</li>" for item in expected_deltas) + "</ul>"
            if expected_deltas
            else "<p class=\"muted\">None.</p>"
        )

        pages_map = {p["page"]: p["diff_percent"] for p in doc.get("pages", []) if "page" in p}
        ta_pages = int(doc.get("typeanvil_pages", 0))
        pr_pages = int(doc.get("prince_pages", 0))
        mismatch = bool(doc.get("page_count_mismatch"))
        max_pages = max(ta_pages, pr_pages)

        page_rows = []
        if render_error:
            page_rows.append(
                f"<div class=\"render-error\"><strong>Render error:</strong> {_escape(str(render_error))}</div>"
            )
        else:
            for page_index in range(1, max_pages + 1):
                ta_exists = page_index <= ta_pages
                pr_exists = page_index <= pr_pages
                diff = pages_map.get(page_index)
                diff_label = f"{diff:.2f}%" if diff is not None else "—"

                ta_cell = _page_cell(
                    exists=ta_exists,
                    label="TypeAnvil",
                    page_index=page_index,
                    img_path=(out_dir / image_rel_root / slugify(file) / f"page-{page_index:03d}-ta.png")
                    if ta_exists
                    else None,
                )
                pr_cell = _page_cell(
                    exists=pr_exists,
                    label="Prince",
                    page_index=page_index,
                    img_path=(out_dir / image_rel_root / slugify(file) / f"page-{page_index:03d}-pr.png")
                    if pr_exists
                    else None,
                )

                page_rows.append(
                    "<div class=\"page-row\">"
                    f"{ta_cell}"
                    f"<div class=\"page-diff\"><div class=\"diff-pill\">{diff_label}</div></div>"
                    f"{pr_cell}"
                    "</div>"
                )

        pages_meta = f"TypeAnvil {ta_pages} page{'s' if ta_pages != 1 else ''} · Prince {pr_pages} page{'s' if pr_pages != 1 else ''}"
        if mismatch:
            pages_meta += " · Page count mismatch"
        sections.append(
            "<section class=\"doc\" id=\"doc-{}\">".format(slugify(file))
            + "<header>"
            + f"<h2>{_escape(name)}</h2>"
            + f"<div class=\"meta\"><code>{_escape(file)}</code> · {pages_meta}</div>"
            + ("<div class=\"badges\">" + "".join(feature_badges) + "</div>" if feature_badges else "")
            + "</header>"
            + "<div class=\"notes\">"
            + "<div><h3>Known limitations</h3>" + limitations_html + "</div>"
            + "<div><h3>Expected deltas vs Prince</h3>" + deltas_html + "</div>"
            + "</div>"
            + "<div class=\"pages\">"
            + "".join(page_rows)
            + "</div>"
            + "</section>"
        )

    table = (
        "<table class=\"scoreboard\">"
        "<thead><tr><th>Document</th><th class=\"num\">Overall diff</th><th>Bucket</th></tr></thead>"
        "<tbody>"
        + "".join(rows)
        + "</tbody></table>"
    )

    html_out = f"""<!DOCTYPE html>
<html lang=\"en\">
<head>
<meta charset=\"utf-8\" />
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />
<title>TypeAnvil vs Prince — Visual Comparison</title>
<style>
:root {{
  --bg: #f7f7f9;
  --card: #ffffff;
  --text: #1f2328;
  --muted: #5c6370;
  --border: #e2e4e8;
  --accent: #2f6feb;
  --shadow: 0 6px 24px rgba(16, 24, 40, 0.12);
}}
* {{ box-sizing: border-box; }}
body {{
  margin: 0;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
  color: var(--text);
  background: var(--bg);
  line-height: 1.5;
}}
header.page {{
  padding: 32px 24px 12px;
  max-width: 1200px;
  margin: 0 auto;
}}
header.page h1 {{ margin: 0 0 6px; font-size: 28px; }}
header.page p {{ margin: 0; color: var(--muted); }}
main {{ max-width: 1200px; margin: 0 auto; padding: 0 24px 64px; }}
.scoreboard {{
  width: 100%;
  border-collapse: collapse;
  background: var(--card);
  border: 1px solid var(--border);
  border-radius: 10px;
  overflow: hidden;
  box-shadow: var(--shadow);
}}
.scoreboard th, .scoreboard td {{ padding: 12px 14px; text-align: left; }}
.scoreboard thead {{ background: #f1f3f5; }}
.scoreboard tbody tr + tr td {{ border-top: 1px solid var(--border); }}
.scoreboard td.num, .scoreboard th.num {{ text-align: right; font-variant-numeric: tabular-nums; }}
.bucket {{ font-weight: 600; text-transform: capitalize; }}
.bucket-identical {{ color: #1a7f37; }}
.bucket-cosmetic {{ color: #9a6700; }}
.bucket-missing-feature {{ color: #c21b2f; }}
.bucket-error {{ color: #b42318; }}

.doc {{ margin-top: 32px; background: var(--card); border: 1px solid var(--border); border-radius: 14px; padding: 24px; box-shadow: var(--shadow); }}
.doc header h2 {{ margin: 0 0 4px; font-size: 22px; }}
.doc header .meta {{ color: var(--muted); margin-bottom: 10px; }}
.badges {{ display: flex; gap: 8px; flex-wrap: wrap; margin-bottom: 8px; }}
.badge {{ padding: 4px 10px; background: #eef2ff; color: #3730a3; border-radius: 999px; font-size: 12px; font-weight: 600; }}
.notes {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(240px, 1fr)); gap: 16px; margin: 18px 0 12px; }}
.notes h3 {{ margin: 0 0 6px; font-size: 14px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--muted); }}
.notes ul {{ margin: 0; padding-left: 18px; }}
.notes li {{ margin-bottom: 4px; }}
.muted {{ color: var(--muted); margin: 0; }}
.pages {{ display: flex; flex-direction: column; gap: 18px; }}
.page-row {{ display: grid; grid-template-columns: 1fr 120px 1fr; gap: 16px; align-items: start; }}
.page-cell {{ background: #fafbfc; border: 1px solid var(--border); border-radius: 10px; padding: 10px; }}
.page-label {{ font-size: 12px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--muted); margin-bottom: 8px; }}
.zoom-check {{ display: none; }}
.zoom-label {{ display: block; max-width: 100%; overflow: auto; }}
.zoom-label img {{ max-width: 100%; border-radius: 6px; box-shadow: 0 10px 20px rgba(0,0,0,0.15); cursor: zoom-in; transition: max-width 0.2s ease; }}
.zoom-check:checked + .zoom-label img {{ max-width: 300%; cursor: zoom-out; }}
.placeholder {{ display: grid; place-items: center; min-height: 160px; border: 1px dashed var(--border); border-radius: 8px; color: var(--muted); text-align: center; padding: 10px; }}
.page-diff {{ display: flex; justify-content: center; align-items: center; }}
.diff-pill {{ padding: 8px 12px; background: #111827; color: #fff; border-radius: 999px; font-variant-numeric: tabular-nums; font-size: 14px; }}
.render-error {{ padding: 12px 14px; border-radius: 10px; background: #fff1f2; border: 1px solid #fecdd3; color: #b42318; font-size: 14px; }}
.bench-heading {{ margin-top: 56px; font-size: 24px; }}
.bench-intro {{ color: var(--muted); margin: 6px 0 18px; max-width: 760px; }}
.badge-pending {{ background: #fff3e0; color: #8a5a00; }}
.badge-resolved {{ background: #e6f4ea; color: #1a7f37; }}
.bench-pill {{ padding: 8px 12px; background: #8a5a00; color: #fff; border-radius: 999px; font-size: 14px; }}

@media (max-width: 900px) {{
  .page-row {{ grid-template-columns: 1fr; }}
  .page-diff {{ order: -1; }}
}}
</style>
</head>
<body>
  <header class=\"page\">
    <h1>TypeAnvil vs Prince — Visual Comparison</h1>
    <p>Static, offline gallery. TypeAnvil renders on the left; Prince on the right.</p>
  </header>
  <main>
    {table}
    {"".join(sections)}
    {benchmark_section}
  </main>
</body>
</html>
"""

    output_path.write_text(html_out, encoding="utf-8")


def render_benchmark_section(
    *,
    benchmark_manifest: dict[str, dict],
    images_dir: Path,
) -> str:
    """The benchmark/roadmap section of the gallery (CORE-146).

    One card per open-issue fixture: status badge, the tracked issue, the
    expectation, and the TypeAnvil render (current state). These fixtures
    are NOT part of the public comparison corpus — they show known gaps on
    purpose and flip to `resolved` as their issues land.
    """
    if not benchmark_manifest:
        return ""
    import base64 as _b64

    cards: list[str] = []
    for file, meta in benchmark_manifest.items():
        name = str(meta.get("name", file))
        issue = str(meta.get("issue_id", ""))
        status = str(meta.get("status", "pending"))
        expectation = str(meta.get("expectation", ""))
        notes = meta.get("notes", []) or []
        base = Path(file).with_suffix("").name
        ta_img = images_dir / f"bench-{base}" / "page-001-ta.png"
        if ta_img.exists():
            zoom_id = f"bench-{slugify(file)}"
            data = ta_img.read_bytes()
            src = f"data:image/png;base64,{_b64.b64encode(data).decode('ascii')}"
            img_html = (
                f'<input class="zoom-check" type="checkbox" id="{zoom_id}" />'
                f'<label class="zoom-label" for="{zoom_id}">'
                f'<img src="{src}" alt="TypeAnvil render of {_escape(name)}" /></label>'
            )
        else:
            img_html = '<div class="placeholder">No render.</div>'
        notes_html = (
            "<ul>" + "".join(f"<li>{_escape(str(n))}</li>" for n in notes) + "</ul>"
            if notes
            else ""
        )
        cards.append(
            f'<section class="doc bench" id="bench-{slugify(file)}">'
            "<header>"
            f"<h2>{_escape(name)}</h2>"
            f'<div class="meta"><code>{_escape(file)}</code> · '
            f'<a href="https://linear.app/whitelodge/issue/{_escape(issue)}">{_escape(issue)}</a></div>'
            f'<div class="badges"><span class="badge badge-{_escape(status)}">{_escape(status)}</span></div>'
            "</header>"
            f'<div class="notes"><div><h3>Expectation</h3><p>{_escape(expectation)}</p>{notes_html}</div></div>'
            f'<div class="pages"><div class="page-row"><div class="page-cell">'
            f'<div class="page-label">TypeAnvil — current state</div>{img_html}</div>'
            '<div class="page-diff"><div class="bench-pill">benchmark</div></div>'
            '<div class="page-cell"><div class="page-label">Expected (engine parity)</div>'
            '<div class="placeholder">See expectation</div></div>'
            "</div></div>"
            "</section>"
        )
    if not cards:
        return ""
    return (
        '<h2 class="bench-heading">Benchmark — open engine issues</h2>'
        '<p class="bench-intro">One fixture per open issue, rendered with the '
        "current engine. These show known gaps on purpose; each flips to "
        "resolved as its issue lands. They are separate from the public "
        "comparison corpus above.</p>"
        + "".join(cards)
    )


def _page_cell(*, exists: bool, label: str, page_index: int, img_path: Path | None) -> str:
    if not exists or img_path is None:
        return (
            "<div class=\"page-cell\">"
            f"<div class=\"page-label\">{_escape(label)} — Page {page_index}</div>"
            f"<div class=\"placeholder\">No page {page_index} in {_escape(label)}.</div>"
            "</div>"
        )
    zoom_id = slugify(f"{label}-{page_index}-{img_path.parent.name}")
    # Inline the image as a data URI so the gallery renders from ANY base URL
    # (file://, WebUI /api/media preview, editor preview panes, static hosts).
    # The spec (visual-comparison-demo §Behavior 8) allows "images inlined or
    # relative"; inlining removes the whole class of broken-relative-path bugs.
    src = _inline_image_src(img_path)
    return (
        "<div class=\"page-cell\">"
        f"<div class=\"page-label\">{_escape(label)} — Page {page_index}</div>"
        f"<input class=\"zoom-check\" type=\"checkbox\" id=\"{zoom_id}\" />"
        f"<label class=\"zoom-label\" for=\"{zoom_id}\">"
        f"<img src=\"{src}\" alt=\"{_escape(label)} page {page_index}\" />"
        "</label>"
        "</div>"
    )


def _inline_image_src(img_path: Path) -> str:
    """Return a base64 data URI for the PNG, or a relative fallback if unreadable."""
    import base64 as _b64
    try:
        data = img_path.read_bytes()
    except OSError:
        return _escape(str(img_path.as_posix()))
    encoded = _b64.b64encode(data).decode("ascii")
    return f"data:image/png;base64,{encoded}"


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
    # demo/out deliverables only (§Behavior 9).
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

    validate_p = sub.add_parser("validate-scoreboard", help="Validate scoreboard schema")
    validate_p.add_argument("path")

    list_p = sub.add_parser("list-manifest", help="Emit manifest entries (file<TAB>name)")
    list_p.add_argument("--manifest", required=True)

    det_p = sub.add_parser("check-determinism", help="Compare two output dirs")
    det_p.add_argument("baseline")
    det_p.add_argument("current")

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
        render_gallery(
            scoreboard=scoreboard,
            manifest=manifest,
            output_path=out_dir / "index.html",
            benchmark_manifest=bench_manifest,
        )
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

    if args.cmd == "check-determinism":
        errors = compare_output_dirs(Path(args.baseline), Path(args.current))
        if errors:
            for err in errors:
                print(f"error: {err}", file=sys.stderr)
            return 1
        print("determinism ok")
        return 0

    return 1


if __name__ == "__main__":
    raise SystemExit(main())
