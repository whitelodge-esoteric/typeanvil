# Task: implement the TypeAnvil harness release gate (CORE-206)

You are implementing a **release gate** for an existing Python conformance
harness. Work only inside this git worktree:

```
/Users/elijah/workspace/typeanvil.worktrees/core-206
```

Python 3.11. Interpreter for every command:
`/Users/elijah/workspace/typeanvil/.venv/bin/python` (has pytest, Pillow,
pypdfium2).

## Read these first, in this order

1. `AGENTS.md` — repo rules.
2. `docs/specifications/harness-release-gate.spec.md` — **THE CONTRACT.** Every
   "shall" statement and every acceptance criterion is binding. Implement it.
3. `docs/specifications/wpt-conformance-harness.spec.md` — the existing harness
   contract. You must not change its meaning.
4. `docs/conventions/doc-conventions.md` and `docs/conventions/issue-evidence.md`
   — house rules and the evidence rules that motivate this work.
5. `harness/*.py` — the code you extend.
6. `tests/test_report.py`, `tests/test_manifest.py` — the existing test style to
   match.

## The problem you are solving

The harness renders a test and its reference through the same engine and compares
them. When a change moves **both** documents the same way, the pair still matches
and the score stays flat while the output changes. That happened: an interior
`vertical-rl` fill moved a Chromium-verified document from 1 page to 2, and the
283-test comparison reported zero flips. The gate adds per-document output
comparison and direct PDF assertions, so this class of change cannot pass
unnoticed.

## Deliverable

New Python modules, a reviewed direct-check manifest, wiring in the CLI, and unit
tests. **No new dependencies** (stdlib + Pillow + pypdfium2 only).

### Files to create

1. `harness/capture.py` — the capture layer.
2. `harness/direct.py` — direct assertions on emitted PDFs.
3. `harness/release_gate.py` — identity validation, per-document comparison,
   dispositions, verdict, exit status.
4. `harness/direct_manifest.json` — the initial reviewed corpus (contents given
   below, verbatim; do not invent expectations).
5. `tests/test_capture.py`, `tests/test_direct.py`, `tests/test_gate.py`.

### File to modify

`harness/cli.py` — add a `baseline` subcommand and a `gate` subcommand. Keep
`fetch`, `run`, `score`, `history`, and the `triage` delegation **exactly** as
they are, including their flags, output, and exit codes. Keep imports lazy so
`fetch`/`score`/`history` still work without Playwright or pypdfium2 installed.

### Existing code you MUST use, and MUST NOT redefine or change

- `harness/engine.py`: `PageSpec` (fields `width_in`, `height_in`,
  `margin_*_in`; `wpt_default()`), `EngineAdapter`, `CliEngine`, `ChromiumEngine`.
- `harness/runner.py`: `EngineConfig` (`kind`, `wpt_root`, `cli_cmd`,
  `timeout_ms`; `.build()`), `RunConfig`, `select_tests`, `run_one`, `TestResult`.
- `harness/manifest.py`: `enumerate_tests`, `parse_test`, `TestCase`
  (`.id`, `.path`, `.refs`, `.fuzzy`, `.pages`, `.mismatch`), `Ref`
  (`.path`, `.relation`, `.chained`).
- `harness/rasterize.py`: `rasterize_pdf`, `DEFAULT_DPI` (96).
- `harness/report.py`: `record_run`, `write_wptreport`, `build_wptreport`,
  `DEFAULT_DB`. Leave `report.gate()` untouched even though the new gate
  supersedes it.
- `harness/wpt_fetch.py`: `default_wpt_dir`, `TEST_PATHS`.

## Interfaces to implement

### `harness/capture.py`

