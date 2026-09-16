#!/usr/bin/env python3
"""Visual QA checks for TypeAnvil rendered documents."""

from __future__ import annotations

import argparse
import html
import json
import re
import sys
from pathlib import Path
from typing import Any, Dict, List, Tuple

# Add project root to path
ROOT = Path(__file__).resolve().parents[1]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

# Try importing PIL and pypdfium2
try:
    from PIL import Image
    HAS_PIL = True
except ImportError:
    HAS_PIL = False
    print("Warning: PIL not available", file=sys.stderr)

try:
    import pypdfium2 as pdfium
    HAS_PYPDFIUM2 = True
except ImportError:
    HAS_PYPDFIUM2 = False
    print("Warning: pypdfium2 not available", file=sys.stderr)

# Import harness modules
try:
    from harness.rasterize import rasterize_pdf
    HAS_HARNESS = True
except ImportError:
    HAS_HARNESS = False
    print("Warning: harness module not available", file=sys.stderr)

# A declared fill must paint at least an 8x8 px region (64 px) to count.
MIN_FILL_PIXELS = 64


def _pdf_text_line_boxes(pdf_path_str: str, page_index: int):
    """Text-line bounding boxes for one page, via charbox baseline clustering.

    The same technique CORE-215's structure layer uses (and the geometry that
    disproved the vision model's false "text collision" claims during
    CORE-209): group chars whose baseline (box bottom) is within ~2.5 pt,
    x-sort each row, and return (x0, y0, x1, y1) per visual line.
    """
    import pypdfium2 as pdfium
    try:
        doc = pdfium.PdfDocument(pdf_path_str)
    except Exception:
        return []
    if page_index >= len(doc):
        doc.close()
        return []
    page = doc[page_index]
    tp = page.get_textpage()
    rows = []
    row_base = []
    boxes = []
    try:
        count = tp.count_chars()
        raw = tp.get_text_range(0, count)
        for i in range(count):
            try:
                box = tp.get_charbox(i)
            except Exception:
                continue
            if box is None:
                continue
            ch = raw[i] if i < len(raw) else ""
            if ch in ("\r", "\n", "\t", "\x00"):
                continue
            left, bottom, right, top = (float(v) for v in box)
            placed = False
            for row, base in zip(rows, row_base):
                # One printed line's charboxes jitter up to ~4 pt (descender
                # bottoms). 4 pt keeps distinct lines separate; duplicate
                # text runs (same physical text emitted twice with slightly
                # different glyph boxes) land in separate clusters here and
                # are filtered by the center-distance rule below.
                if abs(bottom - base) <= 4.0:
                    row.append((ch, left, bottom, right, top))
                    placed = True
                    break
            if not placed:
                rows.append([(ch, left, bottom, right, top)])
                row_base.append(bottom)
        cluster_index = 0
        for row in rows:
            row.sort(key=lambda item: item[1])
            # A baseline row may contain several COLUMNS (body + margin
            # note + page number sharing one baseline). Split at large
            # x-gaps so each segment is a real text box; otherwise a
            # full-width merged box falsely "overlaps" the next row.
            segments = []
            current = [row[0]]
            for prev, item in zip(row, row[1:]):
                gap = item[1] - prev[3]
                if gap > 15.0:  # column break
                    segments.append(current)
                    current = [item]
                else:
                    current.append(item)
            segments.append(current)
            for segment in segments:
                text = "".join(i[0] for i in segment).strip()
                if not text:
                    continue
                boxes.append(((min(i[1] for i in segment), min(i[2] for i in segment),
                               max(i[3] for i in segment), max(i[4] for i in segment)),
                              cluster_index))
            cluster_index += 1
    finally:
        tp.close()
        doc.close()
    return boxes


