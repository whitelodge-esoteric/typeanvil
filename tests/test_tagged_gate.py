"""Tagged PDF gate: the `evaluate` contract (docs/specifications/tagged-gate.spec.md).

Regression gate for CORE-377. Each test builds measured results by hand and
asserts how the gate classifies them; no engine binary is involved.
"""
from __future__ import annotations

import pytest

from scripts.tagged_gate import CHECKS, evaluate

ROLES = {"Document": 1, "H1": 1, "P": 3}


def clean(**over):
    r = {
        "render_exit": 0,
        "deterministic": True,
        "ua_exit": 0,
        "totals": {"untagged": 0, "mcids_duplicated": 0, "mcids_dangling": 0},
        "roles": dict(ROLES),
        "coverage": 0.95,
    }
    r.update(over)
    return r


SNAP = {"corpus/a.html": {"roles": dict(ROLES), "coverage": 0.95}}


def kinds(res):
    return sorted((d, c) for d, c, _ in res.failures)


def test_clean_run_passes():
    res = evaluate({"corpus/a.html": clean()}, SNAP, [])
    assert res.passed and not res.failures and not res.expected


@pytest.mark.parametrize(
    "over, check",
    [
        ({"deterministic": False}, "deterministic"),
        ({"ua_exit": 1}, "ua"),
        ({"totals": {"untagged": 3, "mcids_duplicated": 0, "mcids_dangling": 0}}, "untagged"),
        ({"totals": {"untagged": 0, "mcids_duplicated": 1, "mcids_dangling": 0}}, "duplicated"),
        ({"totals": {"untagged": 0, "mcids_duplicated": 0, "mcids_dangling": 2}}, "dangling"),
        ({"roles": {"Document": 1, "P": 4}}, "roles"),
        ({"coverage": 0.90}, "coverage"),
    ],
)
def test_each_check_fails_on_its_own_regression(over, check):
    res = evaluate({"corpus/a.html": clean(**over)}, SNAP, [])
    assert kinds(res) == [("corpus/a.html", check)]
    assert not res.passed


def test_coverage_within_tolerance_or_rising_passes():
    for cov in (0.946, 0.99):
        assert evaluate({"corpus/a.html": clean(coverage=cov)}, SNAP, []).passed


def test_render_failure_short_circuits_other_checks():
    res = evaluate({"corpus/a.html": {"render_exit": -6}}, SNAP, [])
    assert kinds(res) == [("corpus/a.html", "render")]


def test_known_failure_is_expected_not_a_failure():
    known = [{"doc": "corpus/a.html", "check": "ua", "ticket": "CORE-386", "reason": "notdef"}]
    res = evaluate({"corpus/a.html": clean(ua_exit=1)}, SNAP, known)
    assert res.passed
    assert [(e["doc"], e["check"]) for e in res.expected] == [("corpus/a.html", "ua")]


def test_known_failure_that_now_passes_is_stale():
    known = [{"doc": "corpus/a.html", "check": "ua", "ticket": "CORE-386", "reason": "notdef"}]
    res = evaluate({"corpus/a.html": clean()}, SNAP, known)
    assert kinds(res) == [("corpus/a.html", "stale-known-failure")]


def test_known_failure_with_unknown_check_is_invalid():
    known = [{"doc": "corpus/a.html", "check": "colour", "ticket": "CORE-1", "reason": "x"}]
    res = evaluate({"corpus/a.html": clean()}, SNAP, known)
    assert kinds(res) == [("corpus/a.html", "invalid-known-failure")]


def test_known_failure_does_not_mask_a_different_check():
    known = [{"doc": "corpus/a.html", "check": "ua", "ticket": "CORE-386", "reason": "notdef"}]
    over = {"ua_exit": 1, "totals": {"untagged": 2, "mcids_duplicated": 0, "mcids_dangling": 0}}
    res = evaluate({"corpus/a.html": clean(**over)}, SNAP, known)
    assert kinds(res) == [("corpus/a.html", "untagged")]


def test_snapshot_missing_and_stale():
    res = evaluate({"corpus/new.html": clean()}, SNAP, [])
    assert kinds(res) == [("corpus/a.html", "snapshot-stale"), ("corpus/new.html", "snapshot-missing")]


def test_check_names_are_the_spec_list():
    assert CHECKS == ("render", "deterministic", "untagged", "duplicated", "dangling", "ua", "roles", "coverage")
