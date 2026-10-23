#!/usr/bin/env python3
"""Tagged PDF gate: render every corpus fixture tagged, read the
structure back, and fail on tagging regressions.

Spec: docs/specifications/tagged-gate.spec.md. Runbook:
docs/operations/tagged-gate.md. Issue: CORE-377.

    python3 scripts/tagged_gate.py --bin <typeanvil> [--update] [--report out.json]

Exit 0 = TAGGED GATE PASSED, 1 = TAGGED GATE FAILED, 2 = usage/environment.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
import tempfile
from dataclasses import dataclass, field
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import verify_tagged_pdf  # noqa: E402  (sibling script, not a package)

CHECKS = ("render", "deterministic", "untagged", "duplicated", "dangling", "ua", "roles", "coverage")
COVERAGE_TOLERANCE = 0.005
CORPUS = "corpus"  # corpus/manifest.json lists every gated document (CORE-488)
DATA = Path("harness/tagged-gate")


@dataclass
class GateResult:
    failures: list[tuple[str, str, str]] = field(default_factory=list)
    expected: list[dict] = field(default_factory=list)

    @property
    def passed(self) -> bool:
        return not self.failures


def _failed_checks(r: dict, snap: dict | None) -> dict[str, str]:
    """Check name → detail for every check this fixture fails."""
    if r.get("render_exit", 0) != 0:
        return {"render": f"tagged render exited {r.get('render_exit')}"}
    t = r["totals"]
    out = {}
    if not r["deterministic"]:
        out["deterministic"] = "two --tagged renders differ"
    for check, key in (("untagged", "untagged"), ("duplicated", "mcids_duplicated"),
                       ("dangling", "mcids_dangling")):
        if t[key]:
            out[check] = f"{key} = {t[key]}"
    if r["ua_exit"] != 0:
        out["ua"] = r.get("ua_err") or f"--tagged --ua exited {r['ua_exit']}"
    if snap is not None:
        if r["roles"] != snap["roles"]:
            keys = sorted(set(r["roles"]) | set(snap["roles"]))
            delta = {k: (snap["roles"].get(k, 0), r["roles"].get(k, 0)) for k in keys
                     if snap["roles"].get(k, 0) != r["roles"].get(k, 0)}
            out["roles"] = "snapshot→now " + ", ".join(f"{k} {a}→{b}" for k, (a, b) in delta.items())
        if r["coverage"] < snap["coverage"] - COVERAGE_TOLERANCE:
            out["coverage"] = f"{snap['coverage']:.3f}→{r['coverage']:.3f}"
    return out


def evaluate(results: dict[str, dict], snapshot: dict, known: list[dict]) -> GateResult:
    res = GateResult()
    known_by = {(k["doc"], k["check"]): k for k in known}
    for k in known:
        if k["check"] not in CHECKS:
            res.failures.append((k["doc"], "invalid-known-failure", f"unknown check {k['check']!r}"))
    for doc in sorted(set(snapshot) - set(results)):
        res.failures.append((doc, "snapshot-stale", "snapshot entry for a fixture no manifest lists"))
    for doc in sorted(results):
        snap = snapshot.get(doc)
        if snap is None:
            res.failures.append((doc, "snapshot-missing", "run with --update"))
        failed = _failed_checks(results[doc], snap)
        for check, detail in failed.items():
            if (doc, check) in known_by:
                res.expected.append(known_by[(doc, check)])
            else:
                res.failures.append((doc, check, detail))
        for (kdoc, kcheck), k in known_by.items():
            if kdoc == doc and kcheck in CHECKS and kcheck not in failed:
                res.failures.append((doc, "stale-known-failure",
                                     f"{kcheck} now passes; remove the entry ({k['ticket']})"))
    res.failures.sort()
    return res


def _render(binary: Path, cwd: Path, f: str, geom: str, extra: list[str], out: Path) -> tuple[int, str]:
    p = subprocess.run([str(binary), "render", f, *geom.split(), *extra, "-o", str(out)],
                       cwd=cwd, capture_output=True, text=True, timeout=600)
    err = p.stderr.strip().splitlines()
    return p.returncode, (err[-1] if err else "")[-300:]


def measure(repo: Path, binary: Path, work: Path) -> dict[str, dict]:
    results = {}
    lib = repo / CORPUS
    for m in json.loads((lib / "manifest.json").read_text()):
        rel = m["file"]  # <category>/<doc>.html under corpus/
        g = m["geometry"]
        geom = (f"--page-width {g['width']} --page-height {g['height']} "
                f"--margin-top {g['margin']} --margin-right {g['margin']} "
                f"--margin-bottom {g['margin']} --margin-left {g['margin']}")
        d, f = (lib / rel).parent, Path(rel).name
        doc, stem = f"{CORPUS}/{rel}", f"{CORPUS}-{rel.replace('/', '-')[:-5]}"
        a, b, ua = (work / f"{stem}.{s}.pdf" for s in ("a", "b", "ua"))
        r = {}
        r["render_exit"], _ = _render(binary, d, f, geom, ["--tagged"], a)
        if r["render_exit"] == 0:
            _render(binary, d, f, geom, ["--tagged"], b)
            r["deterministic"] = b.exists() and hashlib.sha256(a.read_bytes()).digest() == hashlib.sha256(b.read_bytes()).digest()
            r["ua_exit"], r["ua_err"] = _render(binary, d, f, geom, ["--tagged", "--ua"], ua)
            rep = verify_tagged_pdf.analyse(str(a), str(d / f))
            r["totals"] = rep["totals"]
            r["roles"] = rep["roles"]
            r["coverage"] = round(rep["order"]["source_coverage"], 4)
        results[doc] = r
        print(f"  {doc:40} " + ("ok" if r["render_exit"] == 0 else f"render exit {r['render_exit']}"),
              flush=True)
    return results


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description="Tagged PDF gate (CORE-377).")
    ap.add_argument("--bin", required=True, type=Path, help="typeanvil engine binary")
    ap.add_argument("--repo", type=Path, default=Path(__file__).resolve().parent.parent)
    ap.add_argument("--update", action="store_true", help="rewrite the snapshot from this run")
    ap.add_argument("--report", type=Path, help="write the full result as JSON")
    ap.add_argument("--keep", type=Path, help="keep rendered PDFs in this directory")
    a = ap.parse_args(argv)
    repo = a.repo.resolve()
    snap_path, known_path = repo / DATA / "snapshot.json", repo / DATA / "known-failures.json"
    if not a.bin.is_file():
        print(f"error: no engine binary at {a.bin}", file=sys.stderr)
        return 2
    try:
        known = json.loads(known_path.read_text())
        snapshot = json.loads(snap_path.read_text()) if snap_path.exists() else {}
    except (OSError, json.JSONDecodeError) as e:
        print(f"error: {e}", file=sys.stderr)
        return 2
    with tempfile.TemporaryDirectory() as tmp:
        work = a.keep.resolve() if a.keep else Path(tmp)
        work.mkdir(parents=True, exist_ok=True)
        print(f"tagged gate: {a.bin} over {repo}")
        results = measure(repo, a.bin.resolve(), work)
    if a.update:
        snapshot = {doc: {"roles": r["roles"], "coverage": r["coverage"]}
                    for doc, r in sorted(results.items()) if r["render_exit"] == 0}
        snap_path.write_text(json.dumps(snapshot, indent=1, sort_keys=True) + "\n")
        print(f"snapshot updated: {len(snapshot)} fixtures → {snap_path.relative_to(repo)}")
    res = evaluate(results, snapshot, known)
    for k in res.expected:
        print(f"  expected  {k['doc']:40} {k['check']:<13} {k['ticket']}: {k['reason']}")
    for doc, check, detail in res.failures:
        print(f"  FAIL      {doc:40} {check:<13} {detail}")
    if a.report:
        a.report.write_text(json.dumps({
            "passed": res.passed, "failures": res.failures, "expected": res.expected,
            "results": results}, indent=1, ensure_ascii=False))
    print(f"{len(results)} fixtures, {len(res.expected)} expected failures, {len(res.failures)} unexpected")
    print("TAGGED GATE PASSED" if res.passed else "TAGGED GATE FAILED")
    return 0 if res.passed else 1


if __name__ == "__main__":
    sys.exit(main())
