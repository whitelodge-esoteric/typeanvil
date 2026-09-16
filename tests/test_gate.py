"""Release-gate blocking conditions, dispositions, and verdict writing.

Every capture here is synthetic: built from fingerprints, review records, and a
stub engine. No engine binary, no WPT checkout, no Playwright, no network.

Tests that inject a deliberate defect are labelled ``# SYNTHETIC FAULT``. A
synthetic fault proves the gate's mechanism. It is not an engine conformance
result.

The direct layer always runs, so a *passing* verdict needs a manifest whose
checks pass. ``StubEngine`` supplies the PDFs: it returns a fixture PDF for any
input name, so the real ``harness.direct`` path is exercised end to end.
"""

from __future__ import annotations

import hashlib
import json
from pathlib import Path

import pytest

from harness.capture import (
    Capture,
    CaptureIdentity,
    DocumentCapture,
    PageCapture,
)
from harness.engine import PageSpec
from harness.release_gate import REVIEW_SCHEMA, change_id, evaluate

from pdf_fixture import Page, build_pdf

PAGE_SPEC = {
    "width_in": 5.0,
    "height_in": 3.0,
    "margin_top_in": 0.5,
    "margin_right_in": 0.5,
    "margin_bottom_in": 0.5,
    "margin_left_in": 0.5,
}
SELECTION = ("css/css-page/x-print.html",)


# ---------------------------------------------------------------------------
# Synthetic capture builders
# ---------------------------------------------------------------------------


def doc(
    doc_id: str,
    fingerprints: list[str],
    roles: tuple[str, ...] = ("test",),
    *,
    page_count: int | None = None,
    render_failed: bool = False,
    message: str = "",
) -> DocumentCapture:
    pages = tuple(
        PageCapture(index=i, size_pt=(360.0, 216.0), fingerprint=fp)
        for i, fp in enumerate(fingerprints)
    )
    return DocumentCapture(
        doc_id=doc_id,
        role=roles,
        content_sha256=hashlib.sha256(doc_id.encode()).hexdigest(),
        page_count=len(fingerprints) if page_count is None else page_count,
        pages=pages,
        render_failed=render_failed,
        message=message,
    )


def capture(
    label: str,
    docs: list[DocumentCapture],
    *,
    selection: tuple[str, ...] = SELECTION,
    results: tuple[dict, ...] | None = None,
    complete: bool = True,
    **identity_overrides,
) -> Capture:
    """Build a synthetic capture. ``label`` seeds distinct source/binary ids."""
    identity = {
        "engine_kind": "cli",
        "cli_cmd": "fake render",
        "source_commit": f"commit-{label}",
        "binary": {
            "path": f"/fake/{label}",
            "sha256": hashlib.sha256(label.encode()).hexdigest(),
            "version": "test",
        },
        "wpt_revision": "wptrev1",
        "page_spec": dict(PAGE_SPEC),
        "dpi": 96,
        "rasterizer": "pypdfium2 test",
        "fonts": {"identity": "unknown", "source": "unavailable"},
    }
    identity.update(identity_overrides)
    if results is None:
        results = tuple({"id": t, "status": "PASS"} for t in selection)
    return Capture(
        label=label,
        complete=complete,
        identity=CaptureIdentity(**identity),
        selection=tuple(selection),
        documents=tuple(docs),
        results=tuple(results),
    )


# ---------------------------------------------------------------------------
# Stub engine and file-backed evidence
# ---------------------------------------------------------------------------


class StubEngine:
    """Returns a one-page fixture PDF for any input."""

    def render_pdf(self, html_path, page):
        return build_pdf(Page(360, 216))

    def close(self) -> None:
        pass


class StubEngineConfig:
    def build(self):
        return StubEngine()


def write_policy(tmp_path: Path, **overrides) -> Path:
    data = {
        "schema": "typeanvil.harness.policy/1",
        "acknowledged_unknown": ["fonts"],
        "allowed_errors": [],
        "allowed_skips": [],
        "allowed_render_failures": [],
    }
    data.update(overrides)
    path = tmp_path / "policy.json"
    path.write_text(json.dumps(data))
    return path


def write_manifest(tmp_path: Path, expectation: dict | None = None) -> Path:
    path = tmp_path / "direct.json"
    path.write_text(
        json.dumps(
            {
                "schema": "typeanvil.harness.direct/1",
                "checks": [
                    {
                        "id": "stub-page-count",
                        "input": "stub.html",
                        "input_source": "fixture",
                        "expectation": expectation or {"page_count": 1},
                        "provenance": "synthetic test stub",
                    }
                ],
            }
        )
    )
    return path


