"""Scoreboard: wptreport.json, SQLite history, and a terminal summary.

Emits three artifacts per run (research brief section 5, item 7):

* ``wptreport.json`` -- a wpt-compatible report shape (``results[]`` each with
  ``test``, ``status``, ``duration``, and per-page ``diff`` subtests) so the output can
  be diffed with wpt tooling / wpt.fyi conventions.
* ``history.sqlite`` -- one ``runs`` row per invocation plus one ``results`` row per
  test, enabling pass-rate-over-time and newly-fixed / newly-broken analysis.
* a terminal summary table -- totals, pass-rate delta vs the previous run, and the
  worst regressions.

The regression gate (:func:`gate`) fails if any test that passed in the previously
recorded run now fails -- the "no regressions" CI gate from the brief.
"""

from __future__ import annotations

import json
import sqlite3
import time
from dataclasses import dataclass
from pathlib import Path

from .runner import TestResult

DEFAULT_DB = Path("history.sqlite")

_SCHEMA = """
CREATE TABLE IF NOT EXISTS runs (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at   REAL NOT NULL,
    engine       TEXT NOT NULL,
    total        INTEGER NOT NULL,
    passed       INTEGER NOT NULL,
    failed       INTEGER NOT NULL,
    errored      INTEGER NOT NULL,
    skipped      INTEGER NOT NULL,
    filter       TEXT
);
CREATE TABLE IF NOT EXISTS results (
    run_id        INTEGER NOT NULL REFERENCES runs(id),
    test          TEXT NOT NULL,
    status        TEXT NOT NULL,
    duration      REAL NOT NULL,
    max_difference INTEGER,
    total_pixels   INTEGER,
    message       TEXT,
    PRIMARY KEY (run_id, test)
);
CREATE INDEX IF NOT EXISTS idx_results_test ON results(test);
"""


# ---------------------------------------------------------------------------
# wptreport.json
# ---------------------------------------------------------------------------


def build_wptreport(results: list[TestResult], *, engine: str) -> dict:
    """Build a wpt-compatible report dict."""
    out_results = []
    for r in results:
        subtests = [
            {
                "name": f"page {p['page']}",
                "status": "PASS" if p["passed"] else "FAIL",
                "message": (
                    f"max_difference={p['max_difference']} total_pixels={p['total_pixels']}"
                ),
            }
            for p in r.diff_stats.get("pages", [])
        ]
        out_results.append(
            {
                "test": "/" + r.id,
                "status": r.status,
                "duration": round(r.time * 1000.0, 3),  # ms, wptreport convention
                "message": r.message or None,
                "subtests": subtests,
            }
        )
    return {
        "time_start": time.time(),
        "run_info": {"product": engine},
        "results": out_results,
    }


def write_wptreport(path: Path, results: list[TestResult], *, engine: str) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(build_wptreport(results, engine=engine), indent=2))
    return path


# ---------------------------------------------------------------------------
# SQLite history
# ---------------------------------------------------------------------------


def _connect(db_path: Path) -> sqlite3.Connection:
    conn = sqlite3.connect(db_path)
    conn.executescript(_SCHEMA)
    return conn


@dataclass
class Counts:
    total: int = 0
    passed: int = 0
    failed: int = 0
    errored: int = 0
    skipped: int = 0

    @property
    def pass_rate(self) -> float:
        return (self.passed / self.total) if self.total else 0.0


def tally(results: list[TestResult]) -> Counts:
    c = Counts(total=len(results))
    for r in results:
        if r.status == "PASS":
            c.passed += 1
        elif r.status == "FAIL":
            c.failed += 1
        elif r.status == "ERROR":
            c.errored += 1
        elif r.status == "SKIP":
            c.skipped += 1
    return c


def record_run(
    results: list[TestResult],
    *,
    engine: str,
    db_path: Path = DEFAULT_DB,
    filter_substr: str | None = None,
) -> int:
    """Insert a run row + per-test rows. Returns the new run id."""
    counts = tally(results)
    conn = _connect(db_path)
    try:
        cur = conn.execute(
            "INSERT INTO runs (started_at, engine, total, passed, failed, errored, "
            "skipped, filter) VALUES (?,?,?,?,?,?,?,?)",
            (
                time.time(),
                engine,
                counts.total,
                counts.passed,
                counts.failed,
                counts.errored,
                counts.skipped,
                filter_substr,
            ),
        )
        run_id = int(cur.lastrowid)
        conn.executemany(
            "INSERT INTO results (run_id, test, status, duration, max_difference, "
            "total_pixels, message) VALUES (?,?,?,?,?,?,?)",
            [
                (
                    run_id,
                    r.id,
                    r.status,
                    r.time,
                    r.diff_stats.get("max_difference"),
                    r.diff_stats.get("total_pixels"),
                    (r.message or "")[:2000],
                )
                for r in results
            ],
        )
        conn.commit()
        return run_id
    finally:
        conn.close()


