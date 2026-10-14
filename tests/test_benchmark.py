"""Engine benchmark: known-crash skips and the WeasyPrint version probe.

The WeasyPrint leg cannot render one corpus fixture (an upstream crash in
WeasyPrint's float layout), so the leg skips it instead of recording per-run
failures. These tests pin the skip contract and the subprocess environment the
version probe needs.
"""

from __future__ import annotations

from scripts.benchmark import WEASY_KNOWN_CRASHES, skip_reason, weasy_env


# ---------------------------------------------------------------------------
# Known-crash skips
# ---------------------------------------------------------------------------


def test_known_crash_fixtures_are_skipped_for_the_weasyprint_leg_only():
    for name in WEASY_KNOWN_CRASHES:
        # The reason string is non-empty, so the leg reports a skip.
        assert skip_reason("weasy", name)
        # TypeAnvil and Prince render these fixtures, so those legs must run.
        assert skip_reason("ta", name) is None
        assert skip_reason("prince", name) is None


def test_unlisted_fixture_runs_on_every_leg():
    for engine in ("ta", "prince", "weasy"):
        assert skip_reason(engine, "Invoice") is None


# ---------------------------------------------------------------------------
# WeasyPrint subprocess environment
# ---------------------------------------------------------------------------


def test_missing_dyld_fallback_is_exported_for_an_existing_lib_dir(tmp_path, monkeypatch):
    monkeypatch.delenv("DYLD_FALLBACK_LIBRARY_PATH", raising=False)
    assert weasy_env(str(tmp_path))["DYLD_FALLBACK_LIBRARY_PATH"] == str(tmp_path)


def test_an_existing_dyld_fallback_is_kept(tmp_path, monkeypatch):
    monkeypatch.setenv("DYLD_FALLBACK_LIBRARY_PATH", "/already/set")
    assert weasy_env(str(tmp_path))["DYLD_FALLBACK_LIBRARY_PATH"] == "/already/set"


def test_no_export_when_the_lib_dir_is_absent(tmp_path, monkeypatch):
    monkeypatch.delenv("DYLD_FALLBACK_LIBRARY_PATH", raising=False)
    assert "DYLD_FALLBACK_LIBRARY_PATH" not in weasy_env(str(tmp_path / "absent"))
