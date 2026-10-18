#!/usr/bin/env python3
"""PR visual diffs (CORE-498).

Renders the gate documents with a base engine and a head engine, finds the
pages that changed, and builds one three-panel PNG per changed page
(base | head | highlighted diff, about 100 DPI). Also builds the PR-description
section and swaps it into a PR body between marker comments.

  pr_visual_diff.py render --base-bin B --head-bin H [--base-root DIR] \
        --out DIR --pr N --sha SHA [--role gate --size small] [--max-images 20]
  pr_visual_diff.py body   --manifest DIR/manifest.json --body-in F --body-out F
  pr_visual_diff.py prune  --root CHECKOUT --days 30

Spec: docs/specifications/pr-visual-diff.spec.md. Runbook:
docs/operations/pr-visual-diff.md.
"""
from __future__ import annotations

import argparse
import importlib.util
import json
import subprocess
import sys
import tempfile
import time
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

_spec = importlib.util.spec_from_file_location("regress", REPO / "scripts" / "regress.py")
regress = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(regress)

DPI = 100
SCALE = DPI / 72
MAX_IMAGES = 20
DIFF_LEVEL = 24          # per-channel difference that counts as changed
ASSET_REPO = "whitelodge-esoteric/typeanvil-showcase"
ASSET_BRANCH = "pr-assets"
MARK_BEGIN = "<!-- typeanvil-visual-diff:begin -->"
MARK_END = "<!-- typeanvil-visual-diff:end -->"


def raw_base_url(pr: int, sha: str) -> str:
    return (f"https://raw.githubusercontent.com/{ASSET_REPO}/{ASSET_BRANCH}/"
            f"pr-{pr}/{sha[:8]}")


def render_pdf(bin_path: Path, root: Path, entry: dict, out_pdf: Path) -> bool:
    """Render one library entry from `root`. False when the file is absent."""
    src = (root / "library" / entry["file"]).resolve()
    if not src.exists():
        return False
    g = entry["geometry"]
    m = g["margin"]
    cmd = [str(bin_path), "render", src.name,
           "--page-width", g["width"], "--page-height", g["height"],
           "--margin-top", m, "--margin-right", m, "--margin-bottom", m, "--margin-left", m,
           "-o", str(out_pdf)]
    r = subprocess.run(cmd, cwd=src.parent, capture_output=True, text=True)
    if r.returncode != 0:
        raise RuntimeError(f"render failed ({entry['file']}): {r.stderr.strip()[-300:]}")
    return True


def page_images(pdf: Path):
    import pypdfium2 as pdfium
    doc = pdfium.PdfDocument(str(pdf))
    out = []
    for i in range(len(doc)):
        page = doc[i]
        out.append(page.render(scale=SCALE).to_pil().convert("RGB"))
        page.close()
    doc.close()
    return out


def diff_image(base, head):
    """Head page dimmed, with pixels that differ from base painted red."""
    from PIL import Image, ImageChops
    w, h = max(base.width, head.width), max(base.height, head.height)
    a = Image.new("RGB", (w, h), "white")
    b = Image.new("RGB", (w, h), "white")
    a.paste(base, (0, 0))
    b.paste(head, (0, 0))
    mask = ImageChops.difference(a, b).convert("L").point(lambda v: 255 if v > DIFF_LEVEL else 0)
    dim = Image.blend(b, Image.new("RGB", (w, h), "white"), 0.7)
    dim.paste(Image.new("RGB", (w, h), (230, 20, 20)), (0, 0), mask)
    return dim, sum(1 for v in mask.tobytes() if v)


def three_panel(base, head):
    """base | head | diff on one canvas. A missing side is blank white.

    Returns (image, changed_pixel_count)."""
    from PIL import Image, ImageDraw
    ref = base or head
    blank = Image.new("RGB", ref.size, "white")
    a, b = base or blank, head or blank
    d, changed = diff_image(a, b)
    w, h = d.width, d.height
    gap, bar = 8, 18
    canvas = Image.new("RGB", (3 * w + 4 * gap, h + bar + 2 * gap), (200, 200, 200))
    dr = ImageDraw.Draw(canvas)
    for i, (label, img) in enumerate((("base" if base else "base (page absent)", a),
                                      ("head" if head else "head (page absent)", b),
                                      ("diff", d))):
        x = gap + i * (w + gap)
        dr.text((x, 3), label, fill=(0, 0, 0))
        canvas.paste(img, (x, bar + gap))
    return canvas, changed


def changed_pages(base_imgs, head_imgs, base_fp, head_fp) -> list[int]:
    """1-based page numbers that differ. Fingerprint diff plus an exact
    pixel check, so a sub-tolerance move still shows when pixels differ."""
    pages = set()
    for c in regress.diff_doc(base_fp, head_fp):
        if c["page"]:
            pages.add(c["page"])
    n = max(len(base_imgs), len(head_imgs))
    if len(base_imgs) != len(head_imgs):
        pages.update(range(min(len(base_imgs), len(head_imgs)) + 1, n + 1))
    return sorted(p for p in pages if p <= n)