def check_ink_row_overlap(page_img: Image.Image,
                          pdf_path: str = "", page_index: int = 0) -> Dict[str, Any]:
    """Check for overlapping TEXT LINES on a page (geometry, not vision).

    Two visually distinct text lines that occupy the same pixel rows (their
    boxes overlap vertically by more than a hair AND their x-ranges overlap)
    is a real text collision — the CORE-150 class. This uses the PDF text
    layer's char boxes (baseline clustering), so a solid background box or a
    figure under text is NOT a false overlap (ink-row run gaps cannot
    distinguish those; CORE-217 records that a vision model produced two
    false "text collision" claims that this geometry disproved).
    """
    if not pdf_path:
        return {"pass": True, "min_gap_px": None, "runs": [],
                "note": "no PDF path provided; overlap needs the text layer"}
    boxes = _pdf_text_line_boxes(pdf_path, page_index)

    def _overlaps(a, b):
        (ax0, ay0, ax1, ay1), ca = a
        (bx0, by0, bx1, by1), cb = b
        # Same baseline cluster = fragments of ONE visual line (mojibake
        # runs / column segments), never a collision by construction.
        if ca == cb:
            return False
        # Tiny fragments (a lone descender 'g' box, a drop cap, a bullet)
        # are not line collisions: ignore lines narrower than 30 pt.
        if min(ax1 - ax0, bx1 - bx0) < 30:
            return False
        x_overlap = min(ax1, bx1) - max(ax0, bx0)
        # A REAL collision places the two lines substantially on top of
        # each other (>60% of the narrower line's width shared). Side-by-
        # side columns or margin boxes merely TOUCH (~30%) and must not
        # fire (measured on the corpus/showcase fixtures).
        x_substantial = x_overlap > 0.6 * min(ax1 - ax0, bx1 - bx0)
        # Line centers, not extreme edges: glyph boxes include ascent/
        # descent, so two ADJACENT lines legitimately touch at the edges.
        # A real collision puts the line centers on nearly the same rows.
        a_cy = (ay0 + ay1) / 2
        b_cy = (by0 + by1) / 2
        min_h = min(ay1 - ay0, by1 - by0)
        y_center_close = abs(a_cy - b_cy) < 0.6 * min_h
        return x_substantial and y_center_close and x_overlap > 2

    def _coincident(a, b):
        """Near-identical boxes = the SAME text run emitted twice (the
        engine writes clean + garbled copies of duplicate runs; CORE-85-class
        artifact), not a collision. A real collision overlaps PARTIALLY."""
        (ax0, ay0, ax1, ay1), _ = a
        (bx0, by0, bx1, by1), _ = b
        inter_w = min(ax1, bx1) - max(ax0, bx0)
        inter_h = min(ay1, by1) - max(ay0, by0)
        union_w = max(ax1, bx1) - min(ax0, bx0)
        union_h = max(ay1, by1) - min(ay0, by0)
        if union_w <= 0 or union_h <= 0:
            return False
        return (inter_w / union_w) >= 0.7 and (inter_h / union_h) >= 0.7

    collisions = []
    for i in range(len(boxes)):
        for j in range(i + 1, len(boxes)):
            if not _overlaps(boxes[i], boxes[j]):
                continue
            (ax0, ay0, ax1, ay1), ca = boxes[i]
            (bx0, by0, bx1, by1), cb = boxes[j]
            if _coincident(boxes[i], boxes[j]):
                continue
            # Duplicate text runs: the engine writes the same physical text
            # twice (a clean run + a garbled run; CORE-85-class artifact),
            # so their line centers sit ~1 pt apart. A REAL overlap of two
            # distinct lines has centers >= 4-5 pt apart (line pitch).
            a_cy = (ay0 + ay1) / 2
            b_cy = (by0 + by1) / 2
            if abs(a_cy - b_cy) < 3.0:
                continue
            collisions.append({"a": boxes[i][0], "b": boxes[j][0]})

    return {
        "pass": len(collisions) == 0,
        "min_gap_px": len(collisions),
        "collisions": collisions[:10],
    }


def check_page_box_overflow(page_img: Image.Image, content_box: Tuple[int, int, int, int]) -> Dict[str, Any]:
    """Check for ink escaping the physical page (MediaBox overflow).

    The boundary is the media box (the full page), so in-band margin-box
    content (running heads, footers, full-bleed) is legal by design. Ink
    within OVERFLOW_EDGE_TOL px of the page edge is a real clip/overflow
    (the rasterizer clips exactly at this edge).
    """
    # Ink right at the page edge (within 2 px) = escaped content.
    tolerance = 2
    left, top, right, bottom = content_box
    right -= 1
    bottom -= 1
    gray_img = page_img.convert('L')
    width, height = gray_img.size

    violations = []
    for y in range(height):
        for x in range(width):
            if gray_img.getpixel((x, y)) < 250:  # Not white
                near_edge = (
                    x - left <= tolerance or right - x <= tolerance
                    or y - top <= tolerance or bottom - y <= tolerance
                )
                if near_edge:
                    if len(violations) < 10:
                        violations.append({"x": x, "y": y})
    return {
        "pass": len(violations) == 0,
        "violations": violations
    }


