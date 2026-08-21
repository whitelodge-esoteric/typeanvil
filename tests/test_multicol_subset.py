"""Curated css-multicol subset (CORE-77) — enumeration and fixture gates."""

from __future__ import annotations

from pathlib import Path

import pytest

from harness.manifest import enumerate_tests, parse_test
from harness.multicol_subset import MULTICOL_SUBSET

FIXTURE_ROOT = Path(__file__).parent / "fixtures" / "wpt"


@pytest.fixture(scope="module")
def real_wpt_root() -> Path | None:
    """The real WPT checkout when present (CI and dev machines keep it in repo root).

    Tests skip when the checkout is absent so the unit suite stays hermetic.
    """
    root = Path(__file__).resolve().parents[1] / ".wpt"
    if not (root / "css" / "css-multicol").is_dir():
        pytest.skip("no WPT checkout (.wpt) available")
    return root


def test_subset_paths_exist_in_wpt(real_wpt_root):
    for rel in MULTICOL_SUBSET:
        assert (real_wpt_root / rel).is_file(), f"missing WPT test: {rel}"


def test_subset_tests_parse_with_resolvable_refs(real_wpt_root):
    for rel in MULTICOL_SUBSET:
        tc = parse_test(real_wpt_root / rel, real_wpt_root)
        assert tc is not None, f"script or missing refs: {rel}"
        assert all(r.path.exists() for r in tc.refs), f"broken ref target: {rel}"


def test_curated_tests_are_enumerated(real_wpt_root):
    tests = enumerate_tests(real_wpt_root)
    ids = {t.id for t in tests}
    for rel in MULTICOL_SUBSET:
        assert rel in ids, f"curated test not enumerated: {rel}"
    # The curated gate must not leak non-listed screen reftests.
    screen_only = [
        i for i in ids if i.startswith("css/css-multicol/") and i not in MULTICOL_SUBSET
    ]
    assert all(
        i.endswith("-print.html") or "/print/" in i or i in MULTICOL_SUBSET
        for i in screen_only
    ), f"non-print multicol tests leaked into enumeration: {screen_only}"


def test_subset_is_deduplicated():
    assert len(MULTICOL_SUBSET) == len(set(MULTICOL_SUBSET))
