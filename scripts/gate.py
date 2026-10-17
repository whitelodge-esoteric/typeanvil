#!/usr/bin/env python3
"""Run the CI checks locally and write a verifiable receipt (CORE-479).

Runs the same steps as .github/workflows/ci.yml (clippy, cargo test, harness
pytest). Rust runs in the dev container (host cargo builds are banned: OOM
guard). Harness tests run on the host with the main checkout venv.

The gate refuses to be fooled:
  * the tree shall be clean, so the receipt describes a commit;
  * every step runs with its real exit code (no shell pipes);
  * a test step that ran zero tests fails;
  * a filtered run (GATE_TEST_FILTER) is recorded as filtered and never passes.

Usage:
  python3 scripts/gate.py                 run the gate, write .gate/result.json
  python3 scripts/gate.py --verify [path] check a receipt against HEAD's tree

Exit codes: 0 pass, 1 fail, 2 refused (dirty tree, no python, bad usage).
Env: PY (harness python), TYPEANVIL_MAIN (main checkout), GATE_TEST_FILTER.
Runbook: docs/operations/ci.md
"""
from __future__ import annotations

import datetime
import json
import os
import re
import subprocess
import sys
from pathlib import Path

SCHEMA = "typeanvil.gate/1"
ROOT = Path(__file__).resolve().parent.parent
GATE_DIR = ROOT / ".gate"
RECEIPT = GATE_DIR / "result.json"
ALLOWED_UNTRACKED = ("probe/", "gate/captures/", ".gate/")

CARGO_RESULT = re.compile(
    r"^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed;(?: (\d+) ignored;)?"
)
PYTEST_COUNT = re.compile(r"(\d+) (passed|failed|skipped|errors?)\b")


def git(*args: str) -> str:
    out = subprocess.run(
        ["git", *args], cwd=ROOT, check=True, capture_output=True, text=True
    )
    return out.stdout.strip()


def dirty_paths() -> list[str]:
    """Porcelain entries that are not allowed untracked scratch paths."""
    out = subprocess.run(
        ["git", "status", "--porcelain", "--untracked-files=all"],
        cwd=ROOT, check=True, capture_output=True, text=True,
    ).stdout
    bad = []
    for line in out.splitlines():
        status, path = line[:2], line[3:]
        if status == "??" and path.startswith(ALLOWED_UNTRACKED):
            continue
        bad.append(line)
    return bad


def parse_cargo(text: str) -> tuple[int, int, int, int]:
    """Return (passed, failed, ignored, result_lines) summed over all binaries."""
    passed = failed = ignored = lines = 0
    for line in text.splitlines():
        m = CARGO_RESULT.match(line)
        if m:
            lines += 1
            passed += int(m.group(2))
            failed += int(m.group(3))
            ignored += int(m.group(4) or 0)
    return passed, failed, ignored, lines


def parse_pytest(text: str) -> tuple[int, int, int] | None:
    """Return (passed, failed+errors, skipped) from the final summary line."""
    for line in reversed(text.splitlines()):
        if not re.search(r" in \d+(\.\d+)?s\b", line) and "no tests ran" not in line:
            continue
        counts = {"passed": 0, "failed": 0, "skipped": 0, "error": 0}
        found = False
        for n, kind in PYTEST_COUNT.findall(line):
            counts["error" if kind.startswith("error") else kind] += int(n)
            found = True
        if found or "no tests ran" in line:
            return counts["passed"], counts["failed"] + counts["error"], counts["skipped"]
    return None


def harness_python() -> str | None:
    main_dir = os.environ.get("TYPEANVIL_MAIN", str(Path.home() / "workspace/typeanvil"))
    for cand in (os.environ.get("PY") or f"{main_dir}/.venv/bin/python",
                 str(ROOT / ".venv/bin/python")):
        if os.access(cand, os.X_OK):
            return cand
    return None


def run_step(name: str, cmd: list[str], kind: str) -> dict:
    log = GATE_DIR / f"{name}.log"
    print(f"=== gate: {name} ===\n+ {' '.join(cmd)}", flush=True)
    with open(log, "wb") as fh:
        exit_code = subprocess.run(
            cmd, cwd=ROOT, stdout=fh, stderr=subprocess.STDOUT, stdin=subprocess.DEVNULL
        ).returncode
    text = log.read_text(errors="replace")
    step = {"name": name, "cmd": cmd, "exit": exit_code,
            "passed": None, "failed": None, "skipped": None,
            "log": str(log.relative_to(ROOT))}
    if kind == "cargo":
        passed, failed, ignored, lines = parse_cargo(text)
        step.update(passed=passed, failed=failed, skipped=ignored)
        step["result_lines"] = lines
    elif kind == "pytest":
        parsed = parse_pytest(text)
        passed, failed, skipped = parsed if parsed else (0, 0, 0)
        step.update(passed=passed, failed=failed, skipped=skipped)
        step["summary_found"] = parsed is not None
    return step