def _run_row(conn: sqlite3.Connection, run_id: int) -> sqlite3.Row | None:
    conn.row_factory = sqlite3.Row
    return conn.execute("SELECT * FROM runs WHERE id=?", (run_id,)).fetchone()


def previous_run_id(conn: sqlite3.Connection, before_id: int) -> int | None:
    row = conn.execute(
        "SELECT id FROM runs WHERE id < ? ORDER BY id DESC LIMIT 1", (before_id,)
    ).fetchone()
    return int(row[0]) if row else None


def latest_run_id(db_path: Path = DEFAULT_DB) -> int | None:
    if not db_path.exists():
        return None
    conn = _connect(db_path)
    try:
        row = conn.execute("SELECT id FROM runs ORDER BY id DESC LIMIT 1").fetchone()
        return int(row[0]) if row else None
    finally:
        conn.close()


def _statuses(conn: sqlite3.Connection, run_id: int) -> dict[str, str]:
    rows = conn.execute(
        "SELECT test, status FROM results WHERE run_id=?", (run_id,)
    ).fetchall()
    return {t: s for t, s in rows}


def regressions(
    conn: sqlite3.Connection, current_id: int, previous_id: int
) -> list[str]:
    """Tests that PASSed in ``previous_id`` but did not PASS in ``current_id``."""
    cur = _statuses(conn, current_id)
    prev = _statuses(conn, previous_id)
    out = [
        t for t, s in prev.items() if s == "PASS" and cur.get(t, "MISSING") != "PASS"
    ]
    return sorted(out)


def fixes(conn: sqlite3.Connection, current_id: int, previous_id: int) -> list[str]:
    """Tests that did not PASS in ``previous_id`` but PASS in ``current_id``."""
    cur = _statuses(conn, current_id)
    prev = _statuses(conn, previous_id)
    out = [t for t, s in cur.items() if s == "PASS" and prev.get(t, "MISSING") != "PASS"]
    return sorted(out)


# ---------------------------------------------------------------------------
# Terminal summary
# ---------------------------------------------------------------------------


def format_summary(db_path: Path = DEFAULT_DB, run_id: int | None = None) -> str:
    """Render a terminal scoreboard for ``run_id`` (defaults to the latest run)."""
    if not db_path.exists():
        return "no history: run the harness first (python -m harness run ...)"

    conn = _connect(db_path)
    try:
        conn.row_factory = sqlite3.Row
        if run_id is None:
            run_id = latest_run_id(db_path)
        if run_id is None:
            return "no runs recorded"
        run = _run_row(conn, run_id)
        if run is None:
            return f"run {run_id} not found"

        lines: list[str] = []
        rate = (run["passed"] / run["total"] * 100.0) if run["total"] else 0.0
        lines.append(
            f"Run #{run['id']}  engine={run['engine']}"
            + (f"  filter={run['filter']}" if run["filter"] else "")
        )
        lines.append("-" * 56)
        lines.append(f"  total   {run['total']:>6}")
        lines.append(f"  pass    {run['passed']:>6}   ({rate:5.1f}%)")
        lines.append(f"  fail    {run['failed']:>6}")
        lines.append(f"  error   {run['errored']:>6}")
        lines.append(f"  skip    {run['skipped']:>6}")

        prev_id = previous_run_id(conn, run_id)
        if prev_id is not None:
            prev = _run_row(conn, prev_id)
            prev_rate = (
                (prev["passed"] / prev["total"] * 100.0) if prev["total"] else 0.0
            )
            delta = rate - prev_rate
            sign = "+" if delta >= 0 else ""
            lines.append("-" * 56)
            lines.append(
                f"  pass-rate delta vs run #{prev_id}: {sign}{delta:.1f}%"
                f"  ({prev_rate:.1f}% -> {rate:.1f}%)"
            )
            regs = regressions(conn, run_id, prev_id)
            fxs = fixes(conn, run_id, prev_id)
            lines.append(f"  newly broken: {len(regs)}   newly fixed: {len(fxs)}")
            if regs:
                lines.append("  worst regressions:")
                for t in regs[:10]:
                    lines.append(f"    - {t}")
        return "\n".join(lines)
    finally:
        conn.close()


def gate(db_path: Path = DEFAULT_DB) -> tuple[bool, list[str]]:
    """Regression gate: (ok, regressions) comparing the latest two recorded runs.

    ``ok`` is False if any test that passed in the previous run now fails.
    """
    if not db_path.exists():
        return True, []
    conn = _connect(db_path)
    try:
        cur_id = latest_run_id(db_path)
        if cur_id is None:
            return True, []
        prev_id = previous_run_id(conn, cur_id)
        if prev_id is None:
            return True, []
        regs = regressions(conn, cur_id, prev_id)
        return (len(regs) == 0), regs
    finally:
        conn.close()
