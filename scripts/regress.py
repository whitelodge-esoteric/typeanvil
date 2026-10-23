#!/usr/bin/env python3
"""Fingerprint regression gate (CORE-489).

Renders corpus documents with the engine, fingerprints every page, and
compares against the committed `baselines/<doc>.json`.

  regress.py check  [--role gate --size small] [--artifacts DIR]
  regress.py update [--role gate --size small]
  regress.py noise  [--runs 5] [--role gate --size small]

Per page: size (pt), SHA-256 of the text layer, 64-bit dHash of the page
rasterised at 72 DPI, dark-pixel count. Per document: page count and
engine version (engine/Cargo.toml).

Tolerance: page count, page size and text hash are exact. The dHash is
compared by Hamming distance <= PHASH_MAX; the dark-pixel count must
stay within DARK_REL_MAX of the baseline. Both are 0 on an unchanged
tree in one environment (the engine is deterministic), see
docs/operations/regression-gate.md for the measured noise floor.

Baselines MUST be generated in CI's Linux environment (bundled Liberation
fonts); run `scripts/dev-container.sh python3 scripts/regress.py update`
for local parity. Exit: 0 clean, 1 unexplained change, 2 usage/env error.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
import time
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO / "scripts"))

PHASH_MAX = 4          # Hamming bits of 64 allowed to differ
DARK_REL_MAX = 0.002   # 0.2 % of the baseline dark-pixel count
DARK_LEVEL = 128       # luminance below this counts as dark
RASTER_SCALE = 1.0     # 72 DPI: 1 pt = 1 px
MANIFEST = REPO / "corpus" / "manifest.json"
BASELINES = REPO / "baselines"


def slug(file: str) -> str:
    return re.sub(r"[^A-Za-z0-9]+", "-", file.rsplit(".", 1)[0]).strip("-").lower()


def engine_version() -> str:
    text = (REPO / "engine" / "Cargo.toml").read_text(encoding="utf-8")
    m = re.search(r'^version\s*=\s*"([^"]+)"', text, re.M)
    return m.group(1) if m else "unknown"


def entries(role, size):
    data = json.loads(MANIFEST.read_text(encoding="utf-8"))
    out = []
    for e in data:
        if not isinstance(e, dict) or "file" not in e:
            continue
        if role and role != "any" and role not in e.get("roles", []):
            continue
        if size and size != "all" and e.get("size") != size:
            continue
        out.append(e)
    return sorted(out, key=lambda e: e["file"])


def engine_bin() -> Path:
    p = Path(os.environ.get("ENGINE_BIN", REPO / "engine" / "target" / "debug" / "typeanvil"))
    if not p.is_absolute():
        p = REPO / p
    return p


def render(entry: dict, out_pdf: Path) -> None:
    g = entry["geometry"]
    m = g["margin"]
    src = (REPO / "corpus" / entry["file"]).resolve()
    cmd = [
        str(engine_bin()), "render", src.name,
        "--page-width", g["width"], "--page-height", g["height"],
        "--margin-top", m, "--margin-right", m, "--margin-bottom", m, "--margin-left", m,
        "-o", str(out_pdf),
    ]
    # Relative assets resolve against the CWD: render from the fixture's dir.
    r = subprocess.run(cmd, cwd=src.parent, capture_output=True, text=True)
    if r.returncode != 0:
        raise RuntimeError(f"render failed ({entry['file']}): {r.stderr.strip()[-400:]}")


def dhash(img) -> str:
    from PIL import Image
    small = img.convert("L").resize((9, 8), Image.LANCZOS)
    px = list(small.tobytes())
    bits = 0
    for row in range(8):
        for col in range(8):
            bits = (bits << 1) | (1 if px[row * 9 + col] > px[row * 9 + col + 1] else 0)
    return f"{bits:016x}"


def fingerprint(pdf: Path, png_dir: Path | None = None, tag: str = "") -> list[dict]:
    import pypdfium2 as pdfium
    doc = pdfium.PdfDocument(str(pdf))
    pages = []
    for i in range(len(doc)):
        page = doc[i]
        w, h = page.get_size()
        text = page.get_textpage().get_text_range()
        img = page.render(scale=RASTER_SCALE).to_pil().convert("L")
        dark = sum(1 for v in img.tobytes() if v < DARK_LEVEL)
        pages.append({
            "page": i + 1,
            "size": [round(w, 2), round(h, 2)],
            "text_sha256": hashlib.sha256(text.encode("utf-8")).hexdigest(),
            "dhash": dhash(img),
            "dark_pixels": dark,
        })
        if png_dir is not None:
            png_dir.mkdir(parents=True, exist_ok=True)
            img.save(png_dir / f"{tag}-page-{i + 1:03d}.png")
        page.close()
    doc.close()
    return pages


def fingerprint_doc(entry: dict, work: Path, png_dir=None, tag="") -> dict:
    pdf = work / f"{slug(entry['file'])}.pdf"
    render(entry, pdf)
    pages = fingerprint(pdf, png_dir, tag)
    return {
        "file": entry["file"],
        "engine_version": engine_version(),
        "page_count": len(pages),
        "pages": pages,
    }


def hamming(a: str, b: str) -> int:
    return bin(int(a, 16) ^ int(b, 16)).count("1")


def diff_doc(base: dict, head: dict) -> list[dict]:
    """Return a list of {page, kind, detail} for unexplained changes."""
    out = []
    if base["page_count"] != head["page_count"]:
        out.append({"page": 0, "kind": "page-count",
                    "detail": f"{base['page_count']} -> {head['page_count']}"})
    for b, h in zip(base["pages"], head["pages"]):
        n = b["page"]
        if b["size"] != h["size"]:
            out.append({"page": n, "kind": "size", "detail": f"{b['size']} -> {h['size']}"})
        if b["text_sha256"] != h["text_sha256"]:
            out.append({"page": n, "kind": "text", "detail": b["text_sha256"][:8] + " -> " + h["text_sha256"][:8]})
        d = hamming(b["dhash"], h["dhash"])
        if d > PHASH_MAX:
            out.append({"page": n, "kind": "image", "detail": f"dhash distance {d} > {PHASH_MAX}"})
        bd, hd = b["dark_pixels"], h["dark_pixels"]
        if abs(hd - bd) > DARK_REL_MAX * max(bd, 1):
            out.append({"page": n, "kind": "ink", "detail": f"dark px {bd} -> {hd}"})
    return out


def cmd_update(args) -> int:
    BASELINES.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory() as td:
        for e in entries(args.role, args.size):
            fp = fingerprint_doc(e, Path(td))
            (BASELINES / f"{slug(e['file'])}.json").write_text(
                json.dumps(fp, indent=2, sort_keys=True) + "\n", encoding="utf-8")
            print(f"updated {e['file']}: {fp['page_count']} page(s)")
    return 0


def cmd_check(args) -> int:
    rows = []
    art = Path(args.artifacts) if args.artifacts else None
    with tempfile.TemporaryDirectory() as td:
        for e in entries(args.role, args.size):
            bpath = BASELINES / f"{slug(e['file'])}.json"
            if not bpath.exists():
                if args.allow_missing:
                    # Nightly (CORE-490): still render, so a crash or panic
                    # on an unbaselined heavy/visual document turns the run red.
                    t0 = time.monotonic()
                    head = fingerprint_doc(e, Path(td))
                    print(f"unbaselined {e['file']}: {head['page_count']} page(s) "
                          f"in {time.monotonic() - t0:.1f}s")
                    continue
                rows.append((e["file"], 0, "missing-baseline", str(bpath.name)))
                continue
            base = json.loads(bpath.read_text(encoding="utf-8"))
            t0 = time.monotonic()
            head = fingerprint_doc(e, Path(td))
            print(f"timing {e['file']}: {head['page_count']} page(s) "
                  f"in {time.monotonic() - t0:.1f}s")
            changes = diff_doc(base, head)
            for c in changes:
                rows.append((e["file"], c["page"], c["kind"], c["detail"]))
            if changes and art is not None:
                # Re-render only changed docs to keep the happy path fast.
                pages = {c["page"] for c in changes if c["page"]} or set()
                d = art / slug(e["file"])
                fingerprint(Path(td) / f"{slug(e['file'])}.pdf", d, "head")
                for f in d.glob("head-page-*.png"):
                    if int(f.stem.rsplit("-", 1)[1]) not in pages and pages:
                        f.unlink()
                (d / "baseline.json").write_text(json.dumps(base, indent=2, sort_keys=True))
                (d / "head.json").write_text(json.dumps(head, indent=2, sort_keys=True))
    if not rows:
        print("regress: OK, no changes")
        return 0
    print(f"{'document':<42} {'page':>4}  {'kind':<16} detail")
    for f, p, k, d in rows:
        print(f"{f:<42} {p:>4}  {k:<16} {d}")
    print(f"regress: FAIL, {len(rows)} change(s). Run `regress.py update` in CI's "
          "environment if intended (docs/operations/regression-gate.md).")
    return 1


def cmd_noise(args) -> int:
    runs = []
    with tempfile.TemporaryDirectory() as td:
        for _ in range(args.runs):
            runs.append({e["file"]: fingerprint_doc(e, Path(td)) for e in entries(args.role, args.size)})
    worst_ham, worst_dark, text_diff = 0, 0.0, 0
    for f in runs[0]:
        for r in runs[1:]:
            for b, h in zip(runs[0][f]["pages"], r[f]["pages"]):
                worst_ham = max(worst_ham, hamming(b["dhash"], h["dhash"]))
                worst_dark = max(worst_dark, abs(h["dark_pixels"] - b["dark_pixels"]) / max(b["dark_pixels"], 1))
                text_diff += b["text_sha256"] != h["text_sha256"]
    print(f"noise floor over {args.runs} runs, {len(runs[0])} docs: "
          f"max dhash distance {worst_ham}, max dark-pixel drift {worst_dark:.5f}, text-hash diffs {text_diff}")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    for name in ("check", "update", "noise"):
        p = sub.add_parser(name)
        p.add_argument("--role", default="gate")
        p.add_argument("--size", default="small")
        if name == "check":
            p.add_argument("--artifacts", help="write changed-page PNGs + fingerprints here")
            p.add_argument("--allow-missing", action="store_true",
                           help="render documents without a baseline instead of failing")
        if name == "noise":
            p.add_argument("--runs", type=int, default=5)
    args = ap.parse_args()
    if not engine_bin().exists():
        print(f"error: engine binary not found: {engine_bin()} (set ENGINE_BIN)", file=sys.stderr)
        return 2
    return {"check": cmd_check, "update": cmd_update, "noise": cmd_noise}[args.cmd](args)


if __name__ == "__main__":
    sys.exit(main())
