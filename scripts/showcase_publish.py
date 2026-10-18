#!/usr/bin/env python3
"""Publish rendered showcase pages into a checkout of the typeanvil-showcase repo (CORE-494).

Spec: docs/specifications/showcase-publish.spec.md

Input : library/out/showcase/images/<doc>/page-NNN-ta.png (+ page-NNN-pr.png oracle)
        produced by scripts/build-showcase.sh, and library/manifest.json
        (entries whose `roles` include "showcase").
Output: a site tree in --site (a checkout of whitelodge-esoteric/typeanvil-showcase):

    README.md  index.md  _config.yml
    versions/<tag>/README.md
    versions/<tag>/<category>/<doc>/README.md + page-NNN-{ta,pr}.png
    latest/            copy of the newest full version (stable links)
    perf/<tag>.json   perf/README.md (history table + SVG chart)

Stdlib only; Pillow is needed solely to downscale PNGs and make thumbnails.
"""

from __future__ import annotations

import argparse
import json
import re
import shutil
import sys
from pathlib import Path

TAG_RE = re.compile(r"^v(\d{4})\.(\d{1,2})\.(\d+)$")
ENGINE_URL = "https://github.com/whitelodge-esoteric/typeanvil"
SHOWCASE_URL = "https://github.com/whitelodge-esoteric/typeanvil-showcase"
PAGES_URL = "https://whitelodge-esoteric.github.io/typeanvil-showcase/"
THUMB_WIDTH = 240
PAGE_RE = re.compile(r"^page-(\d{3})-(ta|pr)\.png$")


def parse_tag(tag: str) -> tuple[int, int, int]:
    m = TAG_RE.match(tag)
    if not m:
        raise ValueError(f"not a CalVer tag (expected vYYYY.M.PATCH): {tag!r}")
    return int(m.group(1)), int(m.group(2)), int(m.group(3))


def sort_tags(tags: list[str]) -> list[str]:
    """Newest first, numeric (v2026.10.0 sorts after v2026.9.0)."""
    return sorted(tags, key=parse_tag, reverse=True)


def split_retention(tags: list[str], keep: int) -> tuple[list[str], list[str]]:
    """Return (full, thumbnail_only). `keep` newest stay in full; keep >= 1."""
    if keep < 1:
        raise ValueError("keep must be >= 1")
    ordered = sort_tags(tags)
    return ordered[:keep], ordered[keep:]


def scale_for(source_dpi: int, target_dpi: int) -> float:
    if target_dpi <= 0 or source_dpi <= 0:
        raise ValueError("dpi must be positive")
    return min(1.0, target_dpi / source_dpi)


def list_pages(doc_images: Path) -> dict[int, dict[str, Path]]:
    pages: dict[int, dict[str, Path]] = {}
    for p in sorted(doc_images.glob("page-*.png")):
        m = PAGE_RE.match(p.name)
        if m:
            pages.setdefault(int(m.group(1)), {})[m.group(2)] = p
    return pages


def render_doc_readme(name: str, tag: str, pages: dict[int, dict[str, str]]) -> str:
    """pages: page number -> {'ta': filename, 'pr': filename (optional)}."""
    has_oracle = any("pr" in v for v in pages.values())
    lines = [f"# {name}", "", f"TypeAnvil `{tag}`"
             + (", beside the Prince reference render (unscored)." if has_oracle else "."), ""]
    if has_oracle:
        lines += ["| Page | TypeAnvil | Prince |", "|---|---|---|"]
        for n in sorted(pages):
            ta = f"![TypeAnvil page {n}]({pages[n]['ta']})" if "ta" in pages[n] else "missing"
            pr = f"![Prince page {n}]({pages[n]['pr']})" if "pr" in pages[n] else "not rendered"
            lines.append(f"| {n} | {ta} | {pr} |")
    else:
        lines += ["| Page | TypeAnvil |", "|---|---|"]
        for n in sorted(pages):
            lines.append(f"| {n} | ![TypeAnvil page {n}]({pages[n]['ta']}) |")
    lines.append("")
    return "\n".join(lines)


def render_version_readme(tag: str, docs: list[dict]) -> str:
    lines = [f"# TypeAnvil {tag}", "",
             f"Rendered showcase documents for [{tag}]({ENGINE_URL}/releases/tag/{tag}).", "",
             "| Document | Pages |", "|---|---|"]
    for d in docs:
        lines.append(f"| [{d['name']}]({d['stem']}/README.md) | {d['pages']} |")
    lines += ["", f"Back to the [index]({'../../README.md'}).", ""]
    return "\n".join(lines)