```python
CAPTURE_SCHEMA = "typeanvil.harness.capture/1"

class CaptureError(Exception): ...

@dataclass(frozen=True)
class PageCapture:
    index: int
    size_pt: tuple[float, float]      # PDF page size in points, 1/72 inch
    fingerprint: str                  # sha256 hex of the rasterized RGB page

@dataclass(frozen=True)
class DocumentCapture:
    doc_id: str                       # WPT-root-relative POSIX path
    role: tuple[str, ...]             # "test" and/or "reference"
    content_sha256: str               # sha256 hex of the document bytes
    page_count: int
    pages: tuple[PageCapture, ...]
    render_failed: bool = False
    message: str = ""

@dataclass(frozen=True)
class CaptureIdentity:
    engine_kind: str                  # "cli" | "chromium"
    cli_cmd: str | None
    source_commit: str                # "unknown" when undeterminable
    binary: dict                      # {"path","sha256","version"}
    wpt_revision: str                 # "unknown" when .wpt is not a git checkout
    page_spec: dict                   # the PageSpec fields as a dict
    dpi: int
    rasterizer: str                   # e.g. "pypdfium2 4.30.0"
    fonts: dict                       # {"identity": ..., "source": ...}

@dataclass(frozen=True)
class Capture:
    label: str
    complete: bool
    identity: CaptureIdentity
    selection: tuple[str, ...]        # test ids, sorted
    documents: tuple[DocumentCapture, ...]
    results: tuple[dict, ...]         # [{"id":..., "status":...}], WPT statuses

def capture_pdf(pdf: bytes, *, dpi: int) -> tuple[int, list[PageCapture]]
def documents_for(tests: list[TestCase], wpt_root: Path) -> list[tuple[str, set[str]]]
def build_capture(*, tests, wpt_root, engine_cfg, spec, label, dpi=DEFAULT_DPI) -> Capture
def write_capture(path: Path, capture: Capture) -> Path
def load_capture(path: Path) -> Capture          # raises CaptureError
def binary_identity(cli_cmd: str | None) -> dict
def source_commit(repo_root: Path) -> str
def wpt_revision(wpt_root: Path) -> str
def rasterizer_identity() -> str
```

Rules:

- `capture_pdf` opens the PDF once with pypdfium2, reads each page's size in
  points, renders each page at `dpi` (same scale rule as
  `harness/rasterize.py`: `scale = dpi / 72.0`), converts to RGB, and
  fingerprints the raw pixel bytes (`PIL.Image.tobytes()` plus width, height,
  and mode). Use one helper so capture and comparison agree.
- `documents_for` returns each unique document once, ordered by `doc_id`, with
  every role it carries: the test document, each immediate reference, and each
  chained reference. Deduplicate by resolved path.
- `build_capture` renders each unique document **once**, sequentially (order and
  determinism matter more than speed here), and records a WPT status per test by
  calling `runner.run_one` for the test's own pair. A document that raises is
  recorded with `render_failed=True` and the capture is marked `complete=False`.
  A capture is `complete=False` when any render failed.
- `binary_identity` sha256s the resolved binary path and records the version it
  prints for `--version` (run it with a short timeout; a failure yields
  `"version": "unknown"`). When `cli_cmd` is None, return
  `{"path": "unknown", "sha256": "unknown", "version": "unknown"}`.
- `source_commit` runs `git rev-parse HEAD` in the given directory and returns
  `"unknown"` on any failure. `wpt_revision` does the same for the WPT checkout.
- `fonts`: the engine exposes no font inventory, so record
  `{"identity": "unknown", "source": "unavailable"}`. Do not fake a value. The
  gate has an explicit acknowledgement path for this (below).
- `load_capture` raises `CaptureError` for a malformed file or any `schema`
  other than `CAPTURE_SCHEMA`.

### `harness/direct.py`

