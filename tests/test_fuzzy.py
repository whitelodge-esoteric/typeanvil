"""Fuzzy ``<meta name=fuzzy>`` parsing across all documented syntax forms."""

from __future__ import annotations

from harness.manifest import FuzzyRange, parse_fuzzy


def test_keyed_ranges():
    fz = parse_fuzzy("maxDifference=10-15;totalPixels=200-300")
    assert None in fz
    f = fz[None]
    assert f.max_difference == FuzzyRange(10, 15)
    assert f.total_pixels == FuzzyRange(200, 300)


def test_bare_values():
    fz = parse_fuzzy("maxDifference=15;totalPixels=300")
    f = fz[None]
    assert f.max_difference == FuzzyRange(15, 15)
    assert f.total_pixels == FuzzyRange(300, 300)


def test_positional_ranges():
    # No keys, just "a-b;c-d".
    fz = parse_fuzzy("10-15;200-300")
    f = fz[None]
    assert f.max_difference == FuzzyRange(10, 15)
    assert f.total_pixels == FuzzyRange(200, 300)


def test_positional_bare():
    fz = parse_fuzzy("2;40")
    f = fz[None]
    assert f.max_difference == FuzzyRange(2, 2)
    assert f.total_pixels == FuzzyRange(40, 40)


def test_per_ref_prefix():
    fz = parse_fuzzy("green-ref.html:maxDifference=1-3;totalPixels=100-200")
    assert "green-ref.html" in fz
    assert None not in fz
    f = fz["green-ref.html"]
    assert f.max_difference == FuzzyRange(1, 3)
    assert f.total_pixels == FuzzyRange(100, 200)


def test_multiple_entries_global_and_perref():
    fz = parse_fuzzy(
        "other-ref.html:1-3;100-200, maxDifference=5;totalPixels=10"
    )
    assert fz["other-ref.html"].max_difference == FuzzyRange(1, 3)
    assert fz["other-ref.html"].total_pixels == FuzzyRange(100, 200)
    assert fz[None].max_difference == FuzzyRange(5, 5)
    assert fz[None].total_pixels == FuzzyRange(10, 10)


def test_prefix_not_confused_with_keyed():
    # A leading "maxDifference=..." must NOT be treated as a per-ref prefix even though
    # it contains no leading colon token before '='.
    fz = parse_fuzzy("maxDifference=15;totalPixels=300")
    assert None in fz


def test_empty():
    assert parse_fuzzy("") == {}