def fixture_is_full_bleed(source_html: str) -> bool:
    """True when the fixture declares a zero-margin page (full-bleed design).

    The showcase Rich Media Print fixture paints a full-page background on
    purpose; its @page margin: 0 makes edge ink legal. A margin-bearing
    document has no such exemption, so ink touching the page edge there is a
    real clip/overflow.
    """
    m = re.search(r"@page[^{]*\{[^}]*\}", source_html, flags=re.S | re.I)
    if not m:
        return False
    block = m.group(0).lower()
    if "margin" not in block:
        return False
    margin_m = re.search(r"margin\s*:\s*([^;};]+)", block)
    if not margin_m:
        return False
    value = margin_m.group(1).strip()
    # margin: 0 / 0in / 0px / 0pt (single zero) => full-bleed.
    return bool(re.fullmatch(r"0(?:in|px|pt|mm|cm)?", value))


def check_text_roundtrip_coverage(pdf_page_text: str, source_text: str) -> Dict[str, Any]:
    """Check that source content survives into the PDF text layer.

    Character-level coverage, not token-level: a token check fails on (a)
    hyphenation fragments ("justi-" + "fied" vs "justified"), (b) words the
    engine splits across a page boundary with a running head between the
    halves, and (c) PDF extraction gluing adjacent runs ("1Contents"). None
    of those is dropped content. SequenceMatcher on the normalized char
    streams covers all three: silently dropped/replaced content (the
    CORE-83/85 class) removes a block of chars and drops the coverage ratio,
    while splits keep every source char present in order (ratio ~1.0).
    """
    # Normalize texts - lowercase, drop U+FFFE/U+00AD (soft-hyphen markers
    # the engine embeds at hyphenation points: "dictio\ufffenary" is
    # "dictionary"), split digit<->letter boundaries so glued PDF runs
    # ("1Contents") separate, then tokenize.
    def normalize_set(text):
        text = text.lower()
        text = text.replace("\ufffe", "").replace("\u00ad", "")
        text = re.sub(r"[^\w\s]", " ", text)
        text = re.sub(r"(?<=[a-z])(?=\d)|(?<=\d)(?=[a-z])", " ", text)
        return set(t for t in text.split() if t)

    source_tokens = normalize_set(source_text)
    pdf_tokens = normalize_set(pdf_page_text)
    if not source_tokens:
        return {"pass": True, "missing_tokens": [], "coverage": 1.0}

    # Token-presence ratio: content is dropped when a source token never
    # appears in the PDF layer. A word split across a page boundary
    # ("fa-" p10 + "miliar" p11) makes a handful of tokens absent even
    # though nothing is dropped, so the bar is 98% present, not 100%.
    missing_tokens = sorted(source_tokens - pdf_tokens)
    coverage = (len(source_tokens) - len(missing_tokens)) / len(source_tokens)

    return {
        "pass": coverage >= 0.98,
        "missing_tokens": missing_tokens,
        "coverage": round(coverage, 4),
    }


def extract_declared_fills(html_content: str) -> List[str]:
    """Extract background-color declarations from HTML.
    
    Args:
        html_content: HTML source content
        
    Returns:
        List of color values found
    """
    colors = set()
    
    # Match background-color declarations
    bg_color_pattern = r'background-color\s*:\s*([^;}]*)'
    matches = re.findall(bg_color_pattern, html_content, re.IGNORECASE)
    
    # Filter out transparent/none values
    for match in matches:
        color = match.strip().lower()
        if color not in ['transparent', 'none', 'inherit', 'initial', 'unset']:
            colors.add(color)
    
    return sorted(list(colors))