def render_thumb_readme(tag: str, docs: list[dict]) -> str:
    lines = [f"# TypeAnvil {tag} (thumbnails)", "",
             "This version is past the retention window. Only thumbnails are kept.", ""]
    for d in docs:
        lines.append(f"## {d['name']}")
        lines.append("")
        lines.append(" ".join(f"![page {n}]({d['stem']}/{f})" for n, f in d["thumbs"]))
        lines.append("")
    return "\n".join(lines)


def render_root_readme(full: list[str], thumbs: list[str], keep: int) -> str:
    lines = [
        "# TypeAnvil showcase", "",
        f"Rendered output from [TypeAnvil]({ENGINE_URL}), a pure-Rust HTML to PDF engine for "
        "paged media: `@page`, fragmentation, running headers, footnotes, tables, and tagged PDF.",
        "",
        f"Site: {PAGES_URL}",
        "",
        "Every release tag publishes its showcase documents here, rendered by the release "
        "binary at 150 DPI, next to a Prince reference render.",
        "",
        "- [Latest](latest/README.md) is a stable link to the newest version.",
        "- [Performance history](perf/README.md)",
        "",
        "## Versions", "",
    ]
    for t in full:
        lines.append(f"- [{t}](versions/{t}/README.md)")
    for t in thumbs:
        lines.append(f"- [{t}](versions/{t}/README.md) (thumbnails only)")
    lines += ["", f"The newest {keep} versions keep full-size pages. Older versions keep "
              "thumbnails only.", "",
              "This repository is generated by the engine's `showcase.yml` workflow. "
              "Do not edit it by hand.", ""]
    return "\n".join(lines)


def perf_chart_svg(history: list[dict]) -> str:
    """Bar chart of total pages per version, oldest to newest, plain SVG."""
    w, h, pad, bar = 640, 220, 30, 28
    data = [(r["tag"], int(r.get("total_pages", 0))) for r in history]
    top = max([v for _, v in data] + [1])
    gap = max(8, (w - 2 * pad - bar * max(len(data), 1)) // max(len(data), 1))
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" '
           f'viewBox="0 0 {w} {h}" font-family="sans-serif" font-size="10">',
           f'<text x="{pad}" y="14">Total showcase pages per release</text>']
    for i, (tag, v) in enumerate(data):
        x = pad + i * (bar + gap)
        bh = int((h - 2 * pad - 10) * v / top)
        y = h - pad - bh
        out.append(f'<rect x="{x}" y="{y}" width="{bar}" height="{bh}" fill="#3b6ea5"/>')
        out.append(f'<text x="{x}" y="{y - 3}">{v}</text>')
        out.append(f'<text x="{x}" y="{h - pad + 12}">{tag}</text>')
    out.append("</svg>")
    return "\n".join(out) + "\n"


def render_perf_readme(history: list[dict]) -> str:
    lines = ["# Performance history", "", "![chart](chart.svg)", "",
             "| Version | Documents | Pages | Render seconds | Commit |", "|---|---|---|---|---|"]
    for r in history:
        secs = r.get("render_seconds")
        lines.append(f"| {r['tag']} | {len(r.get('docs', {}))} | {r.get('total_pages', 0)} | "
                     f"{'n/a' if secs is None else secs} | `{r.get('commit', '')[:10]}` |")
    lines.append("")
    return "\n".join(lines)


def load_history(perf_dir: Path) -> list[dict]:
    rows = []
    for f in perf_dir.glob("v*.json"):
        try:
            rows.append(json.loads(f.read_text()))
        except json.JSONDecodeError:
            continue
    rows = [r for r in rows if TAG_RE.match(str(r.get("tag", "")))]
    return sorted(rows, key=lambda r: parse_tag(r["tag"]))


def _copy_scaled(src: Path, dst: Path, scale: float, thumb: bool = False) -> None:
    dst.parent.mkdir(parents=True, exist_ok=True)
    if scale >= 1.0 and not thumb:
        shutil.copyfile(src, dst)
        return
    from PIL import Image  # noqa: PLC0415 — only needed when resizing

    with Image.open(src) as im:
        if thumb:
            ratio = THUMB_WIDTH / im.width
            size = (THUMB_WIDTH, max(1, round(im.height * ratio)))
        else:
            size = (max(1, round(im.width * scale)), max(1, round(im.height * scale)))
        im.convert("RGB").resize(size, Image.Resampling.LANCZOS).save(dst, "PNG", optimize=True)


