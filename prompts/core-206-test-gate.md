# Task: write `tests/test_gate.py` for the CORE-206 release gate

One file only: `tests/test_gate.py` in this worktree
(`/Users/elijah/workspace/typeanvil.worktrees/core-206`). A previous run was cut
short by a connection error after writing everything else; **do not modify any
other file**. Everything else is done and its tests pass.

Interpreter: `/Users/elijah/workspace/typeanvil/.venv/bin/python`.

## Read first

1. `harness/release_gate.py` — the module under test (`Change`, `Condition`,
   `Disposition`, `GateVerdict`, `change_id`, `compare`, `conditions`,
   `load_reviews`, `load_policy`, `evaluate`).
2. `harness/capture.py` — `Capture`, `CaptureIdentity`, `DocumentCapture`,
   `PageCapture`, `write_capture`.
3. `harness/direct.py` — `load_manifest`, `check_pdf`, `run_manifest`.
4. `docs/specifications/harness-release-gate.spec.md` — **the contract.** The
   Acceptance Criteria section lists the tests this file must provide.
5. `tests/test_capture.py` and `tests/test_direct.py` — match this style
   (`from pdf_fixture import Page, build_pdf` works; `tests/` is on the path).

## Hermetic requirements

No engine binary, no WPT checkout, no Playwright, no network. Runs on any machine
with the repo venv.

## Helpers to write in the file

```python
def page(fp: str, size=(360.0, 216.0)) -> PageCapture
def doc(doc_id: str, fingerprints: list[str], *, roles=("test",),
        page_count=None, render_failed=False, message="") -> DocumentCapture
def capture(label: str, docs: list[DocumentCapture], *,
            selection=("css/css-page/x-print.html",),
            results=None, complete=True, **identity_overrides) -> Capture
```

`capture()` builds a real `Capture` with a `CaptureIdentity` whose fields are:
`engine_kind="cli"`, `cli_cmd="fake render"`, `source_commit=<distinct per
label>`, `binary={"path": ..., "sha256": <distinct per label>, "version": "test"}`,
`wpt_revision="wptrev1"`, `page_spec={"width_in": 5.0, "height_in": 3.0,
"margin_top_in": 0.5, "margin_right_in": 0.5, "margin_bottom_in": 0.5,
"margin_left_in": 0.5}`, `dpi=96`, `rasterizer="pypdfium2 test"`,
`fonts={"identity": "unknown", "source": "unavailable"}`. Let callers override any
of these to create the incompatible cases.

## The direct layer needs a stub, not a real engine

`evaluate()` always assesses direct evidence, so a *passing* verdict needs a
direct manifest whose checks pass. Do not weaken the gate and do not skip direct
evidence. Instead pass a duck-typed stub engine:

```python
class StubEngine:
    """Returns a fixture PDF per input file name."""
    def render_pdf(self, html_path, page): ...
    def close(self): ...

class StubEngineConfig:
    def build(self): return StubEngine()
```

`StubEngine.render_pdf` ignores `html_path`/`page` and returns
`build_pdf(page_obj)` from `pdf_fixture` for the page named by
`Path(html_path).name` (keep a small dict). `engine_cfg.build()` returning this
object is all the gate uses. Write a tmp manifest with checks the stub
satisfies, e.g. `{"id": "pc", "input": "stub.html", "input_source": "fixture",
"expectation": {"page_count": 1}, "provenance": "synthetic"}`.

Use `pytest.raises` or a direct call for the malformed-manifest cases.

## Tests to write

Label every synthetic fault with a `# SYNTHETIC FAULT` comment.

1. `test_shared_page_count_change_blocks_gate` — **the headline case.** SYNTHETIC
   FAULT: baseline has document `a.html` (test, 1 page) and `b.html` (reference,
   1 page) with every WPT status `PASS`; candidate has both at 2 pages with every
   WPT status still `PASS`. Run `evaluate(...)`. Assert: `verdict.ok is False`,
   the conditions include `unreviewed_change`, the changes name **both** `a.html`
   and `b.html`, and each carries a `page_count` change. This is the fault the
   pair score cannot see: state in the test docstring that every WPT status is
   unchanged.

2. `test_unchanged_control_passes` — identical captures (distinct labels, same
   fingerprints) plus a passing direct manifest: `verdict.ok is True`, no
   conditions, no changes, and no files written under `out_dir`.

3. `test_disposition_required_and_bound` — a changed document with no review file
   → `unreviewed_change`. Add a `correction` disposition bound to the computed
   `change_id` → the gate passes. Then change either side's fingerprint with the
   **same** review file still in place → the gate fails again, and the old record
   shows up as `stale_review`. Use `write_capture` + the JSON review format from
   the spec.

4. `test_conditions_blocking` — one parametrized case per condition, each
   asserting the named condition appears and `verdict.ok is False`:
   `missing_baseline`, `missing_candidate`, `empty_selection`,
   `duplicate_document_identity`, `coverage_mismatch`, `incomplete_capture`,
   `unsupported_schema`, `unapproved_error`, `unapproved_skip`,
   `render_failure`, `incompatible_environment` (three variants: page geometry,
   DPI, WPT revision), `unknown_identity_unacknowledged` (fonts), and, with the
   policy file acknowledging `fonts`, no such condition.

5. `test_regression_and_variation_dispositions` — a `regression` disposition
   blocks with `regression_disposition`; a `variation` without provenance blocks
   with `provenance_missing`; a `variation` with provenance passes.

6. `test_failing_verdict_writes_report` — the report exists, is JSON, names every
   condition, and records both identities.

7. `test_direct_check_failure_blocks_gate` — the manifest checks something the
   stub PDF does not satisfy (e.g. `text_present: ["NOT PRESENT"]`) →
   `direct_check_failed` and a nonzero verdict.

8. `test_missing_direct_evidence_blocks` — `manifest_path=None` →
   `missing_direct_evidence`.

## Verification

```bash
cd /Users/elijah/workspace/typeanvil.worktrees/core-206
/Users/elijah/workspace/typeanvil/.venv/bin/python -m pytest tests/ -q
```

The whole suite must pass, including the existing 80 tests. Do not edit any other
file, do not commit, and do not weaken a check to make a test pass. If a check
you are testing looks wrong, write the test as specified and say so in your
summary.
