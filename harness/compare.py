"""Pixel comparison with WPT reftest fuzzy semantics.

WPT reftest semantics (research brief section 2): the test and reference images must be
pixel-identical, unless a ``<meta name=fuzzy>`` tolerance permits it. Fuzzy defines two
budgets:

* ``maxDifference`` -- the maximum per-channel absolute difference allowed for *any*
  single differing pixel. A pixel whose worst channel delta exceeds this bound is a
  hard failure regardless of count.
* ``totalPixels`` -- the number of pixels that may differ at all (within the
  ``maxDifference`` bound).

Both are ranges (``lo-hi``); a value inside ``[lo, hi]`` passes. wptrunner treats the
range as a *both-ends* assertion (too-few differences can also fail, matching the
authored expectation), which we mirror.

Comparison is page-by-page. A page-count mismatch is a FAIL unless the test selected a
specific page subset via ``reftest-pages`` (in which case only the selected pages are
compared). For ``rel=mismatch``, PASS means the images differ *beyond* tolerance.

On failure a visual diff triptych (test | ref | diff-heatmap) is written per page under
``artifacts/<test-id>/``.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path

from PIL import Image, ImageChops

from .manifest import Fuzzy


@dataclass
class PageDiff:
    """Per-page comparison statistics."""

    page_index: int  # 1-based page number (in the compared sequence)
    max_difference: int  # worst per-channel delta over all pixels
    total_pixels: int  # count of pixels that differ at all
    passed: bool


@dataclass
class CompareResult:
    """Outcome of comparing a test image list against a reference image list."""

    passed: bool
    pages: list[PageDiff] = field(default_factory=list)
    reason: str = ""
    diff_artifacts: list[Path] = field(default_factory=list)

    @property
    def max_difference(self) -> int:
        return max((p.max_difference for p in self.pages), default=0)

    @property
    def total_pixels(self) -> int:
        return max((p.total_pixels for p in self.pages), default=0)


def _diff_stats(a: Image.Image, b: Image.Image) -> tuple[int, int, Image.Image]:
    """Return ``(max_difference, total_differing_pixels, per-pixel-max diff image)``.

    ``max_difference`` is the largest single-channel absolute delta anywhere.
    ``total_differing_pixels`` counts pixels differing in any channel. The returned
    diff image is greyscale, each pixel = its worst channel delta (for the heatmap).
    """
    if a.size != b.size:
        # Normalize to the larger canvas so a size difference registers as a full diff.
        w = max(a.width, b.width)
        h = max(a.height, b.height)
        a = _pad(a, w, h)
        b = _pad(b, w, h)

    a = a.convert("RGB")
    b = b.convert("RGB")
    diff = ImageChops.difference(a, b)  # per-channel |a-b|

    # Per-pixel worst channel: split, take max across channels.
    r, g, bch = diff.split()
    worst = ImageChops.lighter(ImageChops.lighter(r, g), bch)

    max_difference = worst.getextrema()[1]
    # Count pixels with any nonzero delta (histogram bin 0 == identical pixels).
    total = sum(worst.histogram()[1:])
    return max_difference, total, worst


def _pad(img: Image.Image, w: int, h: int) -> Image.Image:
    canvas = Image.new("RGB", (w, h), (255, 255, 255))
    canvas.paste(img.convert("RGB"), (0, 0))
    return canvas


def _fuzzy_page_pass(max_diff: int, total: int, fuzzy: Fuzzy | None) -> bool:
    """Return True if this page's diff stats are within the fuzzy budget (match mode)."""
    if max_diff == 0 and total == 0:
        return True
    if fuzzy is None:
        return False
    # A single over-budget pixel is a hard fail.
    if max_diff > fuzzy.max_difference.hi:
        return False
    # Differing-pixel count must land inside the allowed range.
    return fuzzy.total_pixels.contains(total)


def _select_pages(
    images: list[Image.Image], pages: list[int] | None
) -> list[tuple[int, Image.Image]]:
    """Return ``(page_number, image)`` for the selected pages (1-based)."""
    if pages is None:
        return list(enumerate(images, start=1))
    out: list[tuple[int, Image.Image]] = []
    for p in pages:
        if 1 <= p <= len(images):
            out.append((p, images[p - 1]))
    return out


