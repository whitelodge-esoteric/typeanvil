# Task: WPT conformance harness for Typeanvil (CORE-49)

Build `harness/`, a Python package that runs W3C Web Platform Tests print-reftests
against an HTML→PDF engine and produces a conformance scoreboard. There is NO engine
yet — the harness is built first and validated by running **Chromium itself as the
engine under test** (should score near-100%, proving the oracle works).

Read `research/typeanvil-wpt-harness-brief.md` in this repo first — it specifies the
mechanics (print-reftest page geometry, fuzzy matching, manifest filtering) and cites
the wptrunner source this must mirror.

## Environment constraints (this machine)

- macOS, NO system Chrome, NO poppler/pdftoppm. Use ONLY Python-managed deps:
  - `playwright` (Python) with its bundled Chromium for both reference rendering and
    engine-as-Chromium mode. After `uv sync`, run `uv run playwright install chromium`.
  - `pypdfium2` for PDF→PNG rasterization at 96 DPI (NOT pdftoppm).
  - `Pillow` for pixel comparison.
- Python 3.11+, managed with `uv` (`uv` is on PATH). Create `pyproject.toml` with a
  `[project]` table, deps above, plus `pytest` in dev deps.
- Do NOT vendor the WPT repo. Write `harness/wpt_fetch.py` that sparse-clones
  `https://github.com/web-platform-tests/wpt` (depth 1, sparse paths: `css/css-page`,
  `css/css-break`, `css/css-multicol`, `fonts/ahem/`, plus repo-root support files those
  tests reference) into `.wpt/` (gitignored). Idempotent: skip if present.

## Architecture

```
harness/
  __init__.py
  wpt_fetch.py       # sparse WPT checkout into .wpt/
  manifest.py        # enumerate tests by scanning the sparse checkout
  engine.py          # EngineAdapter protocol + ChromiumEngine + CliEngine
  rasterize.py       # PDF bytes -> list[PIL.Image] via pypdfium2 @ 96 DPI
  compare.py         # pixel compare with WPT fuzzy semantics
  runner.py          # orchestrates: enumerate -> render test+ref -> compare -> record
  report.py          # scoreboard: wptreport.json + SQLite history + terminal summary
  cli.py             # `python -m harness` entry: run / score / history subcommands
tests/               # pytest unit tests (see Verification)
```

### manifest.py
Scan `.wpt/css/css-page`, `css/css-break`, `css/css-multicol` for print-reftests:
files matching `*-print.html` or living under a `print/` directory, plus any test whose
`<link rel="match">`/`<link rel="mismatch">` reference exists. Parse each test's HTML head for:
`rel=match`/`rel=mismatch` references (resolve relative paths), `<meta name=fuzzy>`
(parse `maxDifference;totalPixels` ranges, both `a-b` and bare forms, optional per-ref
prefix), `<meta name=reftest-pages>` (page selection list/ranges). Exclude tests
containing `<script`. Emit dataclass `TestCase{path, refs, fuzzy, pages, mismatch}`.
Support chained references (a ref can itself have a ref — follow one level).

### engine.py
```python
class EngineAdapter(Protocol):
    def render_pdf(self, html_path: Path, page: PageSpec) -> bytes: ...
```
- `PageSpec`: width/height/margins in inches. WPT print-reftest geometry is FIXED:
  5in × 3in page, 0.5in margins on all sides.
- `ChromiumEngine`: sync playwright, `page.goto(file_url)`, wait for fonts/load, then
  `page.pdf(width="5in", height="3in", margin=..., print_background=True,
  prefer_css_page_size=False)`. One browser instance reused across tests (launch cost
  dominates otherwise). Must also serve `.wpt/` over localhost HTTP (stdlib
  http.server in a thread) rather than file:// URLs — WPT tests use absolute paths
  like `/fonts/ahem.css`. Root the server at `.wpt/`.
- `CliEngine`: runs `[cmd, input_html, "--page-width", "5in", ...]` per a configurable
  arg template; reads PDF from an output path. This defines the future
  `typeanvil render` contract — document the exact contract in engine.py's docstring.

### compare.py
WPT reftest semantics: images must be pixel-identical unless fuzzy allows a per-channel
max difference and total differing-pixel count within the given ranges. Compare
page-by-page (respect `reftest-pages` selection); page count mismatch = FAIL unless
pages were selected. For `rel=mismatch`, PASS means images differ beyond tolerance.
Return a result object with per-page diff stats and, on failure, save a visual diff
PNG triptych (test/ref/diff) under `artifacts/<test-id>/`.

### runner.py + report.py
- Runner: iterate manifest, render test and reference through the SAME engine, compare,
  collect `TestResult{id, status: PASS|FAIL|ERROR|SKIP, time, diff_stats}`. Support
  `--filter <substr>` and `--limit N`. Crashes/timeouts in the engine = ERROR, never
  abort the run. Parallelize with a small worker pool (default 4) — but playwright sync
  API is not thread-safe, so use one browser context per worker or a process pool.
- Report: write `wptreport.json` (wpt-compatible shape: results[] with test, status,
  duration), append a run row + per-test rows to `history.sqlite`, print a summary
  table (total/pass/fail/error, pass-rate delta vs previous run, worst regressions).
- Regression gate: `python -m harness score --gate` exits non-zero if any test that
  passed in the previous recorded run now fails.

## Conventions

- Python 3.11+, full type hints, dataclasses, pathlib. No framework, stdlib + the three
  deps only. Keep modules importable without playwright installed (lazy import inside
  ChromiumEngine) so unit tests run fast.
- Every module gets a docstring explaining its role citing the research brief.
- `.gitignore`: `.wpt/`, `artifacts/`, `history.sqlite`, `__pycache__/`, `.venv/`.

## Verification (run all of it yourself before finishing)

1. `uv sync && uv run playwright install chromium`
2. `uv run pytest` — unit tests you write for: fuzzy-meta parsing (all syntax forms),
   manifest extraction on a fixture HTML file (commit small fixtures under
   `tests/fixtures/`), compare.py on synthetic PIL images (identical, within-fuzzy,
   beyond-fuzzy, mismatch mode), reftest-pages parsing.
3. `uv run python -m harness fetch` — sparse checkout completes.
4. `uv run python -m harness run --engine chromium --filter css-page --limit 25` —
   end-to-end smoke: Chromium as both engine and reference. Report the actual
   pass/fail counts you observed in your final summary (some legitimate failures are
   expected — Chromium doesn't pass all of css-page — that's signal, not a bug).
5. `uv run python -m harness score` — prints scoreboard from history.sqlite.

Do NOT claim success unless step 4 actually produced a scoreboard with >0 tests run.
