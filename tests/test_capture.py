"""Capture layer: fingerprinting, document enumeration, identity, and loading."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path

import pytest

from harness.capture import (
    CaptureError,
    binary_identity,
    capture_pdf,
    documents_for,
    load_capture,
    source_commit,
    wpt_revision,
)
from harness.manifest import Ref, TestCase, parse_test

FIXTURE_ROOT = Path(__file__).parent / "fixtures" / "wpt"


def test_capture_pdf_fingerprint_stable():
    from pdf_fixture import Page, build_pdf

    page = Page(360, 216)
    page.rect(10, 10, 40, 40)
    pdf = build_pdf(page)
    _, first = capture_pdf(pdf, dpi=96)
    _, second = capture_pdf(pdf, dpi=96)
    assert [p.fingerprint for p in first] == [p.fingerprint for p in second]


def test_capture_pdf_records_size_in_points():
    from pdf_fixture import Page, build_pdf

    pdf = build_pdf(Page(360, 216))
    count, pages = capture_pdf(pdf, dpi=96)
    assert count == 1
    assert pages[0].size_pt == (360.0, 216.0)


def test_documents_for_deduplicates_shared_reference():
    tests = [
        parse_test(FIXTURE_ROOT / "css" / "css-page" / "simple-print.html", FIXTURE_ROOT),
        parse_test(FIXTURE_ROOT / "css" / "css-page" / "chained-print.html", FIXTURE_ROOT),
    ]
    docs = documents_for(tests, FIXTURE_ROOT)
    ids = [doc_id for doc_id, _ in docs]
    # simple-print-ref.html is referenced by both simple-print (immediate) and
    # chained-print (chained): it appears exactly once.
    assert ids.count("css/css-page/simple-print-ref.html") == 1
    by_id = dict(docs)
    assert by_id["css/css-page/simple-print-ref.html"] == {"reference"}
    assert by_id["css/css-page/simple-print.html"] == {"test"}


def test_documents_for_records_both_roles():
    root = FIXTURE_ROOT
    a = root / "css" / "css-page" / "simple-print.html"
    b = root / "css" / "css-page" / "simple-print-ref.html"
    tc_a = TestCase(path=a.resolve(), refs=[Ref(path=b.resolve(), relation="==")])
    tc_b = TestCase(path=b.resolve(), refs=[])
    docs = dict(documents_for([tc_a, tc_b], root))
    # b.html is both a test document and a reference of a.html.
    assert docs["css/css-page/simple-print-ref.html"] == {"test", "reference"}


def test_load_capture_rejects_unsupported_schema(tmp_path):
    p = tmp_path / "cap.json"
    p.write_text(json.dumps({"schema": "other/1", "label": "x"}))
    with pytest.raises(CaptureError):
        load_capture(p)


def test_load_capture_rejects_malformed(tmp_path):
    p = tmp_path / "cap.json"
    p.write_text("{ not valid json")
    with pytest.raises(CaptureError):
        load_capture(p)


def test_binary_identity_stable_sha256():
    ident = binary_identity("/bin/sh")
    assert len(ident["sha256"]) == 64
    # The reported sha256 matches the reported path's bytes, and is stable.
    assert ident["sha256"] == hashlib.sha256(Path(ident["path"]).read_bytes()).hexdigest()
    assert binary_identity("/bin/sh")["sha256"] == ident["sha256"]


def test_binary_identity_none_is_unknown():
    assert binary_identity(None) == {
        "path": "unknown",
        "sha256": "unknown",
        "version": "unknown",
    }


def test_source_commit_and_wpt_revision_non_repo(tmp_path):
    assert source_commit(tmp_path) == "unknown"
    assert wpt_revision(tmp_path) == "unknown"


def test_source_commit_override_wins(tmp_path):
    """An explicit commit is used verbatim (the container path cannot run git)."""
    from harness.capture import CaptureIdentity, build_capture
    from harness.engine import PageSpec

    class _Cfg:
        kind = "cli"
        cli_cmd = None

        def build(self):
            class _Engine:
                def render_pdf(self, path, spec):  # pragma: no cover - no docs run
                    raise AssertionError("no documents expected")

                def close(self):
                    pass

            return _Engine()

    capture = build_capture(
        tests=[],
        wpt_root=tmp_path,
        engine_cfg=_Cfg(),
        label="x",
        spec=PageSpec.wpt_default(),
        source_commit_override="cafebabe" * 5,
    )
    assert isinstance(capture.identity, CaptureIdentity)
    assert capture.identity.source_commit == "cafebabe" * 5


def test_source_commit_comes_from_the_repo_not_the_cwd(tmp_path, monkeypatch):
    """A capture records the checkout that owns the harness, not the caller's cwd."""
    from harness.capture import repo_root_default

    root = repo_root_default()
    assert (root / "harness" / "capture.py").is_file()
    if source_commit(root) == "unknown":
        # Inside the dev container a linked worktree's .git points at a host
        # path, so git cannot resolve there. The module-root property above is
        # still checked; the container path is covered by --source-commit.
        pytest.skip("git metadata is not resolvable in this environment")
    monkeypatch.chdir(tmp_path)
    # The repository root is found from the module, so cwd is irrelevant.
    assert source_commit(repo_root_default()) == source_commit(root)