def compare(
    test_images: list[Image.Image],
    ref_images: list[Image.Image],
    *,
    fuzzy: Fuzzy | None = None,
    pages: list[int] | None = None,
    mismatch: bool = False,
    artifact_dir: Path | None = None,
) -> CompareResult:
    """Compare rendered test vs reference page images with WPT fuzzy semantics.

    ``fuzzy``: applies to every compared page.
    ``pages``: 1-based page selection; ``None`` compares all pages.
    ``mismatch``: if True, PASS iff the images differ beyond tolerance.
    ``artifact_dir``: if given and the comparison fails (match mode), write a per-page
        triptych PNG (test | ref | diff heatmap) here.
    """
    test_sel = _select_pages(test_images, pages)
    # wptrunner print-reftest semantics: when a `reftest-pages` selection
    # names pages the reference does not have, the reference's LAST page is
    # repeated for the remainder (the reference is a shortened rendering of
    # the same pagination, and the selected test pages beyond its count are
    # expected to match its final page). Only applies to explicit page
    # selections; unselected comparisons keep the strict count check below.
    if pages is not None and ref_images and len(ref_images) < len(test_images):
        ref_images = list(ref_images) + [ref_images[-1]] * (len(test_images) - len(ref_images))
    ref_sel = _select_pages(ref_images, pages)

    # Page-count mismatch: for rel=match this is a hard FAIL; for rel=mismatch a
    # differing page count is itself a difference, so it PASSes. An explicit page
    # subset short-circuits the whole-document count check (only selected pages count).
    if pages is None and len(test_images) != len(ref_images):
        reason = f"page count mismatch: test={len(test_images)} ref={len(ref_images)}"
        return CompareResult(passed=mismatch, reason="" if mismatch else reason)
    if len(test_sel) != len(ref_sel):
        reason = (
            f"selected page count mismatch: test={len(test_sel)} ref={len(ref_sel)}"
        )
        return CompareResult(passed=mismatch, reason="" if mismatch else reason)
    if not test_sel:
        return CompareResult(passed=False, reason="no pages to compare")

    page_diffs: list[PageDiff] = []
    diff_images: list[tuple[int, Image.Image, Image.Image, Image.Image]] = []
    any_differs = False

    for (pnum, t_img), (_, r_img) in zip(test_sel, ref_sel):
        max_diff, total, worst = _diff_stats(t_img, r_img)
        within = _fuzzy_page_pass(max_diff, total, fuzzy)
        if not within:
            any_differs = True
        page_diffs.append(
            PageDiff(
                page_index=pnum,
                max_difference=max_diff,
                total_pixels=total,
                passed=within,
            )
        )
        diff_images.append((pnum, t_img, r_img, worst))

    if mismatch:
        # rel=mismatch: pass iff at least one compared page differs beyond tolerance.
        passed = any_differs
        reason = "" if passed else "mismatch expected but images matched"
        result = CompareResult(passed=passed, pages=page_diffs, reason=reason)
    else:
        passed = all(p.passed for p in page_diffs)
        reason = "" if passed else "pixel difference exceeds fuzzy budget"
        result = CompareResult(passed=passed, pages=page_diffs, reason=reason)

    if not result.passed and not mismatch and artifact_dir is not None:
        result.diff_artifacts = _write_triptychs(artifact_dir, diff_images, page_diffs)

    return result


def _write_triptychs(
    artifact_dir: Path,
    diff_images: list[tuple[int, Image.Image, Image.Image, Image.Image]],
    page_diffs: list[PageDiff],
) -> list[Path]:
    """Write one triptych PNG (test | ref | diff heatmap) per failing page."""
    artifact_dir.mkdir(parents=True, exist_ok=True)
    written: list[Path] = []
    failing = {p.page_index for p in page_diffs if not p.passed}
    for pnum, t_img, r_img, worst in diff_images:
        if pnum not in failing:
            continue
        t = t_img.convert("RGB")
        r = r_img.convert("RGB")
        # Heatmap: amplify the worst-channel delta into red for visibility.
        heat = Image.merge("RGB", (worst, Image.new("L", worst.size, 0), Image.new("L", worst.size, 0)))
        heat = heat.resize(t.size) if heat.size != t.size else heat

        gap = 4
        w = t.width + r.width + heat.width + gap * 2
        h = max(t.height, r.height, heat.height)
        canvas = Image.new("RGB", (w, h), (200, 200, 200))
        x = 0
        for panel in (t, r, heat):
            canvas.paste(panel, (x, 0))
            x += panel.width + gap
        out = artifact_dir / f"page-{pnum:03d}.png"
        canvas.save(out)
        written.append(out)
    return written
