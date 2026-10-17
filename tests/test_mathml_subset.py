"""Curated MathML subset (CORE-455) — fetch path and enumeration gates."""

from __future__ import annotations

from pathlib import Path

import pytest

from harness.manifest import enumerate_tests, parse_test
from harness.mathml_subset import MATHML_SUBSET
from harness.wpt_fetch import SPARSE_PATHS, TEST_PATHS


@pytest.fixture(scope="module")
def real_wpt_root() -> Path:
    root = Path(__file__).resolve().parents[1] / ".wpt"
    if not (root / "mathml").is_dir():
        pytest.skip("no WPT mathml checkout (.wpt) available")
    return root


def test_mathml_is_fetched():
    assert "mathml" in TEST_PATHS
    assert "mathml" in SPARSE_PATHS


def test_subset_is_deduplicated_and_nonempty():
    assert MATHML_SUBSET
    assert len(MATHML_SUBSET) == len(set(MATHML_SUBSET))


def test_subset_paths_exist_and_parse(real_wpt_root):
    for rel in MATHML_SUBSET:
        assert (real_wpt_root / rel).is_file(), f"missing WPT test: {rel}"
        tc = parse_test(real_wpt_root / rel, real_wpt_root)
        assert tc is not None, f"script or missing refs: {rel}"
        assert all(r.path.exists() for r in tc.refs), f"broken ref: {rel}"


def test_curated_tests_are_enumerated_and_nothing_else(real_wpt_root):
    ids = {t.id for t in enumerate_tests(real_wpt_root)}
    for rel in MATHML_SUBSET:
        assert rel in ids, f"curated test not enumerated: {rel}"
    leaked = [
        i
        for i in ids
        if i.startswith("mathml/")
        and i not in MATHML_SUBSET
        and not (i.endswith("-print.html") or "/print/" in i)
    ]
    assert not leaked, f"non-curated mathml tests leaked: {leaked}"
