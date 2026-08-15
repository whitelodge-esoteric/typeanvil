"""Compare semantics on synthetic PIL images: identical / within-fuzzy / beyond / mismatch."""

from __future__ import annotations

from PIL import Image

from harness.compare import compare
from harness.manifest import Fuzzy, FuzzyRange


def solid(color, size=(10, 10)):
    return Image.new("RGB", size, color)


def with_diff_pixels(base_color, diff_color, n, size=(10, 10)):
    """Return an image equal to solid(base) except ``n`` pixels set to diff_color."""
    img = Image.new("RGB", size, base_color)
    px = img.load()
    count = 0
    for y in range(size[1]):
        for x in range(size[0]):
            if count >= n:
                break
            px[x, y] = diff_color
            count += 1
        if count >= n:
            break
    return img


def test_identical_pass_no_fuzzy():
    a = [solid((255, 0, 0))]
    b = [solid((255, 0, 0))]
    res = compare(a, b)
    assert res.passed
    assert res.max_difference == 0
    assert res.total_pixels == 0


def test_different_fail_no_fuzzy():
    a = [solid((255, 0, 0))]
    b = [solid((0, 0, 0))]
    res = compare(a, b)
    assert not res.passed
    assert res.max_difference == 255


def test_within_fuzzy_passes():
    # 5 pixels differ by exactly 4 per channel; budget allows maxDiff<=10, up to 8 px.
    a = [solid((100, 100, 100))]
    b = [with_diff_pixels((100, 100, 100), (104, 104, 104), 5)]
    fuzzy = Fuzzy(FuzzyRange(0, 10), FuzzyRange(0, 8))
    res = compare(a, b, fuzzy=fuzzy)
    assert res.passed
    assert res.max_difference == 4
    assert res.total_pixels == 5


def test_beyond_fuzzy_maxdiff_fails():
    # A single pixel exceeds the per-channel max difference budget.
    a = [solid((100, 100, 100))]
    b = [with_diff_pixels((100, 100, 100), (200, 100, 100), 1)]
    fuzzy = Fuzzy(FuzzyRange(0, 10), FuzzyRange(0, 100))
    res = compare(a, b, fuzzy=fuzzy)
    assert not res.passed


def test_beyond_fuzzy_totalpixels_fails():
    # Deltas are within maxDiff but too many pixels differ.
    a = [solid((100, 100, 100))]
    b = [with_diff_pixels((100, 100, 100), (105, 105, 105), 20)]
    fuzzy = Fuzzy(FuzzyRange(0, 10), FuzzyRange(0, 8))
    res = compare(a, b, fuzzy=fuzzy)
    assert not res.passed
    assert res.total_pixels == 20


def test_mismatch_pass_when_different():
    a = [solid((255, 0, 0))]
    b = [solid((0, 0, 255))]
    res = compare(a, b, mismatch=True)
    assert res.passed


def test_mismatch_fail_when_identical():
    a = [solid((255, 0, 0))]
    b = [solid((255, 0, 0))]
    res = compare(a, b, mismatch=True)
    assert not res.passed


def test_page_count_mismatch_fails():
    a = [solid((0, 0, 0)), solid((0, 0, 0))]
    b = [solid((0, 0, 0))]
    res = compare(a, b)
    assert not res.passed
    assert "page count" in res.reason


def test_page_count_mismatch_passes_in_mismatch_mode():
    # A differing page count is itself a difference: rel=mismatch must PASS.
    a = [solid((0, 0, 0)), solid((0, 0, 0))]
    b = [solid((0, 0, 0))]
    res = compare(a, b, mismatch=True)
    assert res.passed


def test_page_selection_compares_only_selected():
    # Page 1 differs, page 2 identical. Selecting page 2 only => pass.
    a = [solid((255, 0, 0)), solid((0, 255, 0))]
    b = [solid((0, 0, 0)), solid((0, 255, 0))]
    res = compare(a, b, pages=[2])
    assert res.passed
    assert [p.page_index for p in res.pages] == [2]


def test_page_selection_still_detects_diff():
    a = [solid((255, 0, 0)), solid((0, 255, 0))]
    b = [solid((0, 0, 0)), solid((0, 255, 0))]
    res = compare(a, b, pages=[1])
    assert not res.passed


def test_triptych_written_on_failure(tmp_path):
    a = [solid((255, 0, 0))]
    b = [solid((0, 0, 0))]
    res = compare(a, b, artifact_dir=tmp_path / "t1")
    assert not res.passed
    assert res.diff_artifacts
    assert res.diff_artifacts[0].exists()
