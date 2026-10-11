"""Four-way WPT triage: our engine vs Chromium vs PrinceXML vs WeasyPrint.

Runs the same WPT filter through up to four render legs and reports who is the
odd one out on each failing test. The first leg is our engine; the rest are
independent comparison engines. See
``docs/conventions/css-standards-alignment.md`` for when to use it.

Each leg is scored by the harness the same way: it renders the test AND its
reference through that one engine and compares them. A leg PASSES only if that
engine renders the pair identically. So:

  engine FAIL + (several other legs PASS) -> our bug; real engines
                                             satisfy the reference
  engine FAIL + (one other leg PASS)      -> that engine satisfies it, the
                                             rest do not -- check the pair
                                             for contradiction, or check
                                             browser ground truth (wpt.fyi)
  engine FAIL + (no other leg PASS)       -> contradictory pair; no engine
                                             passes both sides
  engine PASS + (all other legs FAIL)     -> suspect two-wrongs-matching;
                                             our pair may agree for the
                                             wrong reason
  all legs PASS                           -> consistent

Chromium is the browser oracle (WPT reftests are browser-authored), Prince and
WeasyPrint are independent print engines. When Chromium disagrees with both
print engines, browser ground truth decides.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

LEGS = ("engine", "chromium", "prince", "weasy")

#: One-line triage reading for a row. Empty string means "nothing to flag".
_FAILED_BOTH = (
    "OUR BUG -- several engines satisfy the reference; our engine is the odd one out"
)
_FAILED_NEITHER = (
    "no engine satisfies the reference -- contradictory or unimplementable"
)
_PASSED_BUT_ALONE = (
    "engine PASSES but ALL other legs disagree -- suspect two-wrongs-matching"
)

_ENGINE_NAMES = {
    "engine": "the engine",
    "chromium": "Chromium",
    "prince": "Prince",
    "weasy": "WeasyPrint",
}


def verdict(leg_status: dict[str, str]) -> str:
    """Return a one-line triage reading, or "" when there is nothing to flag.

    ``leg_status`` maps leg name -> status ("PASS"/"FAIL"/"ERROR"/"?"). Missing
    legs are ignored, so a two-leg run still reads sensibly.
    """
    eng = leg_status.get("engine")
    others = [
        (leg, st) for leg, st in leg_status.items()
        if leg != "engine" and st is not None
    ]

    if eng == "PASS":
        if others and all(st != "PASS" for _, st in others):
            return _PASSED_BUT_ALONE
        return ""
    if not others:
        return ""

    passers = [leg for leg, st in others if st == "PASS"]
    if len(passers) >= 2:
        return _FAILED_BOTH
    if len(others) == 1:
        # Only one comparison leg available; that leg is the sole witness.
        if others[0][1] == "PASS":
            return _FAILED_BOTH.replace(
                "several engines satisfy",
                "the single comparison engine satisfies")
        return ""
    if len(passers) == 1:
        leg = passers[0]
        nonpassers = [n for n, st in others if st != "PASS"]
        who = " and ".join(_ENGINE_NAMES[n] for n in nonpassers)
        aux = "does not" if len(nonpassers) == 1 else "do not"
        if leg == "chromium":
            return (
                f"Chromium satisfies it, {who} {aux} -- "
                "check the pair for contradiction"
            )
        return (
            f"{_ENGINE_NAMES[leg]} satisfies it, {who} {aux} -- "
            "check browser ground truth (wpt.fyi)"
        )
    return _FAILED_NEITHER


def resolve_wpt(repo_root: Path) -> Path:
    """Find the WPT checkout, tolerating a git worktree.

    ``.wpt`` is gitignored and lives only in the main checkout, so a worktree
    has none. Prefer a local one, then derive the MAIN checkout from git's
    common dir (a worktree's ``--git-common-dir`` points at ``<main>/.git``),
    then fall back so the caller's error message names a useful path.
    """
    local = repo_root / ".wpt"
    if (local / "css").is_dir():
        return local
    try:
        common = subprocess.run(
            ["git", "rev-parse", "--path-format=absolute", "--git-common-dir"],
            cwd=repo_root, capture_output=True, text=True, check=True,
        ).stdout.strip()
        if common:
            main = Path(common).parent / ".wpt"
            if (main / "css").is_dir():
                return main
    except Exception:  # noqa: BLE001 - git absent or odd layout: fall through
        pass
    return local


def leg_argv(
    leg: str,
    *,
    python: str | None = None,
    wpt: Path,
    report: Path,
    db: Path,
    artifacts: Path,
    filter_substr: str,
    limit: int | None,
    workers: int,
    engine_cmd: str,
    prince_cmd: str,
    weasy_cmd: str | None = None,
) -> list[str]:
    """Build the harness argv for one leg."""
    cmd = [
        python or sys.executable, "-m", "harness",
        "--wpt", str(wpt),
        "run",
        "--report", str(report),
        "--db", str(db),
        "--artifacts", str(artifacts),
        "--filter", filter_substr,
        "--workers", str(workers),
    ]
    if limit is not None:
        cmd += ["--limit", str(limit)]

    if leg == "engine":
        cmd += ["--engine", "cli", "--cli-cmd", engine_cmd]
    elif leg == "prince":
        cmd += ["--engine", "cli", "--cli-cmd", prince_cmd]
    elif leg == "weasy":
        if not weasy_cmd:
            raise ValueError("the weasy leg needs weasy_cmd")
        cmd += ["--engine", "cli", "--cli-cmd", weasy_cmd]
    elif leg == "chromium":
        cmd += ["--engine", "chromium"]
    else:
        raise ValueError(f"unknown leg {leg!r}")
    return cmd


def run_leg(leg: str, *, cwd: Path, quiet: bool = False, **kw) -> dict[str, str]:
    """Run one leg and return {test_id: status}. Prints a one-line summary."""
    report: Path = kw["report"]
    if not quiet:
        print(f"  [{leg}] running ...", flush=True)
    proc = subprocess.run(
        leg_argv(leg, **kw), cwd=cwd, capture_output=True, text=True
    )
    if not report.exists():
        print(
            f"  [{leg}] NO REPORT (rc={proc.returncode})",
            file=sys.stderr,
        )
        tail = (proc.stdout + proc.stderr).strip().splitlines()[-6:]
        for line in tail:
            print(f"        {line}", file=sys.stderr)
        return {}
    try:
        data = json.loads(report.read_text())
    except Exception as exc:  # noqa: BLE001 - report drift is reported, not raised
        print(f"  [{leg}] unreadable report: {exc}", file=sys.stderr)
        return {}

    out: dict[str, str] = {}
    for r in data.get("results", []):
        out[r["test"]] = r["status"]
    if not quiet:
        passed = sum(1 for v in out.values() if v == "PASS")
        print(f"  [{leg}] {passed}/{len(out)} PASS")
    return out


def format_table(
    results: dict[str, dict[str, str]], legs: list[str]
) -> tuple[str, list[tuple[str, str]]]:
    """Render the comparison table; return it plus the flagged rows."""
    test_w, col_w = 46, 10
    header = f"{'test':<{test_w}}" + "".join(f"{leg:>{col_w}}" for leg in legs)
    lines = [header, "-" * len(header)]
    flagged: list[tuple[str, str]] = []

    ids = sorted({t for r in results.values() for t in r})
    for t in ids:
        short = t.split("/css-page/")[-1] if "/css-page/" in t else t
        short = short[: test_w - 1]
        row = f"{short:<{test_w}}"
        leg_status: dict[str, str] = {}
        for leg in legs:
            st = results[leg].get(t, "?")
            leg_status[leg] = st
            row += f"{('PASS' if st == 'PASS' else 'FAIL'):>{col_w}}"
        lines.append(row)
        v = verdict(leg_status)
        if v:
            flagged.append((short, v))
    return "\n".join(lines), flagged


def main(argv: list[str] | None = None) -> int:
    """CLI entry point (registered as the `harness triage` subcommand)."""
    import argparse

    from . import engine as engine_mod  # noqa: F401  (keeps parity with sibling cmds)

    p = argparse.ArgumentParser(
        prog="harness triage",
        description="Run a filter through our engine, Chromium, Prince, and WeasyPrint; report the odd one out.",
    )
    p.add_argument("filter", help="harness --filter substring (e.g. page-name-002)")
    p.add_argument("--legs", default="engine,chromium,prince,weasy",
                   help="comma-separated subset of engine,chromium,prince,weasy")
    p.add_argument("--limit", type=int, default=None, help="max tests per leg")
    p.add_argument("--workers", type=int, default=2, help="harness workers per leg")
    p.add_argument("--engine-cmd", default=None,
                   help="command for the engine leg (default: <repo>/engine/target/debug/typeanvil render)")
    p.add_argument("--prince-cmd", default=None,
                   help="command for the prince leg (default: <repo>/scripts/render-prince.sh)")
    p.add_argument("--weasy-cmd", default=None,
                   help="command for the weasy leg (default: <repo>/scripts/render-weasyprint.sh)")
    p.add_argument("--python", default=None,
                   help="interpreter for the harness legs (default: this interpreter)")
    p.add_argument("--wpt", type=Path, default=None,
                   help="WPT checkout (default: <repo>/.wpt)")
    p.add_argument("--outdir", type=Path, default=None,
                   help="output dir (default: /tmp/triage-three-way/<filter>)")
    args = p.parse_args(argv)

    repo_root = Path(__file__).resolve().parent.parent
    legs = [x.strip() for x in args.legs.split(",") if x.strip()]
    bad = [x for x in legs if x not in LEGS]
    if bad:
        p.error(f"unknown leg(s): {', '.join(bad)} (choose from {', '.join(LEGS)})")

    engine_cmd = args.engine_cmd or f"{repo_root / 'engine' / 'target' / 'debug' / 'typeanvil'} render"
    prince_cmd = args.prince_cmd or str(repo_root / "scripts" / "render-prince.sh")
    weasy_cmd = args.weasy_cmd or str(repo_root / "scripts" / "render-weasyprint.sh")
    wpt_root = args.wpt if args.wpt is not None else resolve_wpt(repo_root)

    outdir = args.outdir or Path("/tmp/triage-three-way") / args.filter.replace("/", "_")
    outdir.mkdir(parents=True, exist_ok=True)

    if not (wpt_root / "css").is_dir():
        print(f"error: no WPT checkout at {wpt_root}. Run `python -m harness fetch` first.",
              file=sys.stderr)
        return 2
    for leg, cmd in (("prince", prince_cmd), ("weasy", weasy_cmd)):
        if leg not in legs:
            continue
        exe = cmd.split()[0]
        from shutil import which
        if not Path(exe).exists() and which(exe) is None:
            print(f"error: {leg} engine not found ({exe}). Drop it from --legs or install it.",
                  file=sys.stderr)
            return 2

    print(f"filter={args.filter!r}  legs={','.join(legs)}  out={outdir}")
    results: dict[str, dict[str, str]] = {}
    for leg in legs:
        results[leg] = run_leg(
            leg,
            cwd=repo_root,
            python=args.python,
            wpt=wpt_root,
            report=outdir / f"{leg}.json",
            db=outdir / f"{leg}.sqlite",
            artifacts=outdir / f"art-{leg}",
            filter_substr=args.filter,
            limit=args.limit,
            workers=args.workers,
            engine_cmd=engine_cmd,
            prince_cmd=prince_cmd,
            weasy_cmd=weasy_cmd,
        )

    if not any(results.values()):
        print("no results", file=sys.stderr)
        return 1

    table, flagged = format_table(results, legs)
    print()
    print(table)
    if flagged:
        print()
        print("TRIAGE")
        for short, v in flagged:
            print(f"  {short}\n     {v}")
    print()
    print(f"reports: {outdir}")
    return 0
