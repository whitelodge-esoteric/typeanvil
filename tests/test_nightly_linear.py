"""Tests for scripts/nightly_linear.py and its nightly.yml steps (CORE-592).

The HTTP layer is a fake `post` callable; no test touches the network.
"""
import importlib.util
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
_spec = importlib.util.spec_from_file_location("nightly_linear", ROOT / "scripts" / "nightly_linear.py")
nl = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(nl)

WORKFLOW = (ROOT / ".github/workflows/nightly.yml").read_text()
ENV = {"LINEAR_API_KEY": "lin_api_test"}
OPEN = {"id": "issue-uuid-1", "identifier": "CORE-900", "url": "https://linear.app/x/CORE-900",
        "createdAt": "2026-10-01T00:00:00Z"}
NEWER = {"id": "issue-uuid-2", "identifier": "CORE-950", "url": "https://linear.app/x/CORE-950",
         "createdAt": "2026-10-20T00:00:00Z"}

LOG = """timing tickets/weasyprint-ticket.html: 1 page(s) in 0.1s
document                                   page  kind             detail
reports/weasyprint-report.html                1  ink              dark px 742 -> 662
tickets/weasyprint-ticket.html                1  text             c54428fb -> 16a0e91d
tickets/weasyprint-ticket.html                1  image            dhash distance 13 > 4
tickets/weasyprint-ticket.html                1  ink              dark px 1876 -> 77685
regress: FAIL, 4 change(s). Run `regress.py update` in CI's environment if intended (docs/operations/regression-gate.md).
"""


class FakeLinear:
    """Records requests; answers the lookup with `open_issues`."""

    def __init__(self, open_issues=(), errors=None, success=True):
        self.open_issues = list(open_issues)
        self.errors = errors
        self.success = success
        self.calls = []

    def __call__(self, url, headers, payload):
        self.calls.append((url, headers, payload))
        if self.errors:
            return {"errors": self.errors}
        q = payload["query"]
        if "issues(" in q:
            return {"data": {"issues": {"nodes": self.open_issues}}}
        if "issueCreate" in q:
            return {"data": {"issueCreate": {"success": self.success,
                                             "issue": {"id": "new", "identifier": "CORE-999",
                                                       "url": "https://linear.app/x/CORE-999"}}}}
        if "commentCreate" in q:
            return {"data": {"commentCreate": {"success": self.success, "comment": {"id": "c"}}}}
        raise AssertionError(f"unexpected request: {q}")

    def queries(self):
        return [c[2]["query"] for c in self.calls]


def args(*argv):
    return nl.parse_args(list(argv))


def fail_args(*extra):
    base = ["fail", "--run-url", "https://example/run/1", "--branch", "release/2026.10",
            "--sha", "abc1234"]
    return args(*base, *extra)


GREEN = ["green", "--run-url", "https://example/run/2", "--branch", "release/2026.10",
         "--sha", "def5678"]


def test_fail_creates_issue_when_none_open(tmp_path):
    log = tmp_path / "regress.log"
    log.write_text(LOG)
    fake = FakeLinear()
    res = nl.run(fail_args("--regress", str(log)), env=ENV, post=fake)
    assert res["action"] == "created" and res["error"] is None
    assert len(fake.calls) == 2
    url, headers, payload = fake.calls[1]
    assert url == "https://api.linear.app/graphql"
    assert headers["Authorization"] == "lin_api_test"  # personal key, no Bearer prefix
    assert headers["Content-Type"] == "application/json"
    inp = payload["variables"]["input"]
    assert "issueCreate" in payload["query"]
    assert inp["teamId"] == nl.TEAM_ID and inp["projectId"] == nl.PROJECT_ID
    assert inp["title"] == "Nightly full run failing on release/2026.10"
    assert inp["priority"] == 1
    assert inp["labelIds"] == [nl.NIGHTLY_LABEL_ID, nl.BUG_LABEL_ID]
    assert "abc1234" in inp["description"] and "https://example/run/1" in inp["description"]
    assert "dhash distance 13 > 4" in inp["description"]
    assert "nightly-evidence" in inp["description"]


def test_lookup_filters_team_label_and_open_states():
    fake = FakeLinear()
    nl.run(fail_args(), env=ENV, post=fake)
    q, variables = fake.calls[0][2]["query"], fake.calls[0][2]["variables"]
    assert 'nin: ["completed", "canceled"]' in q and "orderBy: createdAt" in q
    assert variables == {"teamId": nl.TEAM_ID, "labelId": nl.NIGHTLY_LABEL_ID}


def test_fail_comments_on_oldest_open_issue():
    fake = FakeLinear([NEWER, OPEN])
    res = nl.run(fail_args(), env=ENV, post=fake)
    assert res["action"] == "commented" and res["issue"]["identifier"] == "CORE-900"
    payload = fake.calls[1][2]
    assert "commentCreate" in payload["query"]
    assert payload["variables"]["input"]["issueId"] == "issue-uuid-1"
    assert not any("issueCreate" in q for q in fake.queries())


