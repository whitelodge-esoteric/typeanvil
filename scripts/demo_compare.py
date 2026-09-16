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