def write_reviews(tmp_path: Path, name: str, dispositions: list[dict]) -> Path:
    path = tmp_path / f"reviews-{name}.json"
    path.write_text(
        json.dumps(
            {
                "schema": REVIEW_SCHEMA,
                "candidate": {"label": name},
                "baseline": {"label": "base"},
                "dispositions": dispositions,
            }
        )
    )
    return path


def run_gate(
    baseline,
    candidate,
    tmp_path: Path,
    *,
    manifest=None,
    reviews=None,
    policy=None,
    with_engine: bool = True,
):
    return evaluate(
        baseline=baseline,
        candidate=candidate,
        manifest_path=None if manifest is None else str(manifest),
        reviews_path=None if reviews is None else str(reviews),
        policy_path=None if policy is None else str(policy),
        out_dir=tmp_path / "out",
        wpt_root=tmp_path / "wpt",
        corpus_root=tmp_path,
        fixture_root=tmp_path,
        engine_cfg=StubEngineConfig() if with_engine else None,
        dpi=96,
        spec=PageSpec.wpt_default(),
    )


def names(verdict) -> set[str]:
    return {c.name for c in verdict.conditions}


# ---------------------------------------------------------------------------
# The headline case
# ---------------------------------------------------------------------------


def test_shared_page_count_change_blocks_gate(tmp_path):
    """SYNTHETIC FAULT: both documents of the pair move 1 page -> 2 pages.

    Every WPT status stays PASS on both sides. The pair comparison is
    self-consistent, so it sees nothing; the per-document comparison must fail
    and name both changed documents.
    """
    base = capture(
        "base", [doc("a.html", ["a1"]), doc("b.html", ["b1"], ("reference",))]
    )
    cand = capture(
        "cand",
        [doc("a.html", ["a1", "a2"]), doc("b.html", ["b1", "b2"], ("reference",))],
    )
    verdict = run_gate(
        base, cand, tmp_path, manifest=write_manifest(tmp_path), policy=write_policy(tmp_path)
    )
    assert verdict.ok is False
    assert "unreviewed_change" in names(verdict)
    assert {c.doc_id for c in verdict.changes} == {"a.html", "b.html"}
    assert {c.property for c in verdict.changes} >= {"page_count", "rendered_image"}
    # Every recorded status is unchanged: the score cannot see this fault.
    assert [r["status"] for r in base.results] == [r["status"] for r in cand.results]


def test_unchanged_control_passes_and_writes_nothing(tmp_path):
    docs = [doc("a.html", ["a1"]), doc("b.html", ["b1"], ("reference",))]
    base = capture("base", docs)
    cand = capture("cand", docs)
    verdict = run_gate(
        base, cand, tmp_path, manifest=write_manifest(tmp_path), policy=write_policy(tmp_path)
    )
    assert verdict.ok is True
    assert verdict.conditions == []
    assert verdict.changes == []
    assert verdict.report_path is None
    assert not (tmp_path / "out").exists(), "a passing verdict writes no new state"


# ---------------------------------------------------------------------------
# Dispositions
# ---------------------------------------------------------------------------


def test_disposition_required_and_bound(tmp_path):
    base = capture("base", [doc("a.html", ["a1"])])
    cand = capture("cand", [doc("a.html", ["a2"])])
    manifest = write_manifest(tmp_path)
    policy = write_policy(tmp_path)

    verdict = run_gate(base, cand, tmp_path, manifest=manifest, policy=policy)
    assert verdict.ok is False
    assert "unreviewed_change" in names(verdict)

    cid = change_id("a.html", base.documents[0], cand.documents[0])
    reviews = write_reviews(
        tmp_path,
        "cand",
        [
            {
                "doc_id": "a.html",
                "change_id": cid,
                "kind": "correction",
                "reason": "the new page count is the correct one",
                "provenance": "docs/specifications/paged-media-css.spec.md",
            }
        ],
    )
    accepted = run_gate(
        base, cand, tmp_path, manifest=manifest, policy=policy, reviews=reviews
    )
    assert accepted.ok is True, [c.name for c in accepted.conditions]

    # The same record must not carry forward to different output: change either
    # side's fingerprints and the gate blocks again.
    moved = capture("cand", [doc("a.html", ["a3"])])
    stale = run_gate(
        base, moved, tmp_path, manifest=manifest, policy=policy, reviews=reviews
    )
    assert stale.ok is False
    assert "unreviewed_change" in names(stale)
    assert "stale_review" in names(stale)