def _write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def add_version(site: Path, images: Path, manifest: list[dict], tag: str, commit: str,
                source_dpi: int, target_dpi: int, render_seconds: float | None) -> dict:
    """Write versions/<tag>/ for a new release. Returns the perf record."""
    parse_tag(tag)
    scale = scale_for(source_dpi, target_dpi)
    vdir = site / "versions" / tag
    if vdir.exists():
        shutil.rmtree(vdir)  # re-run of the same tag replaces its own output only
    docs: list[dict] = []
    perf_docs: dict[str, dict] = {}
    for entry in manifest:
        if "showcase" not in entry.get("roles", ["showcase"]):
            continue
        stem = Path(entry["file"]).stem
        category = entry.get("category") or Path(entry["file"]).parent.name or "misc"
        rel = f"{category}/{stem}"
        pages = list_pages(images / stem)
        if not any("ta" in v for v in pages.values()):
            raise SystemExit(f"error: no TypeAnvil pages for {stem} in {images}")
        names: dict[int, dict[str, str]] = {}
        for n, kinds in pages.items():
            names[n] = {}
            for kind, src in kinds.items():
                _copy_scaled(src, vdir / rel / src.name, scale)
                names[n][kind] = src.name
        _write(vdir / rel / "README.md", render_doc_readme(entry["name"], tag, names))
        docs.append({"stem": rel, "name": entry["name"], "pages": len(pages)})
        perf_docs[rel] = {"pages": len(pages)}
    _write(vdir / "README.md", render_version_readme(tag, docs))
    record = {"tag": tag, "commit": commit, "dpi": min(target_dpi, source_dpi),
              "docs": perf_docs, "total_pages": sum(d["pages"] for d in docs),
              "render_seconds": render_seconds}
    _write(site / "perf" / f"{tag}.json", json.dumps(record, indent=2, sort_keys=True) + "\n")
    return record


def thumbnail_version(site: Path, tag: str) -> None:
    """Replace a full version by thumbnails (retention)."""
    vdir = site / "versions" / tag
    if not vdir.is_dir() or (vdir / ".thumbs").exists():
        return
    docs = []
    for sub in sorted(p.parent for p in vdir.glob("*/*/README.md")):
        thumbs = []
        for png in sorted(sub.glob("page-*-ta.png")):
            _copy_scaled(png, png, 1.0, thumb=True)
            thumbs.append((int(PAGE_RE.match(png.name).group(1)), png.name))
        for png in sub.glob("page-*-pr.png"):
            png.unlink()
        (sub / "README.md").unlink(missing_ok=True)
        docs.append({"stem": f"{sub.parent.name}/{sub.name}", "name": sub.name, "thumbs": thumbs})
    _write(vdir / "README.md", render_thumb_readme(tag, docs))
    _write(vdir / ".thumbs", "")


def refresh_site(site: Path, keep: int) -> list[str]:
    tags = [p.name for p in (site / "versions").glob("v*") if TAG_RE.match(p.name)]
    full, thumbs = split_retention(tags, keep)
    for t in thumbs:
        thumbnail_version(site, t)
    # latest/ = copy of the newest version, so links stay stable (no symlink on Pages)
    latest = site / "latest"
    if latest.exists():
        shutil.rmtree(latest)
    if full:
        shutil.copytree(site / "versions" / full[0], latest)
    history = load_history(site / "perf")
    _write(site / "perf" / "chart.svg", perf_chart_svg(history))
    _write(site / "perf" / "README.md", render_perf_readme(history))
    root = render_root_readme(full, thumbs, keep)
    _write(site / "README.md", root)
    _write(site / "index.md", root)
    _write(site / "_config.yml", "theme: jekyll-theme-minimal\ntitle: TypeAnvil showcase\n")
    return full


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--site", required=True, type=Path, help="checkout of the showcase repo")
    ap.add_argument("--images", type=Path, default=Path("library/out/showcase/images"))
    ap.add_argument("--manifest", type=Path, default=Path("library/manifest.json"))
    ap.add_argument("--tag", required=True, help="release tag, vYYYY.M.PATCH")
    ap.add_argument("--commit", default="unknown")
    ap.add_argument("--source-dpi", type=int, default=300)
    ap.add_argument("--dpi", type=int, default=150, help="published DPI (150-200)")
    ap.add_argument("--keep", type=int, default=5, help="versions kept at full size")
    ap.add_argument("--render-seconds", type=float, default=None)
    args = ap.parse_args(argv)

    if not 150 <= args.dpi <= 200:
        print("error: --dpi must be within 150-200", file=sys.stderr)
        return 2
    try:
        parse_tag(args.tag)
    except ValueError as e:
        print(f"error: {e}", file=sys.stderr)
        return 2
    manifest = json.loads(args.manifest.read_text())
    add_version(args.site, args.images, manifest, args.tag, args.commit,
                args.source_dpi, args.dpi, args.render_seconds)
    full = refresh_site(args.site, args.keep)
    print(f"published {args.tag}; full versions: {', '.join(full)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
