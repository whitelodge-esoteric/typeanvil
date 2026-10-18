#!/usr/bin/env python3
"""Helpers for the automated release cycle (CORE-499, .github/workflows/release-cut.yml).

  release_cut.py next-version BRANCH TAG...   print the next vYYYY.M.PATCH for BRANCH
  release_cut.py rotation-names [YYYY-M]      print "NEW PREV" branch names for a month
  release_cut.py notes --prs PRS.json [--version V] [--showcase URL]
                                              print release notes grouped by label
  release_cut.py verify-sums SUMS FILE...     check files against a SHA256SUMS file

Pure functions are unit-tested in tests/test_release_cut.py.
Spec: docs/specifications/release-cycle.spec.md
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from datetime import date
from pathlib import Path

BRANCH_RE = re.compile(r"^(?:origin/)?release/(\d+)\.(\d+)$")
TAG_RE = re.compile(r"^v(\d+)\.(\d+)\.(\d+)$")

# Label -> notes heading, in display order. Unlabelled PRs land under "Other".
SECTIONS = [
    ("engine", "Engine"),
    ("harness", "Harness"),
    ("docs", "Documentation"),
    ("tooling", "Tooling"),
    ("demo", "Demo and library"),
]
OTHER = "Other"


def branch_month(branch: str) -> tuple[int, int]:
    m = BRANCH_RE.match(branch.strip())
    if not m:
        raise ValueError(f"not a release branch: {branch!r}")
    return int(m.group(1)), int(m.group(2))


def next_version(branch: str, tags: list[str]) -> str:
    """Next vYYYY.M.PATCH for the branch's month. Plain month, patch from 0."""
    year, month = branch_month(branch)
    patches = []
    for t in tags:
        m = TAG_RE.match(t.strip())
        if m and (int(m.group(1)), int(m.group(2))) == (year, month):
            patches.append(int(m.group(3)))
    patch = max(patches) + 1 if patches else 0
    return f"v{year}.{month}.{patch}"


def rotation_names(year: int, month: int) -> tuple[str, str]:
    """(new branch, previous branch) for the calendar month. Month is not padded."""
    if month == 1:
        prev = (year - 1, 12)
    else:
        prev = (year, month - 1)
    return f"release/{year}.{month}", f"release/{prev[0]}.{prev[1]}"


def group_prs(prs: list[dict]) -> list[tuple[str, list[dict]]]:
    """Group merged PRs by the first matching known label, keeping section order."""
    buckets: dict[str, list[dict]] = {h: [] for _, h in SECTIONS}
    buckets[OTHER] = []
    for pr in prs:
        labels = {
            (l["name"] if isinstance(l, dict) else l).lower()
            for l in pr.get("labels", [])
        }
        for label, heading in SECTIONS:
            if label in labels:
                buckets[heading].append(pr)
                break
        else:
            buckets[OTHER].append(pr)
    order = [h for _, h in SECTIONS] + [OTHER]
    return [(h, buckets[h]) for h in order if buckets[h]]


def render_notes(
    prs: list[dict], version: str = "", showcase: str = "", compare: str = ""
) -> str:
    out = []
    if version:
        out.append(f"# Typeanvil {version.lstrip('v')}\n")
    if not prs:
        out.append("No merged pull requests since the previous release.\n")
    for heading, items in group_prs(prs):
        out.append(f"## {heading}\n")
        for pr in sorted(items, key=lambda p: p["number"]):
            out.append(f"- {pr['title']} (#{pr['number']})")
        out.append("")
    if showcase:
        out.append(f"Showcase: {showcase}\n")
    if compare:
        out.append(f"Full changelog: {compare}\n")
    return "\n".join(out).rstrip() + "\n"


def parse_sums(text: str) -> dict[str, str]:
    sums = {}
    for line in text.splitlines():
        parts = line.split(None, 1)
        if len(parts) == 2:
            sums[parts[1].strip().lstrip("*")] = parts[0].lower()
    return sums


def verify_sums(sums_text: str, files: list[Path]) -> list[str]:
    """Return a list of problems; empty means every file matches its entry."""
    sums = parse_sums(sums_text)
    problems = []
    for f in files:
        want = sums.get(f.name)
        if want is None:
            problems.append(f"{f.name}: not listed in SHA256SUMS")
            continue
        got = hashlib.sha256(f.read_bytes()).hexdigest()
        if got != want:
            problems.append(f"{f.name}: expected {want}, got {got}")
    return problems


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="cmd", required=True)

    p = sub.add_parser("next-version")
    p.add_argument("branch")
    p.add_argument("tags", nargs="*")

    p = sub.add_parser("rotation-names")
    p.add_argument("month", nargs="?", help="YYYY-M (default: today)")

    p = sub.add_parser("notes")
    p.add_argument("--prs", required=True)
    p.add_argument("--version", default="")
    p.add_argument("--showcase", default="")
    p.add_argument("--compare", default="")

    p = sub.add_parser("verify-sums")
    p.add_argument("sums")
    p.add_argument("files", nargs="+")

    a = ap.parse_args(argv)
    if a.cmd == "next-version":
        print(next_version(a.branch, a.tags))
    elif a.cmd == "rotation-names":
        if a.month:
            y, m = a.month.split("-")
            y, m = int(y), int(m)
        else:
            t = date.today()
            y, m = t.year, t.month
        new, prev = rotation_names(y, m)
        print(new, prev)
    elif a.cmd == "notes":
        prs = json.loads(Path(a.prs).read_text())
        sys.stdout.write(render_notes(prs, a.version, a.showcase, a.compare))
    elif a.cmd == "verify-sums":
        problems = verify_sums(Path(a.sums).read_text(), [Path(f) for f in a.files])
        for line in problems:
            print(line, file=sys.stderr)
        if problems:
            return 1
        print(f"sums ok: {len(a.files)} file(s)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