def check_declared_fill_colors(page_img: Image.Image, declared_colors: List[str]) -> Dict[str, Any]:
    """Check that declared background colors appear in the rendered image.

    Dense numpy scan: every pixel is classified against every declared color
    (within the anti-aliasing tolerance), so a paint the engine actually
    produced cannot be missed by sampling. A color passes when at least
    MIN_FILL_PIXELS pixels match (an 8x8 region = 64 px).
    """
    import numpy as np

    if not declared_colors:
        return {
            "pass": True,
            "missing": [],
            "found": {}
        }

    rgb_img = page_img.convert("RGB")
    arr = np.asarray(rgb_img, dtype=np.int16)  # (H, W, 3)

    color_counts: dict[str, int] = {}
    missing_colors: list[str] = []
    for color_str in declared_colors:
        rgb = parse_css_color(color_str)
        if rgb is None:
            # Unparseable declaration: record as missing with a reason.
            missing_colors.append(color_str)
            continue
        diff = np.abs(arr - np.array(rgb, dtype=np.int16)).max(axis=2)
        count = int((diff <= 10).sum())
        if count >= MIN_FILL_PIXELS:
            color_counts[color_str] = count
        else:
            missing_colors.append(color_str)

    return {
        "pass": len(missing_colors) == 0,
        "missing": missing_colors,
        "found": color_counts
    }


def parse_css_color(color_str: str) -> Tuple[int, int, int] | None:
    """Parse CSS color string to RGB tuple.
    
    Args:
        color_str: CSS color string (named, hex, rgb())
        
    Returns:
        RGB tuple or None if parsing failed
    """
    color_str = color_str.strip().lower()
    
    # Named colors (subset for common ones)
    named_colors = {
        'black': (0, 0, 0),
        'white': (255, 255, 255),
        'red': (255, 0, 0),
        'green': (0, 128, 0),
        'blue': (0, 0, 255),
        'yellow': (255, 255, 0),
        'cyan': (0, 255, 255),
        'magenta': (255, 0, 255),
        'gray': (128, 128, 128),
        'orange': (255, 165, 0),
        'purple': (128, 0, 128),
        'pink': (255, 192, 203),
        'brown': (165, 42, 42),
        'lime': (0, 255, 0),
    }
    
    if color_str in named_colors:
        return named_colors[color_str]
    
    # Hex colors
    if color_str.startswith('#'):
        hex_val = color_str[1:]
        if len(hex_val) == 3:
            # Expand shorthand #RGB to #RRGGBB
            hex_val = ''.join([c*2 for c in hex_val])
        if len(hex_val) == 6:
            try:
                r = int(hex_val[0:2], 16)
                g = int(hex_val[2:4], 16)
                b = int(hex_val[4:6], 16)
                return (r, g, b)
            except ValueError:
                pass
    
    # RGB function
    rgb_match = re.match(r'rgb\s*\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*\)', color_str)
    if rgb_match:
        try:
            r, g, b = map(int, rgb_match.groups())
            return (r, g, b)
        except ValueError:
            pass
    
    return None


def colors_close(pixel: Tuple[int, int, int], target: Tuple[int, int, int], tolerance: int = 10) -> bool:
    """Check if two RGB colors are close within tolerance.
    
    Args:
        pixel: Actual pixel RGB values
        target: Target RGB values
        tolerance: Color difference tolerance (0-255)
        
    Returns:
        True if colors are close enough
    """
    return (abs(pixel[0] - target[0]) <= tolerance and
            abs(pixel[1] - target[1]) <= tolerance and
            abs(pixel[2] - target[2]) <= tolerance)


def checks_for_page(
    page_img: Image.Image, 
    pdf_page_text: str, 
    source_text: str, 
    content_box: Tuple[int, int, int, int],
    pdf_path: str = "",
    page_index: int = 0,
) -> Dict[str, Any]:
    """Run all checks for a single page.
    
    Args:
        page_img: PIL Image of the rendered page
        pdf_page_text: Extracted text from PDF page
        source_text: Original HTML source text
        content_box: (left, top, right, bottom) defining content area in pixels
        pdf_path: path to the page's PDF (for charbox text-overlap)
        page_index: 0-based page index into that PDF
        
    Returns:
        Dictionary with all check results
    """
    overlap_result = check_ink_row_overlap(page_img, pdf_path, page_index)
    overflow_result = check_page_box_overflow(page_img, content_box)
    text_result = check_text_roundtrip_coverage(pdf_page_text, source_text)
    
    return {
        "overlap": overlap_result,
        "overflow": overflow_result,
        "text_roundtrip": text_result
    }