def test_reviews_directory_is_the_normal_case(tmp_path):
    """`gate/reviews` is a DIRECTORY of records; loading one must not crash.

    Regression: ``load_reviews`` treated every existing path as a file, so the
    documented ``--reviews gate/reviews`` call died with ``IsADirectoryError``
    before any condition was evaluated.
    """
    from harness.release_gate import load_reviews

    base = capture("base", [doc("a.html", ["a1"])])
    cand = capture("cand", [doc("a.html", ["a2"])])
    manifest = write_manifest(tmp_path)
    policy = write_policy(tmp_path)

    reviews_dir = tmp_path / "reviews"
    reviews_dir.mkdir()

    # An empty directory is not an error: nothing is reviewed yet, so the
    # change stays unreviewed rather than passing.
    assert load_reviews(reviews_dir) == []
    unreviewed = run_gate(
        base, cand, tmp_path, manifest=manifest, policy=policy, reviews=reviews_dir
    )
    assert unreviewed.ok is False
    assert "unreviewed_change" in names(unreviewed)

    # A record placed in the directory (the documented layout) binds.
    cid = change_id("a.html", base.documents[0], cand.documents[0])
    (reviews_dir / "cand.json").write_text(
        json.dumps(
            {
                "schema": REVIEW_SCHEMA,
                "candidate": {"label": "cand"},
                "baseline": {"label": "base"},
                "dispositions": [
                    {
                        "doc_id": "a.html",
                        "change_id": cid,
                        "kind": "correction",
                        "reason": "the new page count is the correct one",
                        "provenance": "docs/specifications/paged-media-css.spec.md",
                    }
                ],
            }
        )
    )
    assert len(load_reviews(reviews_dir)) == 1
    accepted = run_gate(
        base, cand, tmp_path, manifest=manifest, policy=policy, reviews=reviews_dir
    )
    assert accepted.ok is True, [c.name for c in accepted.conditions]


def test_regression_disposition_blocks(tmp_path):
    base = capture("base", [doc("a.html", ["a1"])])
    cand = capture("cand", [doc("a.html", ["a2"])])
    cid = change_id("a.html", base.documents[0], cand.documents[0])
    reviews = write_reviews(
        tmp_path,
        "cand",
        [{"doc_id": "a.html", "change_id": cid, "kind": "regression", "reason": "unwanted"}],
    )
    verdict = run_gate(
        base,
        cand,
        tmp_path,
        manifest=write_manifest(tmp_path),
        policy=write_policy(tmp_path),
        reviews=reviews,
    )
    assert verdict.ok is False
    assert "regression_disposition" in names(verdict)


def test_variation_disposition_requires_provenance(tmp_path):
    base = capture("base", [doc("a.html", ["a1"])])
    cand = capture("cand", [doc("a.html", ["a2"])])
    cid = change_id("a.html", base.documents[0], cand.documents[0])
    manifest = write_manifest(tmp_path)
    policy = write_policy(tmp_path)

    without = write_reviews(
        tmp_path,
        "cand-noprov",
        [{"doc_id": "a.html", "change_id": cid, "kind": "variation", "reason": "rasteriser"}],
    )
    blocked = run_gate(
        base, cand, tmp_path, manifest=manifest, policy=policy, reviews=without
    )
    assert blocked.ok is False
    assert "provenance_missing" in names(blocked)

    with_prov = write_reviews(
        tmp_path,
        "cand-prov",
        [
            {
                "doc_id": "a.html",
                "change_id": cid,
                "kind": "variation",
                "reason": "rasteriser edge antialiasing",
                "provenance": "CORE-180 raster/vector edge difference",
            }
        ],
    )
    accepted = run_gate(
        base, cand, tmp_path, manifest=manifest, policy=policy, reviews=with_prov
    )
    assert accepted.ok is True, [c.name for c in accepted.conditions]


# ---------------------------------------------------------------------------
# Blocking conditions
# ---------------------------------------------------------------------------


def _clean_pair():
    return capture("base", [doc("a.html", ["a1"])]), capture("cand", [doc("a.html", ["a1"])])


