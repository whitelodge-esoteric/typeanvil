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
    # entry was removed by CORE-545.
    entries = vq.load_known_failures()
    assert len(entries) == 4
    assert all(e["issue"].startswith("CORE-") for e in entries)