```python
DIRECT_SCHEMA = "typeanvil.harness.direct/1"

class DirectError(Exception): ...

@dataclass(frozen=True)
class DirectCheck:
    id: str
    input: str
    input_source: str          # "wpt" | "corpus" | "fixture"
    expectation: dict          # exactly one check key (see below)
    provenance: str

@dataclass(frozen=True)
class CheckResult:
    check_id: str
    input: str
    passed: bool
    detail: str

def load_manifest(path: Path) -> list[DirectCheck]   # raises DirectError
def check_pdf(pdf: bytes, check: DirectCheck, *, dpi: int = DEFAULT_DPI) -> CheckResult
def run_manifest(manifest_path, *, wpt_root, corpus_root, fixture_root, engine_cfg,
                 spec, dpi=DEFAULT_DPI) -> list[CheckResult]
```

Check keys, all measured on the emitted PDF:

| Key | Shape | Semantics |
|---|---|---|
| `page_count` | `int` or `[lo, hi]` | exact count, or an inclusive range |
| `page_size_pt` | `[w, h]`, optional `tolerance_pt` | per-page size in points; default tolerance 0.5pt |
| `text_present` | `[str, ...]` | every string appears in the extracted text of some page |
| `text_absent` | `[str, ...]` | no string appears in any page |
| `paint_region` | `{page, rect_pt, min_dark_fraction?, max_dark_fraction?}` | fraction of dark pixels inside the rect |
| `text_orientation` | `{page, rect_pt, expected}` | `"horizontal"` or `"vertical"`, from glyph extents in the rect |
| `link_target` | `{page, uri, rect_pt?, tolerance_pt?}` | a link annotation on that page with that URI, and (when `rect_pt` is given) bounds within the tolerance |

Coordinate convention, used by `rect_pt` and reported in every `detail`:
**points, top-left origin, x right, y down**, matching the rasterized image.

- `dark_fraction`: rasterize the page at `dpi`, crop the rect, and count pixels
  whose relative luminance is below 0.5. Report the measured fraction in
  `detail` for both pass and fail.
- `text_orientation`: read the glyph boxes in the rect; compare the summed
  advance along x against the summed extent along y and judge the dominant axis.
  Document the rule in the docstring.
- An expectation that carries **no recognised check key** is an error
  (`DirectError`), never a silent pass.
- An entry whose `provenance` is missing or empty is an error. `load_manifest`
  raises `DirectError` for a missing file, an unsupported `schema`, an empty
  `checks` list, a duplicate `id`, or empty provenance.
- Resolution: `"wpt"` → `wpt_root / input`; `"fixture"` →
  `fixture_root / input`; `"corpus"` → `corpus_root / input`.
- Any failure message must name the check id, the input, the expected value, and
  the actual value.

### `harness/release_gate.py`

```python
REVIEW_SCHEMA = "typeanvil.harness.reviews/1"

@dataclass(frozen=True)
class Change:
    doc_id: str
    property: str          # "page_count" | "page_size" | "rendered_image"
    baseline: str
    candidate: str
    change_id: str

@dataclass(frozen=True)
class Disposition:
    doc_id: str
    change_id: str
    kind: str              # "correction" | "regression" | "variation"
    reason: str
    provenance: str

@dataclass(frozen=True)
class Condition:
    name: str              # stable slug, e.g. "missing_baseline"
    detail: str

@dataclass
class GateVerdict:
    ok: bool
    conditions: list[Condition]
    changes: list[Change]
    direct: list[CheckResult]
    report_path: Path | None

def change_id(doc_id: str, baseline, candidate) -> str
def compare(baseline: Capture, candidate: Capture) -> list[Change]
def conditions(baseline, candidate, policy) -> list[Condition]
def load_reviews(path: Path | None) -> list[Disposition]
def load_policy(path: Path | None) -> dict
def evaluate(*, baseline, candidate, manifest_path, reviews_path, policy_path,
             out_dir, wpt_root, corpus_root, fixture_root, engine_cfg, dpi, spec) -> GateVerdict
```

Rules:

