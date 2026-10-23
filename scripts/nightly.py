#!/usr/bin/env python3
"""Helpers for the nightly full run (CORE-490, .github/workflows/nightly.yml).

  nightly.py latest-release BRANCH...   print the newest release/YYYY.M name
  nightly.py summary --regress LOG [--qa REPORT.json] [--status S]
                                         print the Markdown job summary

Pure functions are unit-tested in tests/test_nightly.py.
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

RELEASE_RE = re.compile(r"^(?:origin/)?(release/(\d+)\.(\d+))$")
TIMING_RE = re.compile(r"^(?:timing|unbaselined) (\S+): (\d+) page\(s\) in ([\d.]+)s$")


def latest_release(branches: list[str]) -> str | None:
    """Newest release branch by numeric (year, month) order, not string order."""
    best = None
    for b in branches:
        m = RELEASE_RE.match(b.strip())
        if m:
            key = (int(m.group(2)), int(m.group(3)))
            if best is None or key > best[0]:
                best = (key, m.group(1))
    return best[1] if best else None


def parse_regress(text: str) -> dict:
    """Split `regress.py check` output into timings and changed-document rows."""
    timings, changes = [], []
    in_table = False
    for line in text.splitlines():
        m = TIMING_RE.match(line)
        if m:
            timings.append((m.group(1), int(m.group(2)), float(m.group(3))))
            continue
        if line.startswith("document ") and "kind" in line:
            in_table = True
            continue
        if in_table and line.startswith("regress:"):
            in_table = False
            continue
        if in_table and line.strip():
            changes.append(line.rstrip())
    return {"timings": timings, "changes": changes}


def qa_failures(report: dict) -> list[str]:
    """Names of fixtures that failed visual QA. Tolerates unknown report shapes."""
    out = []
    fixtures = report.get("fixtures", report.get("results", []))
    if isinstance(fixtures, dict):
        fixtures = [dict(v, name=k) for k, v in fixtures.items()]
    for f in fixtures:
        if not isinstance(f, dict):
            continue
        ok = f.get("ok", f.get("passed", f.get("status") in ("ok", "pass", "passed")))
        if not ok:
            out.append(str(f.get("name") or f.get("file") or f.get("fixture") or "?"))
    return out


def summary(regress_text: str, qa: dict | None, status: str) -> str:
    r = parse_regress(regress_text)
    total = sum(t for _, _, t in r["timings"])
    lines = [f"## Nightly full run: {status}", ""]
    lines.append(f"Documents rendered: {len(r['timings'])}, total {total:.1f}s.")
    lines.append("")
    lines.append("### Changed documents")
    if r["changes"]:
        lines += ["```", *r["changes"], "```"]
    else:
        lines.append("None.")
    lines.append("")
    lines.append("### Visual QA")
    if qa is None:
        lines.append("No report (the step did not finish).")
    else:
        new = qa.get("new_failures", [])
        stale = qa.get("stale_known_failures", [])
        known = qa.get("known_failures", [])
        if "new_failures" in qa:
            if not new and not stale:
                lines.append("No new failures.")
            if new:
                lines.append("New failures: " + ", ".join(
                    f"{f['file']} p{f['page']} {f['check']}" for f in new))
            if stale:
                lines.append("Stale known failures (now pass; remove them): " + ", ".join(
                    f"{e['file']} p{e['page']} {e['check']} ({e['issue']})" for e in stale))
            if known:
                lines += ["", "Known failures (tracked, not blocking):"]
                lines += [f"- {e['file']} page {e['page']} {e['check']}: {e['issue']}"
                          for e in known]
        else:
            bad = qa_failures(qa)
            lines.append("All fixtures pass." if not bad else "Failed: " + ", ".join(bad))
    lines.append("")
    lines.append("### Slowest documents")
    lines.append("| document | pages | seconds |")
    lines.append("|---|---|---|")
    for f, p, t in sorted(r["timings"], key=lambda x: -x[2])[:10]:
        lines.append(f"| {f} | {p} | {t:.1f} |")
    return "\n".join(lines) + "\n"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("latest-release")
    p.add_argument("branches", nargs="*")
    p = sub.add_parser("summary")
    p.add_argument("--regress", required=True)
    p.add_argument("--qa")
    p.add_argument("--status", default="unknown")
    a = ap.parse_args()
    if a.cmd == "latest-release":
        names = a.branches or sys.stdin.read().split()
        best = latest_release(names)
        if not best:
            print("error: no release/* branch", file=sys.stderr)
            return 2
        print(best)
        return 0
    rp = Path(a.regress)
    text = rp.read_text(encoding="utf-8") if rp.exists() else ""
    qa = None
    if a.qa and Path(a.qa).exists():
        qa = json.loads(Path(a.qa).read_text(encoding="utf-8"))
    sys.stdout.write(summary(text, qa, a.status))
    return 0


if __name__ == "__main__":
    sys.exit(main())