def test_green_comments_but_never_changes_state():
    fake = FakeLinear([OPEN])
    res = nl.run(args(*GREEN), env=ENV, post=fake)
    assert res["action"] == "commented"
    assert len(fake.calls) == 2
    body = fake.calls[1][2]["variables"]["input"]["body"]
    assert body == ("Nightly green again on release/2026.10 @ def5678: https://example/run/2. "
                    "Confirm the fix, then mark this Done.")
    for q in fake.queries():
        assert "issueUpdate" not in q and "stateId" not in q and "issueCreate" not in q


def test_green_with_none_open_is_a_noop(capsys):
    fake = FakeLinear()
    assert nl.main(GREEN, env=ENV, post=fake) == 0
    assert len(fake.calls) == 1 and "issues(" in fake.queries()[0]
    assert "no open nightly-failure issue" in capsys.readouterr().out


def test_missing_key_warns_and_sends_nothing(capsys):
    fake = FakeLinear()
    assert nl.main(["fail", "--run-url", "u", "--branch", "b", "--sha", "s"], env={}, post=fake) == 0
    assert fake.calls == []
    assert "::warning::LINEAR_API_KEY is not set; skipping Linear nightly issue" in capsys.readouterr().out


def test_graphql_errors_warn_and_exit_zero(capsys):
    fake = FakeLinear(errors=[{"message": "Authentication required"}])
    res = nl.run(fail_args(), env=ENV, post=fake)
    assert res["action"] == "error" and "Authentication required" in res["error"]
    assert nl.main(GREEN, env=ENV, post=FakeLinear(errors=[{"message": "boom"}])) == 0
    assert "::warning::" in capsys.readouterr().out


def test_mutation_success_false_is_an_error():
    res = nl.run(fail_args(), env=ENV, post=FakeLinear(success=False))
    assert res["action"] == "error" and "success=false" in res["error"]


def test_transport_failure_is_an_error():
    def broken(url, headers, payload):
        raise nl.LinearError("request failed: timed out")
    res = nl.run(fail_args(), env=ENV, post=broken)
    assert res["action"] == "error" and "timed out" in res["error"]


def test_dry_run_looks_up_but_sends_no_mutation(capsys):
    fake = FakeLinear()
    res = nl.run(fail_args("--dry-run"), env=ENV, post=fake)
    assert res["action"] == "dry-run"
    assert len(fake.calls) == 1 and "issues(" in fake.queries()[0]
    assert "issueCreate" in capsys.readouterr().out


def test_dry_run_without_key_prints_query_and_mutations(capsys):
    fake = FakeLinear()
    res = nl.run(fail_args("--dry-run"), env={}, post=fake)
    assert res["action"] == "dry-run" and fake.calls == []
    out = capsys.readouterr().out
    assert "issues(" in out and "issueCreate" in out and "commentCreate" in out


def test_regress_table_extraction():
    table = nl.regress_table(LOG).splitlines()
    assert table[0].startswith("document ")
    assert table[-1].startswith("regress: FAIL, 4 change(s).")
    assert len(table) == 6
    assert not any(l.startswith("timing ") for l in table)


def test_regress_table_falls_back_to_tail():
    text = "\n".join(f"line {i}" for i in range(100))
    table = nl.regress_table(text).splitlines()
    assert len(table) == nl.REGRESS_TAIL_LINES and table[-1] == "line 99"


def test_qa_line_reads_visual_qa_report():
    qa = {"overall": "fail", "new_failures": [{"file": "a.html", "page": 2, "check": "overflow"}],
          "stale_known_failures": [], "known_failures": [{"file": "b.html"}]}
    line = nl.qa_line(qa)
    assert "fail" in line and "1 new failure(s)" in line and "a.html p2 overflow" in line
    assert "artifact" in nl.qa_line(None) and "artifact" in nl.qa_line({"x": 1})


def test_fail_body_reads_qa_report_file(tmp_path):
    qa = tmp_path / "qa-report.json"
    qa.write_text(json.dumps({"overall": "pass", "new_failures": [],
                              "stale_known_failures": [], "known_failures": []}))
    fake = FakeLinear()
    nl.run(fail_args("--qa", str(qa), "--regress", str(tmp_path / "missing.log")),
           env=ENV, post=fake)
    body = fake.calls[1][2]["variables"]["input"]["description"]
    assert "Visual QA: pass, 0 new failure(s)" in body
    assert "no log; see the artifact" in body


def test_workflow_runs_linear_steps_with_secret():
    assert "Open or update the Linear failure issue" in WORKFLOW
    assert "Comment on the Linear failure issue after a green run" in WORKFLOW
    assert "scripts/nightly_linear.py fail" in WORKFLOW
    assert "scripts/nightly_linear.py green" in WORKFLOW
    assert "LINEAR_API_KEY: ${{ secrets.LINEAR_API_KEY }}" in WORKFLOW
    assert "name: nightly-evidence" in WORKFLOW and "actions/download-artifact" in WORKFLOW
    assert "Linear: CORE-490." not in WORKFLOW
