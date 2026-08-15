"""``reftest-pages`` selection parsing."""

from __future__ import annotations

from harness.manifest import parse_reftest_pages


def test_single():
    assert parse_reftest_pages("2") == [2]


def test_list():
    assert parse_reftest_pages("1,3,5") == [1, 3, 5]


def test_closed_range():
    assert parse_reftest_pages("2-4") == [2, 3, 4]


def test_leading_open_range():
    # "-2" means pages 1..2.
    assert parse_reftest_pages("-2") == [1, 2]


def test_trailing_open_range():
    # "6-" with a small cap means 6..cap.
    assert parse_reftest_pages("6-", max_pages=8) == [6, 7, 8]


def test_mixed():
    assert parse_reftest_pages("-2,4,6-", max_pages=7) == [1, 2, 4, 6, 7]


def test_dedup_and_sort():
    assert parse_reftest_pages("3,1,3,2") == [1, 2, 3]


def test_empty_is_none():
    assert parse_reftest_pages("") is None
    assert parse_reftest_pages("   ") is None
