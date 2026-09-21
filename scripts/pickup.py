#!/usr/bin/env python3
"""Issue pickup command: resolve branch point, build, reproduce, record evidence.

Wraps the existing Docker wrapper (``scripts/dev-container.sh``) and the existing
harness rather than reimplementing their behavior. The command collects
evidence; causal interpretation and specification decisions stay with the agent.
A completed run is evidence about a reproduction, never a correctness or
landing-gate pass.

Spec: docs/specifications/issue-pickup.spec.md (CORE-238).

Usage::

    python3 scripts/pickup.py \\
        --worktree ~/workspace/typeanvil.worktrees/core-238 \\
        --issue CORE-238 \\
        --wpt-id css-page/margin-boxes/content-003-print.html \\
        [--release-ref release/2026.9] [--source-commit <sha>] \\
        [--html path/to/repro.html] [--out probe/issue-evidence] \\
        [--allow-dirty] [--no-engine] [--wpt /main/.wpt]

Exit codes: 0 success, 1 reproduction/build failure, 2 usage or environment
error.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import shlex
import subprocess
import sys
import time
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path

PICKUP_SCHEMA = "typeanvil.pickup/1"
DEFAULT_RELEASE_REF = "release/2026.9"
DEFAULT_OUT = "probe/issue-evidence"
DEFAULT_WPT = "/main/.wpt"
BINARY_IN_CONTAINER = "/work/engine/target/debug/typeanvil"

#: Outcome classification. A completed run is never a gate pass.
OUTCOMES = (
    "reproduced_expected_failure",
    "passing_reproduction",
    "build_failed",
    "crash",
    "missing_dependency",
    "empty_selection",
    "docs_only",
)


class PickupError(Exception):
    """A usage or environment error (exit 2)."""


# ---------------------------------------------------------------------------
# Pure logic (unit-testable without Docker)
# ---------------------------------------------------------------------------


@dataclass
class Evidence:
    """The evidence record for one pickup run."""

    issue: str
    outcome: str
    source: dict = field(default_factory=dict)
    binary: dict = field(default_factory=dict)
    wpt: dict = field(default_factory=dict)
    environment: dict = field(default_factory=dict)
    command: str = ""
    timestamps: dict = field(default_factory=dict)

    def to_dict(self) -> dict:
        return {
            "schema": PICKUP_SCHEMA,
            "issue": self.issue,
            "outcome": self.outcome,
            "source": self.source,
            "binary": self.binary,
            "wpt": self.wpt,
            "environment": self.environment,
            "command": self.command,
            "timestamps": self.timestamps,
        }


def classify_outcome(
    *,
    build_ok: bool,
    run_ok: bool,
    selected: int,
    statuses: list[str] | None = None,
    render_error: str | None = None,
) -> str:
    """Classify a run into one of the :data:`OUTCOMES`.

    ``statuses`` is the list of WPT statuses from the run report (PASS/FAIL/
    ERROR/SKIP). ``render_error`` is set when the engine crashed on render.
    """
    if not build_ok:
        return "build_failed"
    if selected == 0:
        return "empty_selection"
    if render_error:
        return "crash"
    if not run_ok:
        return "missing_dependency"
    statuses = statuses or []
    if any(s == "ERROR" for s in statuses):
        return "crash"
    if any(s == "FAIL" for s in statuses):
        return "reproduced_expected_failure"
    if any(s == "PASS" for s in statuses):
        return "passing_reproduction"
    return "empty_selection"


def validate_wpt_selection(exact_id: str, available_ids: list[str]) -> list[str]:
    """Return the tests matching ``exact_id``, rejecting empty or broad matches.

    ``available_ids`` is the full list of test ids from the manifest. An exact
    match is accepted. A substring that matches zero tests raises
    :class:`PickupError` (empty). A substring that matches more than one test
    raises :class:`PickupError` (unexpectedly broad).
    """
    if exact_id in available_ids:
        return [exact_id]
    matches = [t for t in available_ids if exact_id in t]
    if not matches:
        raise PickupError(
            f"WPT selection {exact_id!r} matches zero tests. "
            "Use an exact test id from the manifest."
        )
    if len(matches) > 1:
        raise PickupError(
            f"WPT selection {exact_id!r} is unexpectedly broad: matches "
            f"{len(matches)} tests ({', '.join(matches[:5])}{'...' if len(matches) > 5 else ''}). "
            "Use an exact test id."
        )
    return matches


def binary_identity(binary_path: str | None) -> dict:
    """Identify the engine binary by resolved path, sha256, and --version."""
    unknown = {"path": "unknown", "sha256": "unknown", "version": "unknown"}
    if not binary_path:
        return dict(unknown)
    path = Path(binary_path).expanduser()
    resolved = str(path.resolve()) if path.exists() else binary_path
    sha256 = "unknown"
    try:
        sha256 = hashlib.sha256(Path(resolved).read_bytes()).hexdigest()
    except OSError:
        pass
    version = "unknown"
    try:
        proc = subprocess.run(
            [resolved, "--version"], capture_output=True, text=True, timeout=5.0
        )
        out = (proc.stdout or proc.stderr or "").strip()
        if out:
            version = out.splitlines()[0]
    except Exception:  # noqa: BLE001 -- a failed probe yields "unknown"
        pass
    return {"path": resolved, "sha256": sha256, "version": version}


def git_head(repo_root: Path) -> str:
    """Return ``git rev-parse HEAD`` in ``repo_root``, or ``unknown``."""
    try:
        proc = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=repo_root,
            capture_output=True,
            text=True,
            timeout=10.0,
        )
        head = (proc.stdout or "").strip()
        if proc.returncode == 0 and head:
            return head
    except Exception:  # noqa: BLE001
        pass
    return "unknown"


def git_dirty(repo_root: Path) -> tuple[bool, str]:
    """Return ``(is_dirty, porcelain_output)`` for a worktree."""
    try:
        proc = subprocess.run(
            ["git", "status", "--porcelain"],
            cwd=repo_root,
            capture_output=True,
            text=True,
            timeout=10.0,
        )
        out = (proc.stdout or "").strip()
        return bool(out), out
    except Exception:  # noqa: BLE001
        return True, "unknown"


def resolve_release_sha(repo_root: Path, release_ref: str) -> str:
    """Resolve ``origin/<release_ref>`` to a SHA after fetching origin."""
    try:
        subprocess.run(
            ["git", "fetch", "origin"],
            cwd=repo_root,
            capture_output=True,
            text=True,
            timeout=60.0,
            check=True,
        )
    except Exception as exc:  # noqa: BLE001
        raise PickupError(f"git fetch failed: {exc}") from exc
    ref = f"origin/{release_ref}"
    try:
        proc = subprocess.run(
            ["git", "rev-parse", ref],
            cwd=repo_root,
            capture_output=True,
            text=True,
            timeout=10.0,
        )
        sha = (proc.stdout or "").strip()
        if proc.returncode != 0 or not sha:
            raise PickupError(f"release ref {ref!r} does not resolve")
        return sha
    except PickupError:
        raise
    except Exception as exc:  # noqa: BLE001
        raise PickupError(f"cannot resolve {ref!r}: {exc}") from exc


def evidence_dir(out_root: Path, issue: str, commit: str) -> Path:
    """Return a fresh, non-overwriting evidence directory for this run."""
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
    d = out_root / issue / f"{commit}.{stamp}"
    d.mkdir(parents=True, exist_ok=False)
    return d


# ---------------------------------------------------------------------------
# Orchestration (calls dev-container.sh + harness)
# ---------------------------------------------------------------------------


def _run(cmd: list[str], *, cwd: Path, log_path: Path) -> tuple[int, str]:
    """Run ``cmd``, teeing stdout+stderr to ``log_path``; return (exit, log)."""
    with log_path.open("w") as log:
        proc = subprocess.run(
            cmd,
            cwd=cwd,
            stdout=log,
            stderr=subprocess.STDOUT,
            text=True,
        )
    return proc.returncode, log_path.read_text()


def _build(worktree: Path, log_path: Path) -> tuple[bool, str]:
    """Build the engine in the container; return (ok, log)."""
    cmd = [
        "scripts/dev-container.sh",
        "cargo",
        "build",
        "--manifest-path",
        "/work/engine/Cargo.toml",
        "-j",
        "2",
    ]
    code, log = _run(cmd, cwd=worktree, log_path=log_path)
    return code == 0, log


def _work_rel(worktree: Path, path: Path) -> str:
    """Return ``path`` (absolute) relative to ``worktree``, for /work paths."""
    return path.resolve().relative_to(worktree.resolve()).as_posix()


def _run_harness(
    worktree: Path,
    *,
    wpt: str,
    filter_str: str,
    evidence: Path,
    cli_cmd: str,
) -> tuple[bool, str]:
    """Run the harness ``run`` command in the container; return (ok, log)."""
    ev_rel = _work_rel(worktree, evidence)
    cmd = [
        "scripts/dev-container.sh",
        "python3",
        "-m",
        "harness",
        "--wpt",
        wpt,
        "run",
        "--engine",
        "cli",
        "--cli-cmd",
        cli_cmd,
        "--filter",
        filter_str,
        "--workers",
        "1",
        "--report",
        f"/work/{ev_rel}/run.json",
        "--db",
        f"/work/{ev_rel}/history.sqlite",
        "--artifacts",
        f"/work/{ev_rel}/artifacts",
    ]
    code, log = _run(cmd, cwd=worktree, log_path=evidence / "run.log")
    return code == 0, log


def _render_html(
    worktree: Path,
    *,
    html: Path,
    evidence: Path,
    cli_cmd: str,
) -> tuple[bool, str]:
    """Render a single HTML input through the CLI engine; return (ok, error)."""
    out_pdf = evidence / "repro.pdf"
    cmd = [
        "scripts/dev-container.sh",
        "bash",
        "-c",
        (
            f"{cli_cmd} /work/{_work_rel(worktree, html)} "
            f"--page-width 5in --page-height 3in "
            f"--margin-top 0.5in --margin-right 0.5in "
            f"--margin-bottom 0.5in --margin-left 0.5in "
            f"-o /work/{_work_rel(worktree, out_pdf)}"
        ),
    ]
    code, log = _run(cmd, cwd=worktree, log_path=evidence / "run.log")
    if code != 0:
        return False, log
    if not out_pdf.exists():
        return False, "engine produced no output PDF"
    return True, ""


def _read_statuses(report_path: Path) -> list[str]:
    """Read WPT statuses from a harness report JSON."""
    try:
        data = json.loads(report_path.read_text())
    except (OSError, json.JSONDecodeError):
        return []
    results = data.get("results") if isinstance(data, dict) else None
    if not isinstance(results, list):
        return []
    return [str(r.get("status", "")) for r in results if isinstance(r, dict)]


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        prog="pickup", description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("--worktree", required=True, help="path to the worktree")
    parser.add_argument("--release-ref", default=DEFAULT_RELEASE_REF)
    parser.add_argument("--source-commit", default=None, help="explicit source SHA override")
    parser.add_argument("--issue", required=True, help="issue identifier, e.g. CORE-238")
    parser.add_argument("--html", default=None, help="concrete HTML input to render")
    parser.add_argument("--wpt-id", default=None, help="exact expected WPT test ID")
    parser.add_argument("--out", default=DEFAULT_OUT, help="evidence root directory")
    parser.add_argument("--allow-dirty", action="store_true")
    parser.add_argument("--no-engine", action="store_true", help="docs-only: skip build+repro")
    parser.add_argument("--wpt", default=DEFAULT_WPT, help="WPT checkout path")
    args = parser.parse_args(argv)

    if bool(args.html) == bool(args.wpt_id):
        parser.error("exactly one of --html or --wpt-id is required")
    if args.no_engine and args.html:
        parser.error("--no-engine cannot be combined with --html")

    worktree = Path(args.worktree).expanduser().resolve()
    if not worktree.is_dir():
        print(f"error: worktree does not exist: {worktree}", file=sys.stderr)
        return 2

    started = datetime.now(timezone.utc).isoformat()

    # A. Resolve branch point + check ownership/dirty state.
    try:
        sha = args.source_commit or resolve_release_sha(worktree, args.release_ref)
    except PickupError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    head = git_head(worktree)
    if head != "unknown" and head != sha:
        print(
            f"error: worktree HEAD {head} != resolved SHA {sha}. "
            "Refusing to reuse a stale tree.",
            file=sys.stderr,
        )
        return 2

    dirty, porcelain = git_dirty(worktree)
    if dirty and not args.allow_dirty:
        print(
            f"error: worktree is dirty (use --allow-dirty to proceed):\n{porcelain}",
            file=sys.stderr,
        )
        return 2

    # D. Evidence directory (non-overwriting).
    try:
        ev = evidence_dir(Path(args.out), args.issue, sha)
    except FileExistsError:
        print("error: evidence directory already exists; retry", file=sys.stderr)
        return 2

    command = " ".join(shlex.quote(a) for a in sys.argv[1:])
    evidence = Evidence(
        issue=args.issue,
        outcome="empty_selection",
        source={
            "commit": sha,
            "dirty": dirty,
            "patch_identity": porcelain if dirty else None,
        },
        binary=binary_identity(BINARY_IN_CONTAINER),
        wpt={"revision": "unknown", "selection": []},
        environment={
            "container": "typeanvil-dev",
            "fonts": "unknown",
            "page_spec": {
                "width_in": 5.0,
                "height_in": 3.0,
                "margin_top_in": 0.5,
                "margin_right_in": 0.5,
                "margin_bottom_in": 0.5,
                "margin_left_in": 0.5,
            },
            "dpi": 96,
            "rasterizer": "pypdfium2 (unknown)",
        },
        command=command,
        timestamps={"started": started},
    )

    # E. Documentation-only: validate the named selection (cheap), record
    # identity, and stop. No build, no reproduction.
    if args.no_engine:
        if args.wpt_id:
            try:
                from harness.manifest import enumerate_tests
                from harness.wpt_fetch import default_wpt_dir

                host_wpt = default_wpt_dir(worktree)
                if not host_wpt.is_dir():
                    host_wpt = Path(args.wpt)
                available = [t.id for t in enumerate_tests(host_wpt)]
                selected = validate_wpt_selection(args.wpt_id, available)
                evidence.wpt["selection"] = selected
            except PickupError as exc:
                print(f"error: {exc}", file=sys.stderr)
                return 2
            except Exception as exc:  # noqa: BLE001 -- manifest enumeration failure
                print(f"error: cannot enumerate WPT manifest: {exc}", file=sys.stderr)
                return 2
        evidence.outcome = "docs_only"
        evidence.timestamps["finished"] = datetime.now(timezone.utc).isoformat()
        (ev / "evidence.json").write_text(json.dumps(evidence.to_dict(), indent=2))
        print(f"docs-only: identity recorded at {ev}")
        return 0

    # B. Build in the container.
    build_ok, build_log = _build(worktree, ev / "build.log")
    (ev / "build.exit").write_text("0" if build_ok else "1")
    evidence.timestamps["build_finished"] = datetime.now(timezone.utc).isoformat()
    if not build_ok:
        evidence.outcome = "build_failed"
        evidence.timestamps["finished"] = datetime.now(timezone.utc).isoformat()
        (ev / "evidence.json").write_text(json.dumps(evidence.to_dict(), indent=2))
        print(f"build_failed: inspect {ev / 'build.log'}")
        return 1

    # C. Run the smallest reproduction.
    cli_cmd = f"{BINARY_IN_CONTAINER} render"
    selected: list[str] = []
    run_ok = False
    render_error: str | None = None
    statuses: list[str] = []

    if args.wpt_id:
        # Validate selection against the manifest (host-side, exact IDs).
        try:
            from harness.manifest import enumerate_tests
            from harness.wpt_fetch import default_wpt_dir

            # Host-side enumeration uses the worktree's .wpt symlink; the
            # container run uses args.wpt (/main/.wpt by default).
            host_wpt = default_wpt_dir(worktree)
            if not host_wpt.is_dir():
                host_wpt = Path(args.wpt)
            available = [t.id for t in enumerate_tests(host_wpt)]
            selected = validate_wpt_selection(args.wpt_id, available)
        except PickupError as exc:
            print(f"error: {exc}", file=sys.stderr)
            return 2
        except Exception as exc:  # noqa: BLE001 -- manifest enumeration failure
            print(f"error: cannot enumerate WPT manifest: {exc}", file=sys.stderr)
            return 2
        evidence.wpt["selection"] = selected
        run_ok, _ = _run_harness(
            worktree, wpt=args.wpt, filter_str=args.wpt_id, evidence=ev, cli_cmd=cli_cmd
        )
        statuses = _read_statuses(ev / "run.json")
    else:
        html = Path(args.html).expanduser().resolve()
        if not html.is_file():
            print(f"error: HTML input does not exist: {html}", file=sys.stderr)
            return 2
        selected = [str(html)]
        evidence.wpt["selection"] = selected
        run_ok, render_error = _render_html(worktree, html=html, evidence=ev, cli_cmd=cli_cmd)

    (ev / "run.exit").write_text("0" if run_ok else "1")
    evidence.outcome = classify_outcome(
        build_ok=build_ok,
        run_ok=run_ok,
        selected=len(selected),
        statuses=statuses,
        render_error=render_error,
    )
    evidence.timestamps["finished"] = datetime.now(timezone.utc).isoformat()
    (ev / "evidence.json").write_text(json.dumps(evidence.to_dict(), indent=2))

    print(f"outcome: {evidence.outcome}")
    print(f"evidence: {ev}")
    return 0 if evidence.outcome in ("passing_reproduction", "reproduced_expected_failure") else 1


if __name__ == "__main__":
    raise SystemExit(main())