- `change_id` hashes a canonical JSON encoding of the document id, the baseline
  document's page fingerprints, and the candidate document's page fingerprints.
  Any change to either side's output produces a different id.
- `compare` reports one `Change` per changed property per document. Page
  fingerprint differences produce a `rendered_image` change; a page-count change
  produces a `page_count` change as well.
- `conditions` returns every blocking condition, each with the spec's stable
  name. At minimum, and each returning nonzero:
  `missing_baseline`, `missing_candidate`, `empty_selection`,
  `duplicate_document_identity`, `coverage_mismatch`, `incomplete_capture`,
  `unsupported_schema`, `unapproved_error`, `unapproved_skip`,
  `render_failure`, `incompatible_environment`, `unknown_identity_unacknowledged`,
  `unreviewed_change`, `regression_disposition`, `missing_direct_evidence`,
  `direct_check_failed`, `stale_review`, `provenance_missing`.
- Environment compatibility compares `engine_kind`, `page_spec`, `dpi`,
  `wpt_revision`, `rasterizer`, and `fonts.identity` between the two captures.
  `source_commit`, `binary`, and `label` are expected to differ; report them in
  the human output and never treat them as an incompatibility.
- `unknown_identity_unacknowledged`: when a compared identity field is
  `"unknown"` on either side, the gate blocks unless the policy file lists that
  field under `"acknowledged_unknown"`.
- Policy file (`--policy`, JSON, optional): `{"acknowledged_unknown": ["fonts"],
  "allowed_errors": ["test-id"], "allowed_skips": ["test-id"],
  "allowed_render_failures": ["doc-id"]}`. An ERROR, SKIP, or render failure not
  listed there blocks the gate.
- Direct evidence: `missing_direct_evidence` when the manifest path is absent or
  the manifest is empty; `direct_check_failed` when any check fails (name the
  check id in the detail).
- Dispositions from `load_reviews` apply only when both the `doc_id` and the
  `change_id` match. A review whose ids no longer match any change is a
  `stale_review` condition, but only when the file is the one selected for this
  comparison. `regression_disposition` blocks. `correction` and `variation`
  accept the change; a `variation` without a `provenance` string blocks.
- On a passing verdict write **no new state**. On a failing verdict write a
  report (`<out_dir>/<candidate-label>-gate.json`) that lists the conditions,
  the changes, the direct results, and both identities. Also write one
  side-by-side PNG per `rendered_image` change (`<out_dir>/changes/<slug>.png`,
  baseline | candidate) so a reviewer can see the difference.
- `evaluate` also resolves a bare label against `out_dir/../captures`.

### `harness/cli.py` additions

```
python -m harness baseline --engine cli --cli-cmd "..." --label NAME --out DIR
    [--filter F] [--limit N] [--workers N] [--wpt PATH] [--dpi 96]
```

Renders the selected set, writes `<out>/<label>.json`, prints a capture summary,
and prints the literal line `BASELINE CAPTURED — not a gate result`. Exit 0 when
the capture is complete, 1 when it is incomplete, 2 on a usage or environment
error (no WPT checkout, bad engine kind).

```
python -m harness gate --baseline PATH|LABEL --candidate PATH|LABEL
    --captures DIR --direct harness/direct_manifest.json
    --reviews gate/reviews --policy gate/policy.json --out gate/out
    [--wpt PATH] [--corpus .] [--fixtures harness/direct_fixtures] [--dpi 96]
    [--engine cli] [--cli-cmd "..."]
```

Prints, in order: both identities, the environment-compatibility verdict, the
condition list, the change list (document, property, baseline, candidate), the
direct-check results, then `GATE PASSED` or
`GATE FAILED: N condition(s)`. Exit 0 on pass, 1 on failure, 2 on a missing or
unreadable artifact or an unsupported schema. The `gate` command must not require
an engine binary when the direct manifest is absent, but that case still fails
the gate.

## `harness/direct_manifest.json` — write exactly this

