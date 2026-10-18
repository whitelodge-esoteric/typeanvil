"""Unit tests for scripts/pr_visual_diff.py (CORE-498)."""
import importlib.util
from pathlib import Path

_spec = importlib.util.spec_from_file_location(
    "pr_visual_diff", Path(__file__).resolve().parent.parent / "scripts" / "pr_visual_diff.py")
pvd = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(pvd)


def manifest(n_items, shown=None, docs=25):
    items = [{"file": f"d/{i}.html", "page": 1, "image": f"d-{i}-p001.png", "changed_px": 5}
             for i in range(n_items)]
    shown = n_items if shown is None else shown
    return {"pr": 7, "sha": "abcdef0123456789", "documents": docs, "changed_pages": n_items,
            "shown": shown, "more": n_items - shown, "raw_base": pvd.raw_base_url(7, "abcdef0123456789"),
            "items": items}


def test_no_change_line():
    s = pvd.build_section(manifest(0))
    assert "No visual change in 25 documents." in s
    assert s.startswith(pvd.MARK_BEGIN) and s.endswith(pvd.MARK_END)


def test_changed_section_embeds_raw_urls():
    s = pvd.build_section(manifest(1))
    assert "![d/0.html p1](https://raw.githubusercontent.com/" in s
    assert "/pr-assets/pr-7/abcdef01/d-0-p001.png)" in s


def test_cap_adds_more_line():
    s = pvd.build_section(manifest(25, shown=20), "https://example/run")
    assert s.count("![") == 20
    assert "5 more changed page(s) not shown" in s and "https://example/run" in s


def test_replace_in_place_not_appended():
    first = pvd.replace_section("Intro\n", pvd.build_section(manifest(1)))
    second = pvd.replace_section(first, pvd.build_section(manifest(0)))
    assert second.count(pvd.MARK_BEGIN) == 1
    assert "No visual change" in second and "![" not in second
    assert second.startswith("Intro")


def test_text_after_section_is_kept():
    body = "A\n\n" + pvd.build_section(manifest(1)) + "\n\nFixes CORE-1\n"
    out = pvd.replace_section(body, pvd.build_section(manifest(0)))
    assert out.endswith("\n\nFixes CORE-1\n")


def test_three_panel_and_diff_pixels():
    from PIL import Image
    a = Image.new("RGB", (40, 30), "white")
    b = a.copy()
    for x in range(10):
        b.putpixel((x, 5), (0, 0, 0))
    img, changed = pvd.three_panel(a, b)
    assert changed == 10
    assert img.width > 3 * 40
    same, zero = pvd.three_panel(a, a.copy())
    assert zero == 0


def test_missing_page_side():
    from PIL import Image
    img, changed = pvd.three_panel(None, Image.new("RGB", (20, 20), "black"))
    assert changed == 400