def test_missing_baseline_blocks(tmp_path):
    _, cand = _clean_pair()
    verdict = run_gate(
        tmp_path / "absent.json",
        cand,
        tmp_path,
        manifest=write_manifest(tmp_path),
        policy=write_policy(tmp_path),
    )
    assert verdict.ok is False
    assert "missing_baseline" in names(verdict)


def test_missing_candidate_blocks(tmp_path):
    base, _ = _clean_pair()
    verdict = run_gate(
        base,
        tmp_path / "absent.json",
        tmp_path,
        manifest=write_manifest(tmp_path),
        policy=write_policy(tmp_path),
    )
    assert verdict.ok is False
    assert "missing_candidate" in names(verdict)


def test_empty_selection_blocks(tmp_path):
    base = capture("base", [doc("a.html", ["a1"])], selection=())
    cand = capture("cand", [doc("a.html", ["a1"])], selection=())
    verdict = run_gate(
        base, cand, tmp_path, manifest=write_manifest(tmp_path), policy=write_policy(tmp_path)
    )
    assert verdict.ok is False
    assert "empty_selection" in names(verdict)


def test_duplicate_document_identity_blocks(tmp_path):
    base = capture("base", [doc("a.html", ["a1"]), doc("a.html", ["a1"])])
    cand = capture("cand", [doc("a.html", ["a1"]), doc("a.html", ["a1"])])
    verdict = run_gate(
        base, cand, tmp_path, manifest=write_manifest(tmp_path), policy=write_policy(tmp_path)
    )
    assert verdict.ok is False
    assert "duplicate_document_identity" in names(verdict)


def test_coverage_mismatch_blocks(tmp_path):
    base = capture("base", [doc("a.html", ["a1"]), doc("b.html", ["b1"], ("reference",))])
    cand = capture("cand", [doc("a.html", ["a1"])])
    verdict = run_gate(
        base, cand, tmp_path, manifest=write_manifest(tmp_path), policy=write_policy(tmp_path)
    )
    assert verdict.ok is False
    assert "coverage_mismatch" in names(verdict)


def test_incomplete_capture_blocks(tmp_path):
    base = capture("base", [doc("a.html", ["a1"])], complete=False)
    cand = capture("cand", [doc("a.html", ["a1"])])
    verdict = run_gate(
        base, cand, tmp_path, manifest=write_manifest(tmp_path), policy=write_policy(tmp_path)
    )
    assert verdict.ok is False
    assert "incomplete_capture" in names(verdict)


def test_unsupported_schema_blocks(tmp_path):
    bad = tmp_path / "old-capture.json"
    bad.write_text(json.dumps({"schema": "typeanvil.harness.capture/0", "label": "old"}))
    _, cand = _clean_pair()
    verdict = run_gate(
        bad, cand, tmp_path, manifest=write_manifest(tmp_path), policy=write_policy(tmp_path)
    )
    assert verdict.ok is False
    assert "unsupported_schema" in names(verdict)


def test_unapproved_error_blocks_but_policy_can_allow_it(tmp_path):
    results = ({"id": SELECTION[0], "status": "ERROR"},)
    base = capture("base", [doc("a.html", ["a1"])], results=results)
    cand = capture("cand", [doc("a.html", ["a1"])], results=results)
    manifest = write_manifest(tmp_path)

    blocked = run_gate(
        base, cand, tmp_path, manifest=manifest, policy=write_policy(tmp_path)
    )
    assert blocked.ok is False
    assert "unapproved_error" in names(blocked)

    allowed = run_gate(
        base,
        cand,
        tmp_path,
        manifest=manifest,
        policy=write_policy(tmp_path, allowed_errors=[SELECTION[0]]),
    )
    assert allowed.ok is True, [c.name for c in allowed.conditions]


def test_unapproved_skip_blocks(tmp_path):
    results = ({"id": SELECTION[0], "status": "SKIP"},)
    base = capture("base", [doc("a.html", ["a1"])], results=results)
    cand = capture("cand", [doc("a.html", ["a1"])], results=results)
    verdict = run_gate(
        base, cand, tmp_path, manifest=write_manifest(tmp_path), policy=write_policy(tmp_path)
    )
    assert verdict.ok is False
    assert "unapproved_skip" in names(verdict)


def test_render_failure_blocks(tmp_path):
    broken = doc("a.html", [], render_failed=True, message="boom")
    base = capture("base", [broken], complete=False)
    cand = capture("cand", [doc("a.html", [])], complete=False)
    verdict = run_gate(
        base, cand, tmp_path, manifest=write_manifest(tmp_path), policy=write_policy(tmp_path)
    )
    assert verdict.ok is False
    assert "render_failure" in names(verdict)