```json
{
  "schema": "typeanvil.harness.direct/1",
  "checks": [
    {
      "id": "orthogonal-writing-one-page",
      "input": "css/css-page/page-name-orthogonal-writing-003-print.html",
      "input_source": "wpt",
      "expectation": {"page_count": 1},
      "provenance": "engine/tests/page_boundaries.rs::page_change_suppressed_when_inner_mode_orthogonal_to_page_flow; docs/research/wpt-harness/core182-interior-writing-mode-scoping.md"
    },
    {
      "id": "abspos-margins-page-shape",
      "input": "abspos-margins.html",
      "input_source": "fixture",
      "expectation": {"page_count": 1},
      "provenance": "harness/direct_fixtures/abspos-margins.html header comment; CSS 2.2 10.1"
    },
    {
      "id": "abspos-margins-page-size",
      "input": "abspos-margins.html",
      "input_source": "fixture",
      "expectation": {"page_size_pt": [360.0, 216.0], "tolerance_pt": 0.5},
      "provenance": "CLI geometry 5in x 3in at 72pt/in; CSS Page 3 3"
    },
    {
      "id": "abspos-margins-box-painted",
      "input": "abspos-margins.html",
      "input_source": "fixture",
      "expectation": {"paint_region": {"page": 0, "rect_pt": [54.0, 54.0, 126.0, 90.0], "min_dark_fraction": 0.9}},
      "provenance": "CSS 2.2 10.1, 10.3.7, 10.6.4; page area is (36pt,36pt)-(324pt,180pt)"
    },
    {
      "id": "abspos-margins-corner-blank",
      "input": "abspos-margins.html",
      "input_source": "fixture",
      "expectation": {"paint_region": {"page": 0, "rect_pt": [0.0, 0.0, 50.0, 50.0], "max_dark_fraction": 0.02}},
      "provenance": "CSS Page 3 3: content must not paint in the page margin"
    },
    {
      "id": "report-corpus-rows-preserved",
      "input": "demo/corpus/report.html",
      "input_source": "corpus",
      "expectation": {"text_present": ["Foundations", "Methods", "Results", "Discussion"]},
      "provenance": "reviewed corpus: demo/corpus/report.html chapter headings are stable inputs; docs/specifications/string-set-running-headers.spec.md"
    },
    {
      "id": "report-corpus-page-count",
      "input": "demo/corpus/report.html",
      "input_source": "corpus",
      "expectation": {"page_count": [1, 8]},
      "provenance": "reviewed corpus: bounded range for a report of this length; measured 2026-09-15"
    },
    {
      "id": "invoice-corpus-rows-preserved",
      "input": "demo/corpus/invoice.html",
      "input_source": "corpus",
      "expectation": {"text_present": ["Anvil, standard (150 lb)", "Total due", "4,142.00 USD"]},
      "provenance": "reviewed corpus: demo/corpus/invoice.html rows are stable inputs; docs/specifications/tables-fragmentation.spec.md"
    },
    {
      "id": "invoice-corpus-page-count",
      "input": "demo/corpus/invoice.html",
      "input_source": "corpus",
      "expectation": {"page_count": [1, 4]},
      "provenance": "reviewed corpus: bounded range for an invoice of this length; measured 2026-09-15"
    },
    {
      "id": "report-links-link-target",
      "input": "report-links.html",
      "input_source": "fixture",
      "expectation": {"link_target": {"page": 0, "uri": "https://example.com/typeanvil/quarterly"}},
      "provenance": "harness/direct_fixtures/report-links.html; docs/specifications/hyperlinks.spec.md"
    },
    {
      "id": "report-links-heading-present",
      "input": "report-links.html",
      "input_source": "fixture",
      "expectation": {"text_present": ["Quarterly Summary", "Rows preserved"]},
      "provenance": "harness/direct_fixtures/report-links.html reviewed contents"
    }
  ]
}
```

