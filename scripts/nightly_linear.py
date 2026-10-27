#!/usr/bin/env python3
"""Open or update the Linear issue for a red nightly full run (CORE-592).

  nightly_linear.py fail  --run-url URL --branch B --sha SHA
                          [--result R] [--regress LOG] [--qa REPORT.json] [--dry-run]
  nightly_linear.py green --run-url URL --branch B --sha SHA [--dry-run]

`fail` comments on the oldest open issue labelled `nightly-failure` in team
Core, or creates one. `green` comments on that issue and never changes its
state. The key comes from LINEAR_API_KEY. A missing key or an API error is a
GitHub `::warning::` and exit 0: this step must never turn the nightly red.

Stdlib only. Unit-tested in tests/test_nightly_linear.py with a fake transport.
"""
from __future__ import annotations

import argparse
import json
import os
import sys
import urllib.error
import urllib.request
from pathlib import Path
from typing import Callable

API_URL = os.environ.get("LINEAR_API_URL", "https://api.linear.app/graphql")
TEAM_ID = os.environ.get("LINEAR_TEAM_ID", "48693d0c-8d47-4e92-b01f-b5e44f17e1a6")  # Core
PROJECT_ID = os.environ.get("LINEAR_PROJECT_ID", "2949a51f-360e-43a7-a8a2-53295a47b213")  # TypeAnvil Core
NIGHTLY_LABEL_ID = os.environ.get("LINEAR_NIGHTLY_LABEL_ID", "99735758-ad0c-4270-ac45-4364561c0e6c")
BUG_LABEL_ID = os.environ.get("LINEAR_BUG_LABEL_ID", "0e7f838c-2cf2-403e-96f4-c80dbb100fd5")

LOOKUP_QUERY = """query NightlyOpenIssue($teamId: ID!, $labelId: ID!) {
  issues(
    filter: {
      team: { id: { eq: $teamId } }
      labels: { id: { eq: $labelId } }
      state: { type: { nin: ["completed", "canceled"] } }
    }
    first: 5
    orderBy: createdAt
  ) {
    nodes { id identifier url createdAt }
  }
}"""

CREATE_MUTATION = """mutation NightlyIssueCreate($input: IssueCreateInput!) {
  issueCreate(input: $input) { success issue { id identifier url } }
}"""

COMMENT_MUTATION = """mutation NightlyCommentCreate($input: CommentCreateInput!) {
  commentCreate(input: $input) { success comment { id url } }
}"""

REGRESS_TAIL_LINES = 30

# post(url, headers, payload) -> parsed JSON response body.
Post = Callable[[str, dict, dict], dict]


class LinearError(Exception):
    pass


def http_post(url: str, headers: dict, payload: dict) -> dict:
    req = urllib.request.Request(url, data=json.dumps(payload).encode("utf-8"),
                                 headers=headers, method="POST")
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            return json.loads(resp.read().decode("utf-8"))
    except urllib.error.HTTPError as e:
        detail = e.read().decode("utf-8", "replace")[:500]
        raise LinearError(f"HTTP {e.code}: {detail}") from e
    except (urllib.error.URLError, TimeoutError, json.JSONDecodeError) as e:
        raise LinearError(f"request failed: {e}") from e


def graphql(post: Post, key: str, query: str, variables: dict) -> dict:
    """Send one GraphQL request; return `data` or raise LinearError."""
    headers = {"Authorization": key, "Content-Type": "application/json"}
    resp = post(API_URL, headers, {"query": query, "variables": variables})
    if not isinstance(resp, dict):
        raise LinearError(f"unexpected response: {resp!r}")
    if resp.get("errors"):
        msgs = "; ".join(str(e.get("message", e)) if isinstance(e, dict) else str(e)
                         for e in resp["errors"])
        raise LinearError(f"GraphQL errors: {msgs}")
    data = resp.get("data")
    if not isinstance(data, dict):
        raise LinearError("response has no data")
    return data


def lookup_variables() -> dict:
    return {"teamId": TEAM_ID, "labelId": NIGHTLY_LABEL_ID}


def find_open_issue(post: Post, key: str) -> dict | None:
    """Oldest open `nightly-failure` issue in the team, or None."""
    data = graphql(post, key, LOOKUP_QUERY, lookup_variables())
    nodes = ((data.get("issues") or {}).get("nodes")) or []
    if not nodes:
        return None
    return min(nodes, key=lambda n: n.get("createdAt") or "")


def mutate(post: Post, key: str, mutation: str, field: str, variables: dict) -> dict:
    data = graphql(post, key, mutation, variables)
    payload = data.get(field) or {}
    if not payload.get("success"):
        raise LinearError(f"{field} returned success=false")
    return payload


def regress_table(text: str) -> str:
    """Change table from `regress.py check` output, header through summary line.

    Falls back to the last REGRESS_TAIL_LINES lines when the table is absent.
    """
    lines = text.splitlines()
    start = next((i for i, l in enumerate(lines)
                  if l.startswith("document ") and "kind" in l), None)
    if start is not None:
        end = next((i for i in range(start + 1, len(lines))
                    if lines[i].startswith("regress:")), None)
        if end is not None:
            return "\n".join(l.rstrip() for l in lines[start:end + 1])
    return "\n".join(l.rstrip() for l in lines[-REGRESS_TAIL_LINES:])


