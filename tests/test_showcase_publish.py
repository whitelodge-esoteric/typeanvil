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


def _long_fixture(tmp_path, pages=12):
    images = tmp_path / "images"
    (images / "book").mkdir(parents=True)
    (images / "short").mkdir(parents=True)
    for n in range(1, pages + 1):
        Image.new("RGB", (400, 600), "white").save(images / "book" / f"page-{n:03d}-ta.png")
        Image.new("RGB", (400, 600), "gray").save(images / "book" / f"page-{n:03d}-pr.png")
    for n in (1, 2):
        Image.new("RGB", (400, 600), "white").save(images / "short" / f"page-{n:03d}-ta.png")
    pdfs = tmp_path / "work"
    pdfs.mkdir()
    for stem in ("book", "short"):
        for kind in ("ta", "pr"):
            (pdfs / f"{stem}-{kind}.pdf").write_bytes(b"%PDF-1.7 " + kind.encode())
    manifest = [
        {"name": "Book", "file": "book/book.html", "category": "book", "roles": ["showcase"]},
        {"name": "Short", "file": "x/short.html", "category": "x", "roles": ["showcase"]},
    ]
    return images, pdfs, manifest


def test_long_document_is_capped_and_links_full_pdfs(tmp_path):
    import re
    images, pdfs, manifest = _long_fixture(tmp_path)
    site = tmp_path / "site"
    rec = add_version(site, images, manifest, "v2026.10.1", "c", 300, 150, None,
                      max_pages=10, pdf_dir=pdfs)
    vdir = site / "versions/v2026.10.1"
    book = vdir / "book/book"
    assert sorted(p.name for p in book.glob("page-*-ta.png")) == [
        f"page-{n:03d}-ta.png" for n in range(1, 11)]
    assert not (book / "page-011-ta.png").exists()
    assert not (book / "page-012-pr.png").exists()
    readme = (book / "README.md").read_text()
    assert "Showing 10 of 12 pages" in readme
    assert "[full PDF (TypeAnvil)](full-typeanvil.pdf)" in readme
    assert "[full PDF (Prince)](full-prince.pdf)" in readme
    assert (book / "full-typeanvil.pdf").read_bytes().endswith(b"ta")
    assert (book / "full-prince.pdf").exists()
    # nothing links to an unpublished file
    for target in re.findall(r"\]\(([^)#]+)\)", readme):
        assert (book / target).exists(), target
    assert "page 11" not in readme
    # version index and perf say "10 of N"; perf keeps the real total
    assert "| 10 of 12 |" in (vdir / "README.md").read_text()
    assert rec["docs"]["book/book"] == {"pages": 12, "published_pages": 10}
    assert rec["total_pages"] == 14


def test_short_document_unchanged(tmp_path):
    images, pdfs, manifest = _long_fixture(tmp_path)
    site = tmp_path / "site"
    add_version(site, images, manifest, "v2026.10.1", "c", 300, 150, None,
                max_pages=10, pdf_dir=pdfs)
    short = site / "versions/v2026.10.1/x/short"
    readme = (short / "README.md").read_text()
    assert "Showing" not in readme
    assert not list(short.glob("*.pdf"))
    assert (short / "page-002-ta.png").exists()
    assert "| 2 |" in (site / "versions/v2026.10.1/README.md").read_text()


def test_exactly_max_pages_is_not_capped(tmp_path):
    images, pdfs, manifest = _long_fixture(tmp_path, pages=10)
    site = tmp_path / "site"
    add_version(site, images, manifest, "v2026.10.1", "c", 300, 150, None,
                max_pages=10, pdf_dir=pdfs)
    book = site / "versions/v2026.10.1/book/book"
    assert (book / "page-010-ta.png").exists()
    assert not list(book.glob("*.pdf"))


def test_max_pages_flag_and_missing_pdf(tmp_path):
    images, pdfs, manifest = _long_fixture(tmp_path)
    (pdfs / "book-ta.pdf").unlink()
    site = tmp_path / "site"
    with pytest.raises(SystemExit):
        add_version(site, images, manifest, "v2026.10.1", "c", 300, 150, None,
                    max_pages=10, pdf_dir=pdfs)
    mpath = tmp_path / "m.json"
    mpath.write_text(json.dumps(manifest))
    assert main(["--site", str(site), "--images", str(images), "--manifest", str(mpath),
                 "--tag", "v2026.10.1", "--max-pages", "0"]) == 2


def test_retention_drops_full_pdfs(tmp_path):
    images, pdfs, manifest = _long_fixture(tmp_path)
    site = tmp_path / "site"
    for tag in ("v2026.9.0", "v2026.10.0"):
        add_version(site, images, manifest, tag, "c", 300, 150, None, pdf_dir=pdfs)
    refresh_site(site, 1)
    assert not list((site / "versions/v2026.9.0").glob("*/*/*.pdf"))
    assert list((site / "versions/v2026.10.0").glob("*/*/full-typeanvil.pdf"))
