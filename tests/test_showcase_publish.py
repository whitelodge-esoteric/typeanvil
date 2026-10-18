"""scripts/showcase_publish.py: tags, retention, site tree, latest copy, perf history."""

from __future__ import annotations

import json

import pytest
from PIL import Image

from scripts.showcase_publish import (
    add_version,
    main,
    parse_tag,
    refresh_site,
    scale_for,
    sort_tags,
    split_retention,
)


def test_tag_sort_is_numeric():
    assert sort_tags(["v2026.9.0", "v2026.10.0", "v2026.10.1"]) == [
        "v2026.10.1", "v2026.10.0", "v2026.9.0"]


def test_bad_tag_rejected():
    with pytest.raises(ValueError):
        parse_tag("2026.9.0")
    with pytest.raises(ValueError):
        parse_tag("v2026.09")


def test_retention_split():
    full, thumbs = split_retention(["v2026.1.0", "v2026.2.0", "v2026.3.0"], 2)
    assert full == ["v2026.3.0", "v2026.2.0"]
    assert thumbs == ["v2026.1.0"]
    with pytest.raises(ValueError):
        split_retention([], 0)


def test_scale_never_upscales():
    assert scale_for(300, 150) == 0.5
    assert scale_for(150, 200) == 1.0


def _fixture(tmp_path):
    images = tmp_path / "images"
    (images / "report").mkdir(parents=True)
    for n in (1, 2):
        Image.new("RGB", (400, 600), "white").save(images / "report" / f"page-{n:03d}-ta.png")
    Image.new("RGB", (400, 600), "gray").save(images / "report" / "page-001-pr.png")
    manifest = [
        {"name": "Technical Report", "file": "book/report.html", "category": "book",
         "roles": ["showcase"]},
        {"name": "Gate only", "file": "x/gate.html", "category": "x", "roles": ["gate"]},
    ]
    return images, manifest


def test_publish_builds_tree_and_latest(tmp_path):
    images, manifest = _fixture(tmp_path)
    site = tmp_path / "site"
    rec = add_version(site, images, manifest, "v2026.10.0", "abc1234567", 300, 150, 12.5)
    assert rec["total_pages"] == 2
    refresh_site(site, 5)
    doc = (site / "versions/v2026.10.0/book/report/README.md").read_text()
    assert "| 1 | ![TypeAnvil page 1](page-001-ta.png) | ![Prince page 1](page-001-pr.png) |" in doc
    assert "not rendered" in doc  # page 2 has no oracle
    with Image.open(site / "versions/v2026.10.0/book/report/page-001-ta.png") as im:
        assert im.size == (200, 300)  # 300 -> 150 DPI
    assert (site / "latest/book/report/README.md").exists()
    assert json.loads((site / "perf/v2026.10.0.json").read_text())["render_seconds"] == 12.5
    assert "latest/README.md" in (site / "README.md").read_text()
    assert (site / "perf/chart.svg").read_text().startswith("<svg")


def test_retention_turns_old_versions_into_thumbnails(tmp_path):
    images, manifest = _fixture(tmp_path)
    site = tmp_path / "site"
    for tag in ("v2026.9.0", "v2026.10.0"):
        add_version(site, images, manifest, tag, "c", 300, 150, None)
    full = refresh_site(site, 1)
    assert full == ["v2026.10.0"]
    old = site / "versions/v2026.9.0"
    assert not list(old.glob("*/*/page-*-pr.png"))
    with Image.open(old / "book/report/page-001-ta.png") as im:
        assert im.width == 240
    assert "thumbnails only" in (site / "README.md").read_text()
    assert (site / "perf/v2026.9.0.json").exists()  # history survives retention
    refresh_site(site, 1)  # idempotent


def test_rerun_of_same_tag_replaces_it(tmp_path):
    images, manifest = _fixture(tmp_path)
    site = tmp_path / "site"
    add_version(site, images, manifest, "v2026.10.0", "a", 300, 150, None)
    add_version(site, images, manifest, "v2026.10.0", "b", 300, 150, None)
    assert json.loads((site / "perf/v2026.10.0.json").read_text())["commit"] == "b"


def test_cli_rejects_dpi_out_of_range_and_bad_tag(tmp_path):
    images, manifest = _fixture(tmp_path)
    mf = tmp_path / "m.json"
    mf.write_text(json.dumps(manifest))
    base = ["--site", str(tmp_path / "s"), "--images", str(images), "--manifest", str(mf)]
    assert main(base + ["--tag", "v2026.10.0", "--dpi", "300"]) == 2
    assert main(base + ["--tag", "latest"]) == 2
    assert main(base + ["--tag", "v2026.10.0"]) == 0
