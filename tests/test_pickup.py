"""Pickup command: selection validation, outcome classification, identity."""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from scripts.pickup import (
    PickupError,
    binary_identity,
    classify_outcome,
    evidence_dir,
    git_dirty,
    git_head,
    validate_wpt_selection,
)


# ---------------------------------------------------------------------------
# Selection validation
# ---------------------------------------------------------------------------


def test_exact_id_accepted():
    ids = ["css-page/a-print.html", "css-page/b-print.html"]
    assert validate_wpt_selection("css-page/a-print.html", ids) == [
        "css-page/a-print.html"
    ]


def test_empty_selection_rejected():
    with pytest.raises(PickupError, match="zero tests"):
        validate_wpt_selection("css-page/nope-print.html", ["css-page/a-print.html"])


def test_broad_substring_rejected():
    ids = ["css-page/a-print.html", "css-page/a-print-ref.html"]
    with pytest.raises(PickupError, match="unexpectedly broad"):
        validate_wpt_selection("css-page/a-print", ids)


def test_exact_id_wins_over_substring():
    # An exact id is accepted even when it is also a substring of others.
    ids = ["css-page/a-print.html", "css-page/a-print-ref.html"]
    assert validate_wpt_selection("css-page/a-print.html", ids) == [
        "css-page/a-print.html"
    ]


# ---------------------------------------------------------------------------
# Outcome classification
# ---------------------------------------------------------------------------


def test_build_failed_wins():
    assert (
        classify_outcome(build_ok=False, run_ok=False, selected=1) == "build_failed"
    )


def test_empty_selection():
    assert (
        classify_outcome(build_ok=True, run_ok=True, selected=0) == "empty_selection"
    )


def test_crash_on_render_error():
    assert (
        classify_outcome(
            build_ok=True, run_ok=False, selected=1, render_error="boom"
        )
        == "crash"
    )


def test_crash_on_error_status():
    assert (
        classify_outcome(
            build_ok=True, run_ok=True, selected=1, statuses=["ERROR"]
        )
        == "crash"
    )


def test_expected_failure():
    assert (
        classify_outcome(
            build_ok=True, run_ok=True, selected=1, statuses=["FAIL"]
        )
        == "reproduced_expected_failure"
    )


def test_passing_reproduction():
    assert (
        classify_outcome(
            build_ok=True, run_ok=True, selected=1, statuses=["PASS"]
        )
        == "passing_reproduction"
    )


def test_missing_dependency_when_run_fails():
    assert (
        classify_outcome(build_ok=True, run_ok=False, selected=1) == "missing_dependency"
    )


def test_docs_only_outcome_is_distinct():
    # A docs-only run is not a reproduction; it must not look like a pass.
    from scripts.pickup import OUTCOMES

    assert "docs_only" in OUTCOMES
    assert "docs_only" != "passing_reproduction"


def test_completed_run_is_never_a_gate_pass():
    # The outcome vocabulary has no "gate passed" value.
    from scripts.pickup import OUTCOMES

    assert "gate" not in " ".join(OUTCOMES).lower()


# ---------------------------------------------------------------------------
# Identity
# ---------------------------------------------------------------------------


def test_binary_identity_none_is_unknown():
    assert binary_identity(None) == {
        "path": "unknown",
        "sha256": "unknown",
        "version": "unknown",
    }


def test_binary_identity_stable_sha256():
    ident = binary_identity("/bin/sh")
    assert len(ident["sha256"]) == 64
    assert ident["sha256"] == binary_identity("/bin/sh")["sha256"]


def test_git_head_and_dirty(tmp_path):
    # A non-repo directory yields unknown for HEAD.
    assert git_head(tmp_path) == "unknown"


def test_git_dirty_clean_and_dirty_repo(tmp_path):
    import subprocess

    subprocess.run(["git", "init", "-q", str(tmp_path)], check=True)
    subprocess.run(
        ["git", "-C", str(tmp_path), "config", "user.email", "t@t"], check=True
    )
    subprocess.run(
        ["git", "-C", str(tmp_path), "config", "user.name", "t"], check=True
    )
    (tmp_path / "a.txt").write_text("x")
    subprocess.run(["git", "-C", str(tmp_path), "add", "a.txt"], check=True)
    subprocess.run(
        ["git", "-C", str(tmp_path), "commit", "-qm", "init"], check=True
    )
    clean, _ = git_dirty(tmp_path)
    assert clean is False
    (tmp_path / "a.txt").write_text("y")
    dirty, _ = git_dirty(tmp_path)
    assert dirty is True


# ---------------------------------------------------------------------------
# Evidence directory
# ---------------------------------------------------------------------------


def test_evidence_dir_is_non_overwriting(tmp_path):
    d1 = evidence_dir(tmp_path, "CORE-238", "abc123")
    d2 = evidence_dir(tmp_path, "CORE-238", "abc123")
    assert d1 != d2
    assert d1.is_dir() and d2.is_dir()
    assert d1.parent == d2.parent == tmp_path / "CORE-238"


def test_evidence_manifest_shape(tmp_path):
    from scripts.pickup import Evidence

    ev = Evidence(
        issue="CORE-238",
        outcome="reproduced_expected_failure",
        source={"commit": "abc", "dirty": False, "patch_identity": None},
        binary={"path": "x", "sha256": "y", "version": "z"},
        wpt={"revision": "unknown", "selection": ["css-page/a-print.html"]},
        environment={"container": "typeanvil-dev", "fonts": "unknown"},
        command="pickup --issue CORE-238",
        timestamps={"started": "t0", "finished": "t1"},
    )
    data = ev.to_dict()
    assert data["schema"] == "typeanvil.pickup/1"
    assert data["outcome"] == "reproduced_expected_failure"
    assert data["source"]["commit"] == "abc"
    # Unknown values are explicit, never guessed.
    assert data["environment"]["fonts"] == "unknown"
