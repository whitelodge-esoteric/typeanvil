"""Report: wptreport shape, history recording, and the regression gate."""

from __future__ import annotations

from pathlib import Path

from harness import report
from harness.runner import TestResult


def _results(statuses: dict[str, str]) -> list[TestResult]:
    return [
        TestResult(id=t, status=s, time=0.01, diff_stats={"max_difference": 0, "total_pixels": 0})
        for t, s in statuses.items()
    ]


def test_wptreport_shape():
    res = [
        TestResult(
            id="css/css-page/a.html",
            status="PASS",
            time=0.5,
            diff_stats={"max_difference": 0, "total_pixels": 0, "pages": [
                {"page": 1, "max_difference": 0, "total_pixels": 0, "passed": True}
            ]},
        )
    ]
    rep = report.build_wptreport(res, engine="chromium")
    assert rep["run_info"]["product"] == "chromium"
    assert rep["results"][0]["test"] == "/css/css-page/a.html"
    assert rep["results"][0]["status"] == "PASS"
    assert rep["results"][0]["duration"] == 500.0
    assert rep["results"][0]["subtests"][0]["status"] == "PASS"


def test_record_and_tally(tmp_path):
    db = tmp_path / "h.sqlite"
    res = _results({"a": "PASS", "b": "FAIL", "c": "ERROR"})
    rid = report.record_run(res, engine="chromium", db_path=db)
    assert rid == 1
    counts = report.tally(res)
    assert counts.total == 3 and counts.passed == 1 and counts.failed == 1
    assert counts.errored == 1


def test_gate_detects_regression(tmp_path):
    db = tmp_path / "h.sqlite"
    # Run 1: a,b pass.
    report.record_run(_results({"a": "PASS", "b": "PASS"}), engine="chromium", db_path=db)
    # Run 2: b regresses.
    report.record_run(_results({"a": "PASS", "b": "FAIL"}), engine="chromium", db_path=db)
    ok, regs = report.gate(db)
    assert not ok
    assert regs == ["b"]


def test_gate_ok_when_no_regression(tmp_path):
    db = tmp_path / "h.sqlite"
    report.record_run(_results({"a": "PASS", "b": "FAIL"}), engine="chromium", db_path=db)
    report.record_run(_results({"a": "PASS", "b": "PASS"}), engine="chromium", db_path=db)
    ok, regs = report.gate(db)
    assert ok
    assert regs == []


def test_gate_no_history_is_ok(tmp_path):
    ok, regs = report.gate(tmp_path / "missing.sqlite")
    assert ok and regs == []


def test_summary_renders_delta(tmp_path):
    db = tmp_path / "h.sqlite"
    report.record_run(_results({"a": "PASS", "b": "FAIL"}), engine="chromium", db_path=db)
    r2 = report.record_run(
        _results({"a": "PASS", "b": "PASS"}), engine="chromium", db_path=db
    )
    summary = report.format_summary(db, r2)
    assert "pass-rate delta" in summary
    assert "newly fixed: 1" in summary
