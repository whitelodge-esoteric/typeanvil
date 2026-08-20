"""``python -m harness`` entry point: ``fetch`` / ``run`` / ``score`` / ``history``.

Subcommands:

* ``fetch`` -- sparse-clone the WPT paged-media directories into ``.wpt/``.
* ``run`` -- enumerate + render + compare + record a conformance run, writing
  ``wptreport.json`` and appending to ``history.sqlite``, then print the scoreboard.
* ``score`` -- print the scoreboard from ``history.sqlite`` (optionally ``--gate``).
* ``history`` -- list recorded runs with pass rates.

Design goal (task conventions): keep imports lazy so ``fetch``/``score``/``history``
work without Playwright installed; only ``run --engine chromium`` needs it.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

from . import report
from .wpt_fetch import default_wpt_dir


def _cmd_fetch(args: argparse.Namespace) -> int:
    from .wpt_fetch import fetch

    wpt_dir = fetch(Path(args.wpt) if args.wpt else None, force=args.force)
    print(f"WPT checkout ready at {wpt_dir}")
    return 0


def _cmd_run(args: argparse.Namespace) -> int:
    from .engine import PageSpec
    from .runner import EngineConfig, RunConfig, run

    wpt_root = Path(args.wpt) if args.wpt else default_wpt_dir()
    if not (wpt_root / "css").is_dir():
        print(
            f"error: no WPT checkout at {wpt_root}. Run `python -m harness fetch` first.",
            file=sys.stderr,
        )
        return 2

    engine_cfg = EngineConfig(
        kind=args.engine,
        wpt_root=wpt_root,
        cli_cmd=args.cli_cmd,
        timeout_ms=int(args.timeout * 1000),
    )
    cfg = RunConfig(
        wpt_root=wpt_root,
        engine=engine_cfg,
        spec=PageSpec.wpt_default(),
        filter_substr=args.filter,
        limit=args.limit,
        workers=args.workers,
        artifacts_root=Path(args.artifacts),
    )

    n = len(_selected_count(cfg))
    print(f"running {n} test(s)  engine={args.engine}  workers={args.workers}")

    done = {"n": 0}

    def on_result(res) -> None:
        done["n"] += 1
        mark = {"PASS": "PASS", "FAIL": "FAIL", "ERROR": "ERR ", "SKIP": "SKIP"}[
            res.status
        ]
        print(f"  [{done['n']:>4}/{n}] {mark}  {res.id}")

    results = run(cfg, on_result=on_result)

    db_path = Path(args.db)
    report.write_wptreport(Path(args.report), results, engine=args.engine)
    run_id = report.record_run(
        results, engine=args.engine, db_path=db_path, filter_substr=args.filter
    )
    print()
    print(report.format_summary(db_path, run_id))
    print(f"\nwptreport: {args.report}   history: {db_path}")
    return 0


def _selected_count(cfg) -> list:
    from .runner import select_tests

    return select_tests(cfg)


def _cmd_score(args: argparse.Namespace) -> int:
    db_path = Path(args.db)
    if args.gate:
        ok, regs = report.gate(db_path)
        print(report.format_summary(db_path))
        if not ok:
            print(f"\nGATE FAILED: {len(regs)} regression(s)", file=sys.stderr)
            for t in regs[:20]:
                print(f"  - {t}", file=sys.stderr)
            return 1
        print("\nGATE OK: no regressions")
        return 0
    print(report.format_summary(db_path))
    return 0


def _cmd_history(args: argparse.Namespace) -> int:
    import sqlite3

    db_path = Path(args.db)
    if not db_path.exists():
        print("no history recorded")
        return 0
    conn = sqlite3.connect(db_path)
    conn.row_factory = sqlite3.Row
    try:
        rows = conn.execute(
            "SELECT * FROM runs ORDER BY id DESC LIMIT ?", (args.limit,)
        ).fetchall()
    finally:
        conn.close()
    if not rows:
        print("no runs recorded")
        return 0
    print(f"{'run':>4}  {'engine':<10} {'pass':>6} {'/':^1} {'total':<6} {'rate':>6}  filter")
    for r in rows:
        rate = (r["passed"] / r["total"] * 100.0) if r["total"] else 0.0
        print(
            f"{r['id']:>4}  {r['engine']:<10} {r['passed']:>6} / {r['total']:<6} "
            f"{rate:5.1f}%  {r['filter'] or ''}"
        )
    return 0


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(prog="harness", description=__doc__)
    p.add_argument("--wpt", default=None, help="path to WPT checkout (default .wpt/)")
    sub = p.add_subparsers(dest="command", required=True)

    pf = sub.add_parser("fetch", help="sparse-clone WPT into .wpt/")
    pf.add_argument("--force", action="store_true", help="re-checkout even if present")
    pf.set_defaults(func=_cmd_fetch)

    pr = sub.add_parser("run", help="run a conformance pass")
    pr.add_argument("--engine", choices=["chromium", "cli"], default="chromium")
    pr.add_argument("--cli-cmd", default=None, help="command for --engine cli")
    pr.add_argument("--filter", default=None, help="substring filter on test id")
    pr.add_argument("--limit", type=int, default=None, help="max tests to run")
    pr.add_argument("--workers", type=int, default=2, help="parallel worker processes")
    pr.add_argument("--timeout", type=float, default=30.0, help="per-render timeout (s)")
    pr.add_argument("--report", default="wptreport.json", help="wptreport output path")
    pr.add_argument("--db", default=str(report.DEFAULT_DB), help="history SQLite path")
    pr.add_argument("--artifacts", default="artifacts", help="diff artifact dir")
    pr.set_defaults(func=_cmd_run)

    ps = sub.add_parser("score", help="print scoreboard from history")
    ps.add_argument("--db", default=str(report.DEFAULT_DB))
    ps.add_argument("--gate", action="store_true", help="exit nonzero on regressions")
    ps.set_defaults(func=_cmd_score)

    ph = sub.add_parser("history", help="list recorded runs")
    ph.add_argument("--db", default=str(report.DEFAULT_DB))
    ph.add_argument("--limit", type=int, default=20)
    ph.set_defaults(func=_cmd_history)

    return p


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
