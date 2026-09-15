"""Direct checks against hand-written PDFs, verified with pypdfium2."""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from harness.direct import DirectCheck, DirectError, check_pdf, load_manifest

from pdf_fixture import Page, build_pdf


def _check(expectation: dict, pdf: bytes) -> "object":
    return check_pdf(
        pdf,
        DirectCheck(
            id="t",
            input="x.html",
            input_source="fixture",
            expectation=expectation,
            provenance="test",
        ),
    )


def test_missing_text_detected():
    page = Page(360, 216)
    page.text(50, 50, "Hello")
    result = _check({"text_present": ["Hello", "World"]}, build_pdf(page))
    assert not result.passed
    assert "actual=['World']" in result.detail


def test_paint_region_mismatch_detected():
    # SYNTHETIC FAULT: text paints where a blank region is expected.
    page = Page(360, 216)
    page.text(50, 50, "Hello")
    result = _check(
        {"paint_region": {"page": 0, "rect_pt": [40, 35, 90, 60], "max_dark_fraction": 0.02}},
        build_pdf(page),
    )
    assert not result.passed
    assert "dark_fraction" in result.detail


def test_text_orientation_mismatch_detected():
    # SYNTHETIC FAULT: horizontal text judged against a vertical expectation.
    page = Page(360, 216)
    page.text(50, 50, "Hello")
    result = _check(
        {"text_orientation": {"page": 0, "rect_pt": [40, 35, 90, 60], "expected": "vertical"}},
        build_pdf(page),
    )
    assert not result.passed
    assert "horizontal" in result.detail


def test_link_target_present_passes():
    page = Page(360, 216)
    page.link("https://example.com/typeanvil/quarterly", (100, 100, 160, 120))
    result = _check(
        {"link_target": {"page": 0, "uri": "https://example.com/typeanvil/quarterly"}},
        build_pdf(page),
    )
    assert result.passed


def test_link_target_missing_uri_fails():
    page = Page(360, 216)
    page.link("https://example.com/typeanvil/quarterly", (100, 100, 160, 120))
    result = _check(
        {"link_target": {"page": 0, "uri": "https://example.com/other"}},
        build_pdf(page),
    )
    assert not result.passed
    assert "https://example.com/other" in result.detail


def test_page_count_mismatch_fails():
    result = _check({"page_count": 2}, build_pdf(Page(360, 216)))
    assert not result.passed
    assert "actual=1" in result.detail


def test_page_size_mismatch_fails():
    result = _check({"page_size_pt": [400.0, 300.0]}, build_pdf(Page(360, 216)))
    assert not result.passed
    assert "360.0" in result.detail
    assert "216.0" in result.detail


def test_no_recognised_check_key_raises():
    with pytest.raises(DirectError):
        _check({"bogus": 1}, build_pdf(Page(360, 216)))


def test_empty_provenance_raises(tmp_path):
    manifest = tmp_path / "m.json"
    manifest.write_text(
        json.dumps(
            {
                "schema": "typeanvil.harness.direct/1",
                "checks": [
                    {
                        "id": "x",
                        "input": "a.html",
                        "input_source": "fixture",
                        "expectation": {"page_count": 1},
                        "provenance": "",
                    }
                ],
            }
        )
    )
    with pytest.raises(DirectError):
        load_manifest(manifest)


def test_checked_in_manifest_loads():
    manifest = Path(__file__).parent.parent / "harness" / "direct_manifest.json"
    checks = load_manifest(manifest)
    assert len(checks) == 13
    assert all(c.provenance for c in checks)
    # Every reviewed expectation carries provenance that names its source.
    # Provenance must name a real source: a repo path, a specification, or a
    # measured value. A placeholder string is not provenance.
    assert all(len(c.provenance) >= 20 for c in checks)
    assert any("CSS" in c.provenance or "docs/" in c.provenance for c in checks)