def qa_line(qa: dict | None) -> str:
    """One-line visual-QA summary from qa-report.json (scripts/visual_qa.py)."""
    if not isinstance(qa, dict):
        return "Visual QA: no report; see the artifact."
    if "new_failures" in qa:
        new = qa.get("new_failures") or []
        stale = qa.get("stale_known_failures") or []
        known = qa.get("known_failures") or []
        parts = [f"{qa.get('overall', 'unknown')}", f"{len(new)} new failure(s)"]
        if new:
            parts[-1] += " (" + ", ".join(
                f"{f.get('file')} p{f.get('page')} {f.get('check')}" for f in new[:5]) + \
                (", ..." if len(new) > 5 else "") + ")"
        parts.append(f"{len(stale)} stale known failure(s)")
        parts.append(f"{len(known)} known failure(s)")
        return "Visual QA: " + ", ".join(parts) + "."
    if "overall" in qa:
        return f"Visual QA: {qa['overall']}; see the artifact."
    return "Visual QA: see the artifact."


def fail_body(run_url: str, branch: str, sha: str, result: str,
              regress_text: str | None, qa: dict | None) -> str:
    lines = [f"Nightly full run **{result}** on `{branch}` @ `{sha}`. Run: {run_url}", ""]
    if regress_text:
        lines += ["Regression check:", "", "```text", regress_table(regress_text), "```"]
    else:
        lines.append("Regression check: no log; see the artifact.")
    lines += ["", qa_line(qa), "",
              f"Evidence: download the `nightly-evidence` artifact from the run: {run_url}"]
    return "\n".join(lines) + "\n"


def green_body(run_url: str, branch: str, sha: str) -> str:
    return (f"Nightly green again on {branch} @ {sha}: {run_url}. "
            "Confirm the fix, then mark this Done.")


def create_variables(branch: str, body: str) -> dict:
    return {"input": {
        "teamId": TEAM_ID,
        "projectId": PROJECT_ID,
        "title": f"Nightly full run failing on {branch}",
        "description": body,
        "priority": 1,
        "labelIds": [NIGHTLY_LABEL_ID, BUG_LABEL_ID],
    }}


def comment_variables(issue_id: str, body: str) -> dict:
    return {"input": {"issueId": issue_id, "body": body}}


def _read_inputs(args) -> tuple[str | None, dict | None]:
    regress_text = qa = None
    if getattr(args, "regress", None) and Path(args.regress).is_file():
        regress_text = Path(args.regress).read_text(encoding="utf-8", errors="replace")
    if getattr(args, "qa", None) and Path(args.qa).is_file():
        try:
            qa = json.loads(Path(args.qa).read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            qa = None
    return regress_text, qa


def _show(out, title: str, query: str, variables: dict) -> None:
    print(f"--- {title}", file=out)
    print(query, file=out)
    print(json.dumps(variables, indent=2), file=out)


def run(args, env: dict | None = None, post: Post = http_post, out=None) -> dict:
    """Do the work. Returns {"action": ..., "issue": ..., "error": str | None}.

    Never raises for missing keys or API failures; the caller turns `error`
    into a warning.
    """
    env = os.environ if env is None else env
    out = sys.stdout if out is None else out
    key = env.get("LINEAR_API_KEY") or ""
    if args.cmd == "fail":
        regress_text, qa = _read_inputs(args)
        body = fail_body(args.run_url, args.branch, args.sha, args.result, regress_text, qa)
    else:
        body = green_body(args.run_url, args.branch, args.sha)

    if not key:
        if not args.dry_run:
            return {"action": "skipped", "issue": None,
                    "error": "LINEAR_API_KEY is not set; skipping Linear nightly issue"}
        print("dry run, no LINEAR_API_KEY: the lookup would send", file=out)
        _show(out, "lookup query", LOOKUP_QUERY, lookup_variables())
        if args.cmd == "fail":
            _show(out, "if no open issue: mutation", CREATE_MUTATION,
                  create_variables(args.branch, body))
        _show(out, "if an open issue exists: mutation", COMMENT_MUTATION,
              comment_variables("<open issue id>", body))
        return {"action": "dry-run", "issue": None, "error": None}

    try:
        issue = find_open_issue(post, key)
        if issue:
            print(f"open nightly-failure issue: {issue.get('identifier')} {issue.get('url')}",
                  file=out)
        else:
            print("no open nightly-failure issue", file=out)

        if args.cmd == "green" and not issue:
            return {"action": "none", "issue": None, "error": None}

        if issue:
            mutation, field, variables = (COMMENT_MUTATION, "commentCreate",
                                          comment_variables(issue["id"], body))
            action = "commented"
        else:
            mutation, field, variables = (CREATE_MUTATION, "issueCreate",
                                          create_variables(args.branch, body))
            action = "created"

        if args.dry_run:
            _show(out, f"dry run: would send ({action})", mutation, variables)
            return {"action": "dry-run", "issue": issue, "error": None}

        payload = mutate(post, key, mutation, field, variables)
        if action == "created":
            issue = payload.get("issue") or {}
            print(f"created {issue.get('identifier')} {issue.get('url')}", file=out)
        else:
            print(f"commented on {issue.get('identifier')}", file=out)
        return {"action": action, "issue": issue, "error": None}
    except LinearError as e:
        return {"action": "error", "issue": None, "error": f"Linear nightly issue: {e}"}


def parse_args(argv: list[str] | None = None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    for name in ("fail", "green"):
        p = sub.add_parser(name)
        p.add_argument("--run-url", required=True)
        p.add_argument("--branch", required=True)
        p.add_argument("--sha", required=True)
        p.add_argument("--dry-run", action="store_true")
        if name == "fail":
            p.add_argument("--result", default="failure",
                           help="needs.full-run.result (failure, cancelled, ...)")
            p.add_argument("--regress")
            p.add_argument("--qa")
    return ap.parse_args(argv)


def main(argv: list[str] | None = None, env: dict | None = None,
         post: Post = http_post) -> int:
    res = run(parse_args(argv), env=env, post=post)
    if res["error"]:
        print("::warning::" + " ".join(res["error"].split()))
    return 0


if __name__ == "__main__":
    sys.exit(main())