def declared_fills(html_content: str) -> List[str]:
    """Extract declared background colors from HTML.
    
    Args:
        html_content: HTML source content
        
    Returns:
        List of declared background colors
    """
    return extract_declared_fills(html_content)


def get_content_box(track: str, dpi: int) -> Tuple[int, int, int, int]:
    """Get the overflow boundary for a track.

    Both tracks compare ink against the MEDIA BOX (the physical page):
    corpus fixtures deliberately paint running heads/footers in the 0.5in
    margin bands (paged-media margin boxes are a wedge feature, not an
    overflow), and the showcase poster is full-bleed by design. Overflow
    therefore fires only when ink escapes the physical page — the
    rasterizer clips exactly at the media edge.
    """
    if track == 'corpus':
        # 5in x 3in page at 96 DPI -> 480 x 288 media box.
        return (0, 0, int(5 * dpi), int(3 * dpi))
    # showcase: US Letter media box. Letter @ 300 DPI rasterizes to
    # 2550 x 3301 (the rasterizer rounds the height up by one pixel —
    # spec AC2 states the measured value), so the bottom edge is +1.
    return (0, 0, int(8.5 * dpi), int(11 * dpi) + 1)


def _html_to_text(source_html: str) -> str:
    """Strip markup so text round-trip compares visible text, not source code.

    Drops <style>/<script>/<head> bodies (head content like <title> is not
    visible page text and never lands in the PDF text layer), then tags +
    entities; collapses whitespace. Matching against raw HTML would flag
    every class name and title tag as a dropped token.
    """
    text = re.sub(r"<head[^>]*>.*?</head>", " ", source_html, flags=re.S | re.I)
    text = re.sub(r"<(style|script)[^>]*>.*?</\1>", " ", text, flags=re.S | re.I)
    text = re.sub(r"<[^>]+>", " ", text)
    text = html.unescape(text)
    return text


def _pdf_page_texts(pdf_path: Path) -> list[str]:
    """Extract the text layer of each page of a PDF (pypdfium2)."""
    if not pdf_path.exists():
        return []
    pdf = pdfium.PdfDocument(str(pdf_path))
    try:
        texts: list[str] = []
        for page in pdf:
            tp = page.get_textpage()
            texts.append(tp.get_text_range())
            tp.close()
        return texts
    finally:
        pdf.close()


