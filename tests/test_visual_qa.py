"""Unit tests for scripts/visual_qa.py page-box overflow checks (CORE-540)."""
import importlib.util
from pathlib import Path

from PIL import Image

_spec = importlib.util.spec_from_file_location(
    "visual_qa", Path(__file__).resolve().parent.parent / "scripts" / "visual_qa.py")
vq = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(vq)

LETTER_BOX = (0, 0, 2550, 3301)


def page(size, ink=None):
    img = Image.new("RGB", size, "white")
    if ink:
        img.putpixel(ink, (0, 0, 0))
    return img


def test_a4_footer_near_letter_bottom_is_not_overflow():
    # A4 @300dpi is 2480x3508; ink at y=3300 is mid-page, not at the edge.
    res = vq.check_page_box_overflow(page((2480, 3508), (1000, 3300)), LETTER_BOX)
    assert res["pass"]


def test_custom_size_ink_beyond_letter_width_is_not_overflow():
    res = vq.check_page_box_overflow(page((3000, 4000), (2550, 100)), LETTER_BOX)
    assert res["pass"]


def test_ink_at_own_right_and_bottom_edge_is_overflow():
    right = vq.check_page_box_overflow(page((2480, 3508), (2479, 500)), LETTER_BOX)
    bottom = vq.check_page_box_overflow(page((2480, 3508), (500, 3507)), LETTER_BOX)
    assert not right["pass"] and right["violations"] == [{"x": 2479, "y": 500}]
    assert not bottom["pass"] and bottom["violations"] == [{"x": 500, "y": 3507}]


def test_ink_at_top_left_edge_is_overflow():
    res = vq.check_page_box_overflow(page((200, 300), (2, 2)), LETTER_BOX)
    assert not res["pass"]


def test_full_bleed_across_multiple_page_blocks():
    html = "<style>@page { size: A4 } @page { margin: 0 }</style>"
    assert vq.fixture_is_full_bleed(html)


def test_full_bleed_ignores_margin_box_and_nonzero():
    assert not vq.fixture_is_full_bleed("@page { size: A4; margin: 2cm }")
    assert not vq.fixture_is_full_bleed(
        "@page { margin: 0; } @page { margin: 1in }")
    assert not vq.fixture_is_full_bleed("<p>no page rule</p>")


def test_full_bleed_nested_margin_box():
    html = "@page { size: A4; margin: 0; @top-center { content: 'x' } }"
    assert vq.fixture_is_full_bleed(html)


def _band_page(color, ink_color=None):
    # A4 at 300 dpi with a full-width band touching both edges, plus an
    # anti-aliased edge row (half-blend with white).
    img = Image.new("RGB", (2481, 3508), "white")
    for x in range(2481):
        for y in range(800, 806):
            img.putpixel((x, y), color)
        img.putpixel((x, 799), tuple((255 + c) // 2 for c in color))
    if ink_color:
        img.putpixel((0, 2000), ink_color)
    return img


YELLOW = (0xfb, 0xc8, 0x47)


def test_declared_bleed_color_at_edge_passes():
    # CORE-546: weasyprint-report's yellow bands bleed by design.
    img = _band_page(YELLOW)
    assert not vq.check_page_box_overflow(img, LETTER_BOX)["pass"]
    assert vq.check_page_box_overflow(img, LETTER_BOX, ["#fbc847"])["pass"]


def test_bleed_allowance_still_flags_other_ink():
    img = _band_page(YELLOW, ink_color=(0x39, 0x39, 0x39))  # body text grey
    res = vq.check_page_box_overflow(img, LETTER_BOX, ["#fbc847"])
    assert not res["pass"]
    assert res["violations"] == [{"x": 0, "y": 2000}]


def test_bleed_allowance_rejects_other_fill():
    img = _band_page((0x20, 0x60, 0xc0))
    assert not vq.check_page_box_overflow(img, LETTER_BOX, ["#fbc847"])["pass"]


def test_edge_bleed_colors_requires_reason():
    import pytest
    assert vq.edge_bleed_colors({"file": "a"}) == []
    assert vq.edge_bleed_colors(
        {"file": "a", "edge_bleed": {"colors": ["#FBC847"], "reason": "band"}}) == ["#fbc847"]
    with pytest.raises(ValueError):
        vq.edge_bleed_colors({"file": "a", "edge_bleed": {"colors": ["#fbc847"]}})
    with pytest.raises(ValueError):
        vq.edge_bleed_colors({"file": "a", "edge_bleed": {"colors": ["red"], "reason": "x"}})


def test_roundtrip_folds_math_alphanumerics():
    # CORE-541: MathML <mi> letters extract as Unicode math italic.
    pdf = "mass \U0001D45A at speed \U0001D463 and the angle \U0001D703 acts"
    src = "mass m at speed v and the angle \u03b8 acts"
    res = vq.check_text_roundtrip_coverage(pdf, src)
    assert res["pass"] and not res["missing_tokens"], res


# CORE-549: known-failures list.
KNOWN = [
    {"file": "a.html", "page": 2, "check": "overflow", "issue": "CORE-1"},
    {"file": "b.html", "page": "*", "check": "text_roundtrip", "issue": "CORE-2"},
]


def test_known_failures_are_not_new():
    rec = vq.reconcile_failures(
        {("a.html", 2, "overflow"), ("b.html", "*", "text_roundtrip")}, KNOWN)
    assert rec["new"] == [] and rec["stale"] == []


def test_unlisted_failure_is_new():
    rec = vq.reconcile_failures({("a.html", 3, "overflow")}, KNOWN)
    assert rec["new"] == [("a.html", 3, "overflow")]


def test_listed_entry_that_passes_is_stale():
    rec = vq.reconcile_failures({("b.html", "*", "text_roundtrip")}, KNOWN)
    assert [e["file"] for e in rec["stale"]] == ["a.html"]


def test_committed_list_cites_issues_and_seeds_five_failures():
    # Seven seeded by CORE-549; the two journal-article entries (CORE-544)
    # were removed when CORE-532 fixed the fixture; the weasyprint-invoice
    # entry was removed by CORE-545. The weasyprint-ticket entry was
    # re-pointed at CORE-558, which removed it when the flex item base size
    # started to include padding. The two weasyprint-report overflow entries
    # were removed by CORE-546 (declared edge_bleed in the manifest). The
    # last entry (academic/math.html) was removed by CORE-548.
    entries = vq.load_known_failures()
    assert len(entries) == 0
    assert all(e["issue"].startswith("CORE-") for e in entries)


def test_roundtrip_splits_function_name_from_greek_argument():
    # CORE-548: <mi>cos</mi><mi>θ</mi> extracts as one glued run.
    pdf = "R = ( \U0001D450\U0001D45C\U0001D460\U0001D703 - sin\u03b8 )"
    src = "R = ( cos \u03b8 - sin \u03b8 )"
    res = vq.check_text_roundtrip_coverage(pdf, src)
    assert res["pass"] and not res["missing_tokens"], res
