"""Manifest extraction on fixture HTML files."""

from __future__ import annotations

from pathlib import Path

import pytest

from harness.manifest import (
    FuzzyRange,
    enumerate_tests,
    parse_test,
)

FIXTURE_ROOT = Path(__file__).parent / "fixtures" / "wpt"


def _test(name: str):
    return parse_test(FIXTURE_ROOT / "css" / "css-page" / name, FIXTURE_ROOT)


def test_simple_match_ref_resolved():
    tc = _test("simple-print.html")
    assert tc is not None
    assert tc.id == "css/css-page/simple-print.html"
    assert len(tc.refs) == 1
    ref = tc.refs[0]
    assert ref.relation == "=="
    assert ref.path.name == "simple-print-ref.html"
    assert ref.path.exists()
    assert not tc.mismatch


def test_simple_fuzzy_parsed():
    tc = _test("simple-print.html")
    assert tc.fuzzy[None].max_difference == FuzzyRange(0, 2)
    assert tc.fuzzy[None].total_pixels == FuzzyRange(0, 50)


def test_scripted_excluded():
    assert _test("scripted-print.html") is None


def test_absolute_href_and_mismatch_and_pages():
    tc = _test("pages-print.html")
    assert tc is not None
    assert tc.mismatch is True
    ref = tc.refs[0]
    assert ref.relation == "!="
    # Absolute /css/support/other-ref.html resolved against the WPT root.
    assert ref.path == (FIXTURE_ROOT / "css" / "support" / "other-ref.html").resolve()
    assert ref.path.exists()
    # reftest-pages "-2,4,6-"
    assert tc.pages[:3] == [1, 2, 4]
    # per-ref fuzzy keyed by basename + a global entry.
    assert "other-ref.html" in tc.fuzzy
    assert tc.fuzzy["other-ref.html"].max_difference == FuzzyRange(1, 3)
    assert tc.fuzzy[None].max_difference == FuzzyRange(5, 5)


def test_chained_reference_followed_one_level():
    tc = _test("chained-print.html")
    assert tc is not None
    ref = tc.refs[0]
    assert ref.path.name == "chained-print-ref.html"
    assert ref.chained is not None
    assert ref.chained.path.name == "simple-print-ref.html"


def test_enumerate_finds_print_tests_excludes_refs_and_scripts():
    tests = enumerate_tests(
        FIXTURE_ROOT, dirs=("css/css-page", "css/css-break", "css/css-multicol")
    )
    ids = {t.id for t in tests}
    assert "css/css-page/simple-print.html" in ids
    assert "css/css-page/pages-print.html" in ids
    assert "css/css-page/chained-print.html" in ids
    # Reference files and scripted tests are not enumerated as tests.
    assert "css/css-page/simple-print-ref.html" not in ids
    assert "css/css-page/scripted-print.html" not in ids
