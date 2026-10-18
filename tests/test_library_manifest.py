"""Document library manifest contract (docs/specifications/document-library.spec.md, CORE-488)."""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parent.parent
LIBRARY = ROOT / "library"
MANIFEST = LIBRARY / "manifest.json"
REPROS = ROOT / "repros"
SIZES = {"small", "heavy", "visual"}
ORACLES = {"prince", "weasyprint", "chromium"}
ROLES = {"gate", "perf", "showcase"}
REQUIRED = {"name", "file", "category", "size", "oracle", "roles", "geometry",
            "source", "license", "baseline_pages", "wedge_features",
            "known_limitations", "expected_deltas"}


@pytest.fixture(scope="module")
def entries() -> list[dict]:
    return json.loads(MANIFEST.read_text(encoding="utf-8"))


def test_required_fields_and_vocabulary(entries):
    assert entries
    for e in entries:
        assert REQUIRED <= set(e), (e["file"], REQUIRED - set(e))
        assert e["size"] in SIZES, e["file"]
        assert e["oracle"] in ORACLES, e["file"]
        assert e["roles"] and set(e["roles"]) <= ROLES, e["file"]
        assert isinstance(e["baseline_pages"], int) and e["baseline_pages"] >= 1, e["file"]
        g = e["geometry"]
        assert set(g) == {"width", "height", "margin"}, e["file"]
        for v in g.values():
            assert re.fullmatch(r"\d+(\.\d+)?(in|mm|cm|pt)", v), (e["file"], v)


def test_files_exist_and_category_matches_directory(entries):
    for e in entries:
        path = LIBRARY / e["file"]
        assert path.is_file(), e["file"]
        assert path.parent.name == e["category"], e["file"]


def test_every_html_under_library_is_listed(entries):
    listed = {e["file"] for e in entries}
    on_disk = {p.relative_to(LIBRARY).as_posix() for p in LIBRARY.glob("*/*.html")}
    assert on_disk == listed


def test_file_stems_are_unique(entries):
    stems = [Path(e["file"]).stem for e in entries]
    assert len(stems) == len(set(stems))


def test_size_agrees_with_geometry_and_pages(entries):
    for e in entries:
        if "showcase" in e["roles"]:
            assert e["size"] == "visual", e["file"]
        else:
            assert e["geometry"]["width"] == "5in", e["file"]
            assert e["size"] == ("heavy" if e["baseline_pages"] >= 10 else "small"), e["file"]


def test_baseline_pages_recorded(entries):
    """Page counts measured at 570d3a5 (probe/pagecount.py, CORE-488 evidence)."""
    pages = {e["file"]: e["baseline_pages"] for e in entries}
    assert pages["financial/invoice.html"] == 5
    assert pages["reports/report.html"] == 11
    assert pages["stress/table-stress.html"] == 26
    assert pages["posters/poster.html"] == 1
    assert pages["reports/weasyprint-report.html"] == 10
    assert len(pages) == 40


def test_assets_referenced_by_fixtures_exist(entries):
    ref = re.compile(r"""(?:src="|url\(["']?)(assets/[^"')]+)""")
    for e in entries:
        path = LIBRARY / e["file"]
        for rel in ref.findall(path.read_text(encoding="utf-8")):
            assert (path.parent / rel).is_file(), f"{e['file']} -> {rel}"


def test_non_repo_licenses_are_documented(entries):
    licenses = (LIBRARY / "LICENSES.md").read_text(encoding="utf-8")
    for e in entries:
        if e["oracle"] == "weasyprint":
            assert "weasyprint-samples" in licenses.lower() or "WeasyPrint" in licenses


def test_demo_has_no_fixtures():
    demo = ROOT / "demo"
    leftovers = [p for p in demo.rglob("*") if p.is_file()] if demo.exists() else []
    assert leftovers == []


def test_repros_manifest_matches_files():
    items = json.loads((REPROS / "manifest.json").read_text(encoding="utf-8"))
    assert items
    for e in items:
        assert re.fullmatch(r"CORE-\d+-[a-z0-9-]+\.html", e["file"]), e["file"]
        assert (REPROS / e["file"]).is_file(), e["file"]
    on_disk = {p.name for p in REPROS.glob("*.html")}
    assert on_disk == {e["file"] for e in items}


def _list(*flags: str) -> list[list[str]]:
    out = subprocess.run(
        [sys.executable, "-I", str(ROOT / "scripts/demo_compare.py"), "list-manifest",
         "--manifest", str(MANIFEST), *flags],
        capture_output=True, text=True, check=True, cwd=ROOT,
    ).stdout
    return [line.split("\t") for line in out.splitlines()]


def test_filters(entries):
    assert len(_list()) == len(entries)
    gate = _list("--role", "gate", "--oracle", "prince")
    assert {r[0] for r in gate} == {e["file"] for e in entries if "gate" in e["roles"]}
    small = _list("--role", "gate", "--size", "small")
    assert small and all(
        next(e for e in entries if e["file"] == r[0])["size"] == "small" for r in small
    )
    assert {r[0] for r in _list("--category", "posters")} == {
        e["file"] for e in entries if e["category"] == "posters"
    }
    assert _list("--role", "nonexistent") == []


def test_list_manifest_emits_geometry_flags():
    rows = {r[0]: r for r in _list()}
    assert rows["financial/invoice.html"][2] == (
        "--page-width 5in --page-height 3in --margin-top 0.5in --margin-right 0.5in "
        "--margin-bottom 0.5in --margin-left 0.5in")
    assert "--page-width 8.5in --page-height 11in" in rows["posters/poster.html"][2]


def test_benchmark_loads_perf_set(entries):
    sys.path.insert(0, str(ROOT))
    from scripts.benchmark import load_corpus

    perf = load_corpus(role="perf")
    assert {f for _n, f in perf} == {e["file"] for e in entries if "perf" in e["roles"]}
    assert load_corpus(role="perf", size="heavy")
    assert load_corpus(role="perf", category="financial")