@pytest.mark.parametrize(
    "override",
    [
        {"page_spec": dict(PAGE_SPEC, height_in=2.0)},
        {"dpi": 72},
        {"wpt_revision": "other-revision"},
        {"engine_kind": "chromium"},
    ],
)
def test_incompatible_environment_blocks(tmp_path, override):
    base = capture("base", [doc("a.html", ["a1"])])
    cand = capture("cand", [doc("a.html", ["a1"])], **override)
    verdict = run_gate(
        base, cand, tmp_path, manifest=write_manifest(tmp_path), policy=write_policy(tmp_path)
    )
    assert verdict.ok is False
    assert "incompatible_environment" in names(verdict)


def test_unknown_identity_needs_acknowledgement(tmp_path):
    base = capture("base", [doc("a.html", ["a1"])])
    cand = capture("cand", [doc("a.html", ["a1"])])
    manifest = write_manifest(tmp_path)

    blocked = run_gate(
        base,
        cand,
        tmp_path,
        manifest=manifest,
        policy=write_policy(tmp_path, acknowledged_unknown=[]),
    )
    assert blocked.ok is False
    assert "unknown_identity_unacknowledged" in names(blocked)

    acknowledged = run_gate(
        base,
        cand,
        tmp_path,
        manifest=manifest,
        policy=write_policy(tmp_path, acknowledged_unknown=["fonts"]),
    )
    assert acknowledged.ok is True, [c.name for c in acknowledged.conditions]


# ---------------------------------------------------------------------------
# Direct evidence
# ---------------------------------------------------------------------------


def test_missing_direct_evidence_blocks(tmp_path):
    base, cand = _clean_pair()
    verdict = run_gate(base, cand, tmp_path, manifest=None, policy=write_policy(tmp_path))
    assert verdict.ok is False
    assert "missing_direct_evidence" in names(verdict)


def test_empty_direct_manifest_blocks(tmp_path):
    empty = tmp_path / "empty.json"
    empty.write_text(json.dumps({"schema": "typeanvil.harness.direct/1", "checks": []}))
    base, cand = _clean_pair()
    verdict = run_gate(base, cand, tmp_path, manifest=empty, policy=write_policy(tmp_path))
    assert verdict.ok is False
    assert "missing_direct_evidence" in names(verdict)


def test_direct_check_failure_blocks_gate(tmp_path):
    # SYNTHETIC FAULT: the manifest requires text the stub PDF does not contain.
    base, cand = _clean_pair()
    manifest = write_manifest(tmp_path, {"text_present": ["NOT PRESENT"]})
    verdict = run_gate(base, cand, tmp_path, manifest=manifest, policy=write_policy(tmp_path))
    assert verdict.ok is False
    assert "direct_check_failed" in names(verdict)
    assert any(not r.passed for r in verdict.direct)


# ---------------------------------------------------------------------------
# Report writing
# ---------------------------------------------------------------------------


def test_failing_verdict_writes_report(tmp_path):
    base = capture("base", [doc("a.html", ["a1"])])
    cand = capture("cand", [doc("a.html", ["a2"])])
    verdict = run_gate(
        base, cand, tmp_path, manifest=write_manifest(tmp_path), policy=write_policy(tmp_path)
    )
    assert verdict.ok is False
    assert verdict.report_path is not None
    assert verdict.report_path.exists()
    # The report lands under --out, never beside the captures.
    assert verdict.report_path.parent == tmp_path / "out"

    report = json.loads(verdict.report_path.read_text())
    reported = {c["name"] for c in report["conditions"]}
    assert "unreviewed_change" in reported
    assert report["changes"], "the report must name the changed documents"
    assert report["baseline_identity"]["source_commit"] == "commit-base"
    assert report["candidate_identity"]["source_commit"] == "commit-cand"


def test_conditions_function_is_importable_for_direct_use():
    """`conditions` stays usable on its own, without the review or direct layers."""
    from harness.release_gate import conditions as gate_conditions

    base = capture("base", [doc("a.html", ["a1"])])
    cand = capture("cand", [doc("a.html", ["a1"])], complete=False)
    found = gate_conditions(base, cand, {"acknowledged_unknown": ["fonts"]})
    assert "incomplete_capture" in {c.name for c in found}