## Tests you must write

Tests must be **hermetic**: no engine binary, no WPT checkout, no Playwright, no
network. Synthetic captures are built inline as dicts. Label every synthetic
fault test with a `# SYNTHETIC FAULT` comment and make its name say so.

Add a small PDF builder used by `tests/test_direct.py` (put it in
`tests/pdf_fixture.py`): hand-write minimal PDFs with a content stream using a
standard font (`/Helvetica`), supporting: text at a position, a filled black
rectangle, a vertical text run, a page of a given MediaBox size, and a `/Link`
annotation with a URI. Verify it in-test with pypdfium2 (the builder is the
fixture; the assertions are real).

`tests/test_capture.py`
- fingerprint stability: the same PDF twice yields the same fingerprints.
- page size recorded in points for a 360x216pt page.
- `documents_for` deduplicates a shared reference and records both roles on the
  test document. Use the checked-in fixtures under `tests/fixtures/wpt/`.
- `load_capture` rejects an unsupported schema and a malformed file.
- `binary_identity` returns a stable sha256 for a real executable (use
  `/bin/sh`), and the all-`unknown` dict when `cli_cmd` is None.
- `source_commit`/`wpt_revision` return `"unknown"` for a non-repository path.

`tests/test_direct.py`
- required text removed → fails, and the detail names the missing string.
- paint region with text where a blank region is expected → fails.
- wrong orientation (horizontal text judged vertical) → fails.
- link target present → passes; missing URI → fails.
- page count and page size mismatches → fail with the actual value in the detail.
- an expectation with no recognised check key → `DirectError`.
- empty provenance → `DirectError`.

`tests/test_gate.py`
- **The headline case.** SYNTHETIC FAULT: baseline has the test document and its
  reference at 1 page each and every WPT status `PASS`; candidate has both at 2
  pages and every WPT status still `PASS`. The gate fails and the conditions name
  both changed documents.
- unchanged compatible control passes with no review file.
- dispositions: no disposition → fails; a `correction` bound to the change id →
  passes; change either side's fingerprint afterwards → the same record is stale
  and the gate fails again.
- one parametrized case per blocking condition: missing baseline, missing
  candidate, empty selection, duplicate document identity, coverage mismatch,
  incomplete capture, unsupported schema, unapproved ERROR, unapproved SKIP,
  render failure, incompatible page geometry, incompatible DPI, incompatible WPT
  revision, unknown font identity without acknowledgement, missing direct
  manifest, failing direct check, `variation` without provenance.
- a passing verdict writes no new files.
- a failing verdict writes a report naming every condition.

## Verification you must run

```bash
cd /Users/elijah/workspace/typeanvil.worktrees/core-206
/Users/elijah/workspace/typeanvil/.venv/bin/python -m pytest tests/ -q
/Users/elijah/workspace/typeanvil/.venv/bin/python -m harness --help
/Users/elijah/workspace/typeanvil/.venv/bin/python -m harness baseline --help
/Users/elijah/workspace/typeanvil/.venv/bin/python -m harness gate --help
python3 scripts/validate_docs.py
```

All existing tests must still pass, unchanged. Do not edit any existing test.

## Forbidden

- Do not run `python -m harness run`, `fetch`, or `triage`. They need an engine
  binary and a multi-minute WPT run; the parent runs those.
- Do not edit anything under `engine/`, `docs/`, `docsite/`, `demo/`, `scripts/`,
  `npm/`, or `probe/`. The parent syncs documentation.
- Do not modify `pyproject.toml` or add dependencies.
- Do not modify existing tests or existing module behaviour.
- Do not modify the fixture files under `harness/direct_fixtures/` — their
  comments state the reviewed expectations.
- Do not use the network.
- Do not commit. Leave the work in the working tree.
- Do not weaken a check to make it pass. If an expectation in the manifest looks
  wrong, implement it as written and report the disagreement in your summary.