def cmd_render(args) -> int:
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    base_root = Path(args.base_root) if args.base_root else REPO
    base_bin, head_bin = Path(args.base_bin), Path(args.head_bin)
    items, total = [], 0
    with tempfile.TemporaryDirectory() as td:
        td = Path(td)
        for e in regress.entries(args.role, args.size):
            total += 1
            slug = regress.slug(e["file"])
            hp, bp = td / f"{slug}-head.pdf", td / f"{slug}-base.pdf"
            render_pdf(head_bin, REPO, e, hp)
            has_base = render_pdf(base_bin, base_root, e, bp)
            head_imgs = page_images(hp)
            base_imgs = page_images(bp) if has_base else []
            head_fp = {"page_count": len(head_imgs), "pages": regress.fingerprint(hp)}
            base_fp = ({"page_count": len(base_imgs), "pages": regress.fingerprint(bp)}
                       if has_base else {"page_count": 0, "pages": []})
            for p in changed_pages(base_imgs, head_imgs, base_fp, head_fp):
                b = base_imgs[p - 1] if p <= len(base_imgs) else None
                h = head_imgs[p - 1] if p <= len(head_imgs) else None
                img, px = three_panel(b, h)
                if px == 0 and b is not None and h is not None:
                    continue   # fingerprint noise with identical pixels
                name = f"{slug}-p{p:03d}.png"
                items.append({"file": e["file"], "page": p, "image": name, "changed_px": px,
                              "_img": img})
    items.sort(key=lambda i: (i["file"], i["page"]))
    shown = items[:args.max_images]
    for it in shown:
        it.pop("_img").save(out / it["image"], optimize=True)
    for it in items[args.max_images:]:
        it.pop("_img")
    manifest = {
        "pr": args.pr, "sha": args.sha, "documents": total,
        "changed_pages": len(items), "shown": len(shown),
        "more": len(items) - len(shown),
        "raw_base": raw_base_url(args.pr, args.sha),
        "items": [{k: v for k, v in i.items()} for i in items],
    }
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(f"visual-diff: {total} documents, {len(items)} changed page(s), {len(shown)} image(s)")
    return 0


def build_section(manifest: dict, run_url: str = "") -> str:
    """Markdown for the PR description, marker comments included."""
    lines = [MARK_BEGIN, "## Visual diff", ""]
    n = manifest["changed_pages"]
    if n == 0:
        lines.append(f"No visual change in {manifest['documents']} documents.")
    else:
        docs = len({i["file"] for i in manifest["items"]})
        lines.append(f"{n} changed page(s) in {docs} of {manifest['documents']} documents "
                     f"(base | head | diff).")
        lines.append("")
        base = manifest["raw_base"]
        for it in manifest["items"][:manifest["shown"]]:
            lines.append(f"**{it['file']}**, page {it['page']}")
            lines.append("")
            lines.append(f"![{it['file']} p{it['page']}]({base}/{it['image']})")
            lines.append("")
        if manifest["more"]:
            more = f"{manifest['more']} more changed page(s) not shown"
            more += f" ([run artifacts]({run_url})).\n" if run_url else ".\n"
            lines.append(more)
    lines.append(MARK_END)
    return "\n".join(lines)


def replace_section(body: str, section: str) -> str:
    """Swap the marked section in place; append it when absent."""
    body = body or ""
    s, e = body.find(MARK_BEGIN), body.find(MARK_END)
    if s != -1 and e != -1 and e > s:
        return body[:s] + section + body[e + len(MARK_END):]
    sep = "\n\n" if body.strip() else ""
    return body.rstrip() + sep + section + "\n"


def cmd_body(args) -> int:
    manifest = json.loads(Path(args.manifest).read_text(encoding="utf-8"))
    body = Path(args.body_in).read_text(encoding="utf-8") if Path(args.body_in).exists() else ""
    Path(args.body_out).write_text(
        replace_section(body, build_section(manifest, args.run_url)), encoding="utf-8")
    return 0


def cmd_prune(args) -> int:
    root, cutoff, removed = Path(args.root), time.time() - args.days * 86400, 0
    for d in sorted(root.glob("pr-*")):
        r = subprocess.run(["git", "-C", str(root), "log", "-1", "--format=%ct", "--", d.name],
                           capture_output=True, text=True)
        ts = int(r.stdout.strip() or 0)
        if d.is_dir() and ts and ts < cutoff:
            subprocess.run(["git", "-C", str(root), "rm", "-rq", d.name], check=True)
            removed += 1
    print(f"prune: removed {removed} directory(ies) older than {args.days} days")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("render")
    r.add_argument("--base-bin", required=True)
    r.add_argument("--head-bin", required=True)
    r.add_argument("--base-root", help="checkout of the base commit (its library/ is used)")
    r.add_argument("--out", required=True)
    r.add_argument("--pr", type=int, required=True)
    r.add_argument("--sha", required=True)
    r.add_argument("--role", default="gate")
    r.add_argument("--size", default="small")
    r.add_argument("--max-images", type=int, default=MAX_IMAGES)
    b = sub.add_parser("body")
    b.add_argument("--manifest", required=True)
    b.add_argument("--body-in", required=True)
    b.add_argument("--body-out", required=True)
    b.add_argument("--run-url", default="")
    p = sub.add_parser("prune")
    p.add_argument("--root", required=True)
    p.add_argument("--days", type=int, default=30)
    args = ap.parse_args()
    return {"render": cmd_render, "body": cmd_body, "prune": cmd_prune}[args.cmd](args)


if __name__ == "__main__":
    sys.exit(main())
