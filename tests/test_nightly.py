"""Tests for scripts/nightly.py and .github/workflows/nightly.yml (CORE-490)."""
import importlib.util
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
_spec = importlib.util.spec_from_file_location("nightly", ROOT / "scripts" / "nightly.py")
nightly = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(nightly)

WORKFLOW = (ROOT / ".github/workflows/nightly.yml").read_text()


def test_latest_release_is_numeric_not_lexical():
    names = ["origin/release/2026.9", "origin/release/2026.10", "origin/main"]
    assert nightly.latest_release(names) == "release/2026.10"


def test_latest_release_none():
    assert nightly.latest_release(["origin/main"]) is None


LOG = """timing a/x.html: 3 page(s) in 1.5s
unbaselined b/y.html: 40 page(s) in 9.0s
document                                   page  kind             detail
a/x.html                                      2  text             aaaa -> bbbb
regress: FAIL, 1 change(s). Run `regress.py update`
"""


def test_parse_regress():
    r = nightly.parse_regress(LOG)
    assert [t[0] for t in r["timings"]] == ["a/x.html", "b/y.html"]
    assert len(r["changes"]) == 1 and "a/x.html" in r["changes"][0]


def test_summary_reports_failure_and_timings():
    s = nightly.summary(LOG, {"fixtures": [{"name": "m", "ok": False}]}, "failure")
    assert "failure" in s and "a/x.html" in s and "Failed: m" in s and "| b/y.html | 40 | 9.0 |" in s


def test_summary_without_qa_report():
    assert "did not finish" in nightly.summary("", None, "failure")


def test_workflow_triggers_and_scope():
    assert re.search(r"^  schedule:", WORKFLOW, re.M)
    assert re.search(r"^  workflow_dispatch:", WORKFLOW, re.M)
    assert "--size all" in WORKFLOW and "--role any" in WORKFLOW
    assert "visual-qa.sh --track showcase" in WORKFLOW


def test_workflow_keeps_prince_off_ci():
    assert "--track corpus" not in WORKFLOW
    assert "prince" not in WORKFLOW.lower().replace("prince/weasyprint comparison", "")


def test_workflow_reports_failure_where_a_human_sees_it():
    assert "issues: write" in WORKFLOW
    assert "statuses: write" in WORKFLOW
    assert "gh issue" in WORKFLOW
    assert "GITHUB_STEP_SUMMARY" in WORKFLOW


def test_summary_separates_known_failures():
    qa = {"new_failures": [], "stale_known_failures": [],
          "known_failures": [{"file": "a.html", "page": 1, "check": "overflow", "issue": "CORE-544"}]}
    out = nightly.summary("", qa, "success")
    assert "Known failures" in out and "CORE-544" in out and "No new failures." in out