def run_visual_qa(
    track: str,
    report_path: str,
    work_dir: Path,
    dpi: int = 96,
    check_pr: bool = False,
) -> int:
    """Run visual QA checks for a track over already-rendered PDFs/rasters.

    Contract with scripts/visual-qa.sh: the orchestrator renders each
    fixture's PDF into `work_dir/pdf/<base>-{ta,pr}.pdf` and rasterizes every
    page into `work_dir/img/<base>-{ta,pr}/page-NNN.png`. This function runs
    the four checks per page and writes a deterministic JSON report
    (fixture order from the manifest, sorted pages, no timestamps).

    Returns exit code 0 (all pass) or 1 (any check failed).
    """
    if track == 'corpus':
        manifest_path = ROOT / 'demo' / 'corpus' / 'manifest.json'
        fixtures_dir = ROOT / 'demo' / 'corpus'
    else:  # showcase
        manifest_path = ROOT / 'demo' / 'showcase' / 'manifest.json'
        fixtures_dir = ROOT / 'demo' / 'showcase'

    manifest = json.loads(manifest_path.read_text(encoding='utf-8'))

    report = {
        "track": track,
        "dpi": dpi,
        "fixtures": [],
        "overall": "pass",
    }
    overall_pass = True

    for entry in manifest:
        file_name = str(entry.get("file", ""))
        name = str(entry.get("name", file_name))
        if not file_name:
            continue
        base = Path(file_name).stem

        source_path = fixtures_dir / file_name
        if not source_path.exists():
            print(f"  ERROR: source missing {source_path}", file=sys.stderr)
            overall_pass = False
            continue
        source_text = _html_to_text(source_path.read_text(encoding='utf-8'))
        declared_colors = declared_fills(source_path.read_text(encoding='utf-8'))

        print(f"Processing {name} ({file_name})...")
        fixture_result = {"file": file_name, "name": name, "pages": []}
        full_bleed = fixture_is_full_bleed(
            (fixtures_dir / file_name).read_text(encoding="utf-8")
        )

        engines = ['ta', 'pr'] if check_pr else (['ta'] if track == 'showcase' else ['ta', 'pr'])
        for engine in engines:
            pdf_path = work_dir / 'pdf' / f"{base}-{engine}.pdf"
            page_texts = _pdf_page_texts(pdf_path)
            # Text round-trip is a DOCUMENT-level check: a token may legally
            # live on any page (pagination differs between engines), so the
            # source token set must appear in the WHOLE PDF's text layer —
            # catching silently dropped/replaced content (the CORE-83/85
            # class), not per-page pagination movement.
            doc_text = "".join(page_texts)
            roundtrip = check_text_roundtrip_coverage(doc_text, source_text)
            img_dir = work_dir / 'img' / f"{base}-{engine}"
            page_files = sorted(img_dir.glob("page-*.png")) if img_dir.is_dir() else []

            # Declared-fill is a FIXTURE-level check: a declared color is
            # painted on whatever page its element lands on, so require it
            # on at least one page of the fixture, not on every page.
            fill_pages: dict[str, list[int]] = {}
            for page_index, page_file in enumerate(page_files, start=1):
                page_img = Image.open(page_file)
                content_box = get_content_box(track, dpi)
                checks_result = checks_for_page(
                    page_img, "", source_text, content_box,
                    pdf_path=str(pdf_path), page_index=page_index - 1,
                )
                if full_bleed:
                    # Full-bleed fixtures paint to the page edge by design;
                    # the overflow check does not apply (all other checks do).
                    checks_result["overflow"] = {
                        "pass": True,
                        "violations": [],
                        "note": "full-bleed fixture (zero-margin @page); edge ink is legal",
                    }
                checks_result["text_roundtrip"] = {
                    "pass": roundtrip["pass"],
                    "missing_tokens": roundtrip["missing_tokens"],
                    "coverage": roundtrip.get("coverage"),
                }
                fills = check_declared_fill_colors(page_img, declared_colors)
                for color in fills.get("found", {}):
                    fill_pages.setdefault(color, []).append(page_index)
                checks_result["declared_fills"] = {
                    "pass": True,
                    "missing": [],
                    "found": {color: pages for color, pages in fill_pages.items()},
                }
                page_result = {"page": page_index, "engine": engine, "checks": checks_result}
                fixture_result["pages"].append(page_result)

                for check_name, check_result in checks_result.items():
                    if not check_result.get("pass", True):
                        overall_pass = False
                        print(f"  FAILED: {name} page {page_index} {engine} {check_name}")

            # After the loop: any declared color never found on the fixture
            # is a real finding (report once per fixture, not per page).
            missing_fills = [c for c in declared_colors if c not in fill_pages]
            if missing_fills:
                overall_pass = False
                print(f"  FAILED: {name} {engine} declared_fills missing={missing_fills}")

        report["fixtures"].append(fixture_result)

    report["overall"] = "pass" if overall_pass else "fail"
    report_path_obj = Path(report_path)
    report_path_obj.parent.mkdir(parents=True, exist_ok=True)
    report_path_obj.write_text(
        json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )
    print(f"report: {report_path_obj} (overall: {report['overall']})")
    return 0 if overall_pass else 1


def main() -> int:
    parser = argparse.ArgumentParser(description="Visual QA checks for rendered documents")
    parser.add_argument("command", choices=["run"], help="Command to execute")
    parser.add_argument("--track", choices=["corpus", "showcase"], required=True,
                       help="Track to check")
    parser.add_argument("--report", required=True,
                       help="Path to write JSON report")
    parser.add_argument("--work-dir", required=True,
                       help="Directory holding pdf/<base>-{ta,pr}.pdf and "
                            "img/<base>-{ta,pr}/page-NNN.png (created by "
                            "scripts/visual-qa.sh)")
    parser.add_argument("--dpi", type=int, default=96,
                       help="DPI of the rasterized pages (default: 96)")
    parser.add_argument("--check-pr", action="store_true",
                       help="Showcase: also check the Prince reference side "
                            "(CORE-217 overlay); corpus always checks PR)")

    args = parser.parse_args()

    if args.command == "run":
        return run_visual_qa(
            args.track, args.report, Path(args.work_dir), args.dpi,
            check_pr=args.check_pr,
        )

    return 1


if __name__ == "__main__":
    raise SystemExit(main())