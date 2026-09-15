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
def _fmt_identity(identity) -> str:
    if identity is None:
        return "  (unknown)"
    return "\n".join(
        [
            f"  engine_kind: {identity.engine_kind}",
            f"  cli_cmd: {identity.cli_cmd}",
            f"  source_commit: {identity.source_commit}",
            f"  binary: path={identity.binary.get('path')} "
            f"sha256={identity.binary.get('sha256')} "
            f"version={identity.binary.get('version')}",
            f"  wpt_revision: {identity.wpt_revision}",
            f"  page_spec: {identity.page_spec}",
            f"  dpi: {identity.dpi}",
            f"  rasterizer: {identity.rasterizer}",
            f"  fonts: {identity.fonts}",
        ]
    )


def _cmd_baseline(args: argparse.Namespace) -> int:
    from .capture import build_capture, write_capture
    from .engine import PageSpec
    from .runner import EngineConfig, RunConfig, select_tests

    wpt_root = Path(args.wpt) if args.wpt else default_wpt_dir()
    if not (wpt_root / "css").is_dir():
        print(
            f"error: no WPT checkout at {wpt_root}. Run `python -m harness fetch` first.",
            file=sys.stderr,
        )
        return 2
    if args.engine == "cli" and not args.cli_cmd:
        print("error: --engine cli requires --cli-cmd", file=sys.stderr)
        return 2

    engine_cfg = EngineConfig(
        kind=args.engine, wpt_root=wpt_root, cli_cmd=args.cli_cmd, timeout_ms=30_000
    )
    spec = PageSpec.wpt_default()
    cfg = RunConfig(
        wpt_root=wpt_root,
        engine=engine_cfg,
        spec=spec,
        filter_substr=args.filter,
        limit=args.limit,
        workers=args.workers,
    )
    tests = select_tests(cfg)
    capture = build_capture(
        tests=tests,
        wpt_root=wpt_root,
        engine_cfg=engine_cfg,
        spec=spec,
        label=args.label,
        dpi=args.dpi,
        source_commit_override=args.source_commit,
    )
    path = write_capture(Path(args.out) / f"{args.label}.json", capture)

    statuses: dict[str, int] = {}
    for r in capture.results:
        statuses[r["status"]] = statuses.get(r["status"], 0) + 1

    print(f"capture: {capture.label}")
    print(f"complete: {capture.complete}")
    print(f"engine: {capture.identity.engine_kind}  source_commit: {capture.identity.source_commit}")
    print(f"wpt_revision: {capture.identity.wpt_revision}")
    print(f"selection: {len(capture.selection)} test(s)  documents: {len(capture.documents)}")
    print("results: " + (", ".join(f"{k}={v}" for k, v in sorted(statuses.items())) or "none"))
    print(f"written: {path}")
    print("BASELINE CAPTURED — not a gate result")
    return 0 if capture.complete else 1


def _cmd_gate(args: argparse.Namespace) -> int:
    from .engine import PageSpec
    from .release_gate import _resolve_capture, evaluate
    from .runner import EngineConfig

    wpt_root = Path(args.wpt) if args.wpt else default_wpt_dir()
    corpus_root = Path(args.corpus)
    fixture_root = Path(args.fixtures)
    out_dir = Path(args.out)

    baseline_ref = args.baseline
    candidate_ref = args.candidate
    captures_dir = Path(args.captures)
    baseline_ref = _resolve_ref_cli(baseline_ref, captures_dir)
    candidate_ref = _resolve_ref_cli(candidate_ref, captures_dir)

    engine_cfg = None
    direct_path = Path(args.direct) if args.direct else None
    if direct_path is not None and direct_path.exists():
        if args.engine == "cli" and not args.cli_cmd:
            print("error: --engine cli requires --cli-cmd", file=sys.stderr)
            return 2
        engine_cfg = EngineConfig(
            kind=args.engine, wpt_root=wpt_root, cli_cmd=args.cli_cmd, timeout_ms=30_000
        )

    verdict = evaluate(
        baseline=baseline_ref,
        candidate=candidate_ref,
        manifest_path=args.direct,
        reviews_path=args.reviews,
        policy_path=args.policy,
        out_dir=out_dir,
        wpt_root=wpt_root,
        corpus_root=corpus_root,
        fixture_root=fixture_root,
        engine_cfg=engine_cfg,
        dpi=args.dpi,
        spec=PageSpec.wpt_default(),
    )

    base_cap, _ = _resolve_capture(baseline_ref, out_dir)
    cand_cap, _ = _resolve_capture(candidate_ref, out_dir)

    print("baseline identity:")
    print(_fmt_identity(base_cap.identity if base_cap else None))
    print("candidate identity:")
    print(_fmt_identity(cand_cap.identity if cand_cap else None))

    env = next(
        (c for c in verdict.conditions if c.name == "incompatible_environment"), None
    )
    if env is not None:
        print(f"environment: INCOMPATIBLE ({env.detail})")
    else:
        print("environment: COMPATIBLE")

    print(f"conditions: {len(verdict.conditions)}")
    for c in verdict.conditions:
        print(f"  - {c.name}: {c.detail}")

    print(f"changes: {len(verdict.changes)}")
    for ch in verdict.changes:
        print(
            f"  - {ch.doc_id} [{ch.property}]: baseline={ch.baseline} "
            f"candidate={ch.candidate}"
        )

    print(f"direct: {len(verdict.direct)}")
    for r in verdict.direct:
        mark = "PASS" if r.passed else "FAIL"
        print(f"  [{mark}] {r.check_id}: {r.detail}")

    if verdict.ok:
        print("GATE PASSED")
        return 0
    if verdict.report_path is not None:
        print(f"report: {verdict.report_path}")
    print(f"GATE FAILED: {len(verdict.conditions)} condition(s)")
    if any(
        c.name in ("missing_baseline", "missing_candidate", "unsupported_schema")
        for c in verdict.conditions
    ):
        return 2
    return 1