def evaluate(steps: list[dict], filtered: bool) -> list[str]:
    reasons = []
    for s in steps:
        if s["exit"] != 0:
            reasons.append(f"{s['name']}: exit {s['exit']} (see {s['log']})")
        if s["failed"]:
            reasons.append(f"{s['name']}: {s['failed']} failed")
        if s["passed"] is not None and s["passed"] == 0:
            reasons.append(f"{s['name']}: 0 tests passed (nothing ran or no summary found)")
    if filtered:
        reasons.append("filtered run (GATE_TEST_FILTER) is not a gate")
    return reasons


def run_gate() -> int:
    bad = dirty_paths()
    if bad:
        print("gate: tree is dirty — commit first (receipt must describe a commit)",
              file=sys.stderr)
        for line in bad:
            print(f"  {line}", file=sys.stderr)
        return 2
    py = harness_python()
    if py is None:
        print("gate: no harness venv python found (set PY=...)", file=sys.stderr)
        return 2

    head, tree = git("rev-parse", "HEAD"), git("rev-parse", "HEAD^{tree}")
    branch = git("rev-parse", "--abbrev-ref", "HEAD")
    GATE_DIR.mkdir(exist_ok=True)
    RECEIPT.unlink(missing_ok=True)  # a crash mid-run shall not leave an old receipt

    test_filter = os.environ.get("GATE_TEST_FILTER", "")
    filtered = bool(test_filter)
    manifest = "/work/engine/Cargo.toml"
    test_cmd = ["scripts/dev-container.sh", "cargo", "test", "--manifest-path", manifest,
                "-j", "2", "--no-fail-fast"]
    if filtered:
        test_cmd.append(test_filter)

    steps = [
        run_step("clippy", ["scripts/dev-container.sh", "cargo", "clippy",
                            "--manifest-path", manifest, "-j", "2", "--all-targets",
                            "--", "-D", "warnings"], "clippy"),
        run_step("cargo-test", test_cmd, "cargo"),
        # pyproject.toml addopts already holds -q; a second -q drops the summary line.
        run_step("pytest", ["env", "-u", "PYTHONPATH", py, "-m", "pytest", "tests", "-q",
                            "-o", "addopts="], "pytest"),
    ]
    reasons = evaluate(steps, filtered)
    ok = not reasons

    receipt = {
        "schema": SCHEMA, "ok": ok, "filtered": filtered, "head": head, "tree": tree,
        "branch": branch,
        "created_at": datetime.datetime.now(datetime.timezone.utc)
        .strftime("%Y-%m-%dT%H:%M:%SZ"),
        "steps": steps, "reasons": reasons,
    }
    RECEIPT.write_text(json.dumps(receipt, indent=2) + "\n")

    def cell(v: int | None) -> str:
        return "-" if v is None else str(v)

    print(f"\n{'step':<12} {'exit':>4} {'passed':>7} {'failed':>7} {'skipped':>8}")
    for s in steps:
        print(f"{s['name']:<12} {s['exit']:>4} {cell(s['passed']):>7} "
              f"{cell(s['failed']):>7} {cell(s['skipped']):>8}")
    print(f"\nhead {head}\ntree {tree}\nreceipt {RECEIPT.relative_to(ROOT)}")
    print("GATE PASSED" if ok else "GATE FAILED: " + "; ".join(reasons))
    return 0 if ok else 1


def verify(path: str | None) -> int:
    p = Path(path) if path else RECEIPT
    problems = []
    try:
        r = json.loads(p.read_text())
    except FileNotFoundError:
        print(f"gate verify: FAILED: receipt not found: {p}")
        return 1
    except (OSError, ValueError) as e:
        print(f"gate verify: FAILED: receipt unreadable: {p}: {e}")
        return 1
    if not isinstance(r, dict) or r.get("schema") != SCHEMA:
        problems.append(f"schema is not {SCHEMA}")
        r = r if isinstance(r, dict) else {}
    if r.get("ok") is not True:
        problems.append("ok is not true")
    if r.get("filtered") is not False:
        problems.append("filtered is not false")
    current = git("rev-parse", "HEAD^{tree}")
    if r.get("tree") != current:
        problems.append(f"tree mismatch: receipt {r.get('tree')} != current {current}")
    if problems:
        print("gate verify: FAILED: " + "; ".join(problems))
        return 1
    print(f"gate verify: OK (tree {current}, head {r.get('head')})")
    return 0


def main(argv: list[str]) -> int:
    if argv and argv[0] == "--verify":
        if len(argv) > 2:
            print("usage: gate.py [--verify [path]]", file=sys.stderr)
            return 2
        return verify(argv[1] if len(argv) == 2 else None)
    if argv:
        print("usage: gate.py [--verify [path]]", file=sys.stderr)
        return 2
    return run_gate()


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
