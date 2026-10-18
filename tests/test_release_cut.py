"""Tests for scripts/release_cut.py and .github/workflows/release-cut.yml (CORE-499)."""
import importlib.util
import hashlib
import re
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parent.parent
_spec = importlib.util.spec_from_file_location("release_cut", ROOT / "scripts" / "release_cut.py")
rc = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(rc)

WORKFLOW = (ROOT / ".github/workflows/release-cut.yml").read_text()


def test_next_version_first_of_month_is_zero():
    assert rc.next_version("release/2026.10", ["v2026.9.5"]) == "v2026.10.0"


def test_next_version_increments_numerically():
    tags = ["v2026.9.0", "v2026.9.2", "v2026.9.10", "v2026.10.3"]
    assert rc.next_version("origin/release/2026.9", tags) == "v2026.9.11"


def test_next_version_ignores_foreign_tags():
    assert rc.next_version("release/2026.9", ["nightly", "v2026.9.1-rc1", "v2025.9.7"]) == "v2026.9.0"


def test_next_version_rejects_bad_branch():
    with pytest.raises(ValueError):
        rc.next_version("main", [])


def test_rotation_names_plain_month_and_year_wrap():
    assert rc.rotation_names(2026, 10) == ("release/2026.10", "release/2026.9")
    assert rc.rotation_names(2027, 1) == ("release/2027.1", "release/2026.12")


def test_notes_grouped_by_label_with_other_last():
    prs = [
        {"number": 5, "title": "Fix flex", "labels": [{"name": "engine"}]},
        {"number": 3, "title": "Typo", "labels": [{"name": "docs"}]},
        {"number": 4, "title": "Mystery", "labels": []},
        {"number": 2, "title": "Fix tables", "labels": ["engine"]},
    ]
    text = rc.render_notes(prs, "v2026.10.0", "https://x/showcase")
    assert text.index("## Engine") < text.index("## Documentation") < text.index("## Other")
    assert text.index("(#2)") < text.index("(#5)")
    assert "Showcase: https://x/showcase" in text
    assert text.startswith("# Typeanvil 2026.10.0")


def test_notes_empty():
    assert "No merged pull requests" in rc.render_notes([])


def test_verify_sums(tmp_path):
    f = tmp_path / "a.tar.gz"
    f.write_bytes(b"data")
    good = f"{hashlib.sha256(b'data').hexdigest()}  a.tar.gz\n"
    assert rc.verify_sums(good, [f]) == []
    assert rc.verify_sums("0" * 64 + "  a.tar.gz\n", [f])
    assert rc.verify_sums("", [f]) == ["a.tar.gz: not listed in SHA256SUMS"]


def test_workflow_triggers_and_jobs():
    assert re.search(r"^  schedule:", WORKFLOW, re.M)
    assert re.search(r"^  workflow_dispatch:", WORKFLOW, re.M)
    for job in ("cut:", "verify:", "rotate:"):
        assert re.search(rf"^  {job}", WORKFLOW, re.M), job
    assert "git tag -a" in WORKFLOW
    assert "SHA256SUMS" in WORKFLOW
    assert "--version" in WORKFLOW
    assert "RELEASE_BOT_TOKEN" in WORKFLOW


def test_workflow_never_force_pushes():
    assert "--force" not in WORKFLOW and " -f " not in WORKFLOW



def test_workflow_repo_paths_exist():
    refs = set(re.findall(r"\b((?:library|docs|scripts|tests|engine|harness)/[A-Za-z0-9_./-]+\.[A-Za-z0-9]+)", WORKFLOW))
    assert "library/financial/invoice-statement.html" in refs
    assert "library/SHOWCASE.md" in refs
    missing = sorted(p for p in refs if not (ROOT / p).exists())
    assert not missing, missing
