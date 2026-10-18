"""Tests for scripts/check_llms_txt.py (CORE-507)."""
import importlib.util
from pathlib import Path

SPEC = importlib.util.spec_from_file_location(
    "check_llms_txt", Path(__file__).resolve().parent.parent / "scripts" / "check_llms_txt.py"
)
mod = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(mod)


def make_build(tmp_path, drop_md=False):
    (tmp_path / "guides").mkdir()
    (tmp_path / "sitemap.xml").write_text(
        "<urlset><url><loc>https://typeanvil.dev/guides/getting-started</loc></url></urlset>"
    )
    page = "# Getting started\n\nInstall the binary.\n"
    if not drop_md:
        (tmp_path / "guides" / "getting-started.md").write_text(page)
    (tmp_path / "llms.txt").write_text(
        "# TypeAnvil\n\nUse these docs to render.\n\nThese are not a fit for cloud.\n\n"
        "Install: https://typeanvil.dev/guides/getting-started.md\n"
    )
    (tmp_path / "llms-full.txt").write_text(page)
    return tmp_path


def test_complete_build_passes(tmp_path):
    assert mod.check(make_build(tmp_path)) == []


def test_missing_page_markdown_fails(tmp_path):
    errors = mod.check(make_build(tmp_path, drop_md=True))
    assert any("no Markdown copy" in e for e in errors)


def test_missing_artifact_fails(tmp_path):
    build = make_build(tmp_path)
    (build / "llms.txt").unlink()
    assert "missing llms.txt" in mod.check(build)