def _resolve_ref_cli(ref, captures_dir: Path):
    """Resolve a bare label against the captures directory for the gate command."""
    if isinstance(ref, str):
        p = Path(ref)
        if not p.exists() and not p.is_absolute():
            return str(captures_dir / f"{ref}.json")
    return ref



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

    pb = sub.add_parser("baseline", help="render the selected set and record a baseline capture")
    pb.add_argument("--engine", choices=["chromium", "cli"], default="cli")
    pb.add_argument("--cli-cmd", default=None, help="command for --engine cli")
    pb.add_argument("--label", required=True, help="capture label (file name, no extension)")
    pb.add_argument("--out", default="gate/captures", help="output directory for the capture")
    pb.add_argument("--filter", default=None, help="substring filter on test id")
    pb.add_argument("--limit", type=int, default=None, help="max tests to run")
    pb.add_argument("--workers", type=int, default=1, help="accepted for interface parity; capture is sequential")
    pb.add_argument("--dpi", type=int, default=96, help="rasterization DPI for fingerprints")
    pb.add_argument(
        "--source-commit",
        default=None,
        help="commit recorded in the capture identity; required inside the dev "
        "container, where the worktree's .git pointer resolves to a host path "
        '(pass "$(git rev-parse HEAD)")',
    )
    pb.set_defaults(func=_cmd_baseline)

    pg = sub.add_parser("gate", help="compare two captures plus direct checks")
    pg.add_argument("--baseline", required=True, help="baseline capture path or label")
    pg.add_argument("--candidate", required=True, help="candidate capture path or label")
    pg.add_argument("--captures", default="gate/captures", help="directory of capture files (for label resolution)")
    pg.add_argument("--direct", default="harness/direct_manifest.json", help="direct-check manifest (omit for none)")
    pg.add_argument("--reviews", default="gate/reviews", help="review records directory or file")
    pg.add_argument("--policy", default="gate/policy.json", help="policy JSON (optional)")
    pg.add_argument("--out", default="gate/out", help="output directory for the gate report")
    pg.add_argument("--corpus", default=".", help="corpus root (for input_source corpus)")
    pg.add_argument("--fixtures", default="harness/direct_fixtures", help="fixture root")
    pg.add_argument("--dpi", type=int, default=96, help="rasterization DPI for direct checks")
    pg.add_argument("--engine", choices=["chromium", "cli"], default="cli")
    pg.add_argument("--cli-cmd", default=None, help="command for --engine cli (direct checks)")
    pg.set_defaults(func=_cmd_gate)

    # `triage` owns its own arguments (see harness/triage.py); it is delegated
    # wholesale so the two command surfaces stay in step.
    sub.add_parser("triage", help="run a filter through engine + Chromium + Prince and report the odd one out",
                   add_help=False)

    return p


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    raw = list(sys.argv[1:] if argv is None else argv)
    # Delegate `triage` before argparse sees its flags, so it can define its own.
    if raw and raw[0] == "triage":
        from . import triage

        return triage.main(raw[1:])
    args = parser.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
