"""Unit tests for scripts/regress.py (CORE-489): diff logic and tolerances."""
import copy
import importlib.util
from pathlib import Path

_spec = importlib.util.spec_from_file_location(
    "regress", Path(__file__).resolve().parent.parent / "scripts" / "regress.py")
regress = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(regress)


def doc(pages=2):
    return {
        "file": "x/y.html", "engine_version": "0", "page_count": pages,
        "pages": [
            {"page": i + 1, "size": [360.0, 216.0], "text_sha256": "a" * 64,
             "dhash": "00ff00ff00ff00ff", "dark_pixels": 10000}
            for i in range(pages)
        ],
    }


def test_identical_is_clean():
    assert regress.diff_doc(doc(), doc()) == []


def test_page_count_change_flagged():
    kinds = {c["kind"] for c in regress.diff_doc(doc(2), doc(3))}
    assert "page-count" in kinds


def test_text_hash_exact():
    h = doc()
    h["pages"][1]["text_sha256"] = "b" * 64
    changes = regress.diff_doc(doc(), h)
    assert [(c["page"], c["kind"]) for c in changes] == [(2, "text")]


def test_dhash_threshold():
    h = doc()
    h["pages"][0]["dhash"] = "00ff00ff00ff00f0"  # 4 bits: at the limit, allowed
    assert regress.diff_doc(doc(), h) == []
    h["pages"][0]["dhash"] = "00ff00ff00ff0000"  # 8 bits: flagged
    assert [c["kind"] for c in regress.diff_doc(doc(), h)] == ["image"]


def test_dark_pixel_tolerance():
    h = doc()
    h["pages"][0]["dark_pixels"] = 10010  # 0.1 %: allowed
    assert regress.diff_doc(doc(), h) == []
    h["pages"][0]["dark_pixels"] = 10100  # 1 %: flagged
    assert [c["kind"] for c in regress.diff_doc(doc(), h)] == ["ink"]


def test_size_change_flagged():
    h = copy.deepcopy(doc())
    h["pages"][0]["size"] = [612.0, 792.0]
    assert [c["kind"] for c in regress.diff_doc(doc(), h)] == ["size"]


def test_slug_is_stable():
    assert regress.slug("financial/invoice.html") == "financial-invoice"
