"""Benchmark perf role (CORE-493): real geometry, startup, scaling series."""

from __future__ import annotations

import math

from scripts import benchmark as b


def test_every_manifest_document_has_a_geometry():
    geoms = b.load_geometry()
    names = [n for n, _ in b.load_corpus("perf")]
    assert names
    assert all(n in geoms for n in names)


def test_visual_documents_are_in_the_perf_role_at_real_geometry():
    names = dict(b.perf_corpus("perf"))
    assert "Event Poster" in names
    assert "Event Poster" not in dict(b.load_corpus("perf"))
    geoms = b.load_geometry()
    assert geoms["Event Poster"]["width"] == "8.5in"
    assert geoms["Concert Ticket"]["margin"] == "0in"


def test_render_argv_uses_the_document_geometry():
    g = {"width": "6in", "height": "9in", "margin": "0.75in"}
    from pathlib import Path
    ta, _ = b.render_argv("ta", Path("a.html"), Path("a.pdf"), Path("ta"), None, g)
    assert ta[ta.index("--page-width") + 1] == "6in"
    assert ta[ta.index("--page-height") + 1] == "9in"
    assert ta[ta.index("--margin-left") + 1] == "0.75in"
    pr, _ = b.render_argv("prince", Path("a.html"), Path("a.pdf"), Path("ta"), None, g)
    assert "--page-size=6in 9in" in pr
    assert "--page-margin=0.75in" in pr


def test_default_geometry_is_unchanged_for_small_documents():
    assert b.geom_ta(b.DEFAULT_GEOMETRY) == b.GEOM_TA
    assert b.GEOM_TA[b.GEOM_TA.index("--page-width") + 1] == "5in"


def test_chrome_input_gets_the_document_page_rule(tmp_path):
    src = tmp_path / "a.html"
    src.write_text("<html><head></head><body>x</body></html>")
    dst = tmp_path / "out.html"
    b.prepare_chrome_input(src, dst, tmp_path, {"width": "6in", "height": "9in",
                                                "margin": "0.75in"})
    assert "size: 6in 9in" in dst.read_text()


def test_scaling_html_is_deterministic_and_grows_linearly():
    assert b.scaling_html(10) == b.scaling_html(10)
    small, big = b.scaling_html(10), b.scaling_html(100)
    assert big.count("<p>") == 10 * small.count("<p>")


def test_scaling_corpus_has_startup_plus_every_size(tmp_path):
    corpus, geoms = b.scaling_corpus(tmp_path)
    names = [n for n, _ in corpus]
    assert names[0] == b.STARTUP_NAME
    assert names[1:] == [b.scaling_name(n) for n in b.SCALING_SIZES]
    assert all(geoms[n] == b.SCALING_GEOMETRY for n in names)
    assert all((tmp_path / "scaling").glob("*.html"))


def test_per_page_ms_subtracts_startup():
    e = {"median_s": 1.02, "pages": 100}
    assert math.isclose(b.per_page_ms(e, 0.02), 10.0)
    assert b.per_page_ms({"median_s": 0.01, "pages": 5}, 0.02) == 0.0
    assert b.per_page_ms({}, 0.02) is None
    assert b.per_page_ms({"median_s": 1.0, "pages": 0}, 0.0) is None


def test_growth_exponent_distinguishes_linear_from_quadratic():
    linear = [(p, 0.01 * p) for p in (10, 50, 150, 300)]
    quad = [(p, 1e-4 * p * p) for p in (10, 50, 150, 300)]
    assert math.isclose(b.growth_exponent(linear), 1.0, abs_tol=1e-9)
    assert math.isclose(b.growth_exponent(quad), 2.0, abs_tol=1e-9)
    assert b.growth_exponent([(10, 1.0)]) is None
    # A noisy near-zero point under min_pages must not skew the fit.
    noisy = [(10, 1e-6)] + linear[1:]
    assert math.isclose(b.growth_exponent(noisy), 1.0, abs_tol=1e-9)


def _data():
    def e(med, pages):
        return {"median_s": med, "pages": pages, "tree_footprint_mib": 20.0}
    scaling = {b.scaling_name(n): e(0.02 + 0.001 * n, n) for n in b.SCALING_SIZES}
    return {
        "environment": {"date_utc": "2026-10-17T00:00:00Z", "platform": "macOS-26 x"},
        "engines": ["ta"],
        "typeanvil_version": "abc", "typeanvil_profile": "release",
        "corpus": [["Event Poster", "posters/event-poster.html"]],
        "heavy_names": ["Event Poster"],
        "geometries": {"Event Poster": {"width": "8.5in", "height": "11in",
                                        "margin": "0.75in"}},
        "results": {"ta": {"Event Poster": e(0.05, 2)}},
        "startup": {"ta": {b.STARTUP_NAME: {"median_s": 0.02, "peak_rss_mib": 14.0}}},
        "scaling": {"ta": scaling},
    }


def test_report_has_startup_heavy_and_scaling_sections():
    text = "\n".join(b.perf_sections(_data(), {"ta": "TypeAnvil"}))
    assert "## Startup cost (empty document)" in text
    assert "## Heavy and visual documents at real geometry" in text
    assert "8.5in×11in" in text
    assert "## Scaling series" in text
    assert "Growth exponent" in text
    assert "| ~300 pp |" in text


def test_perf_record_is_compact_and_json_safe():
    import json
    rec = b.perf_record(_data())
    json.dumps(rec)
    ta = rec["engines"]["ta"]
    assert ta["startup_s"] == 0.02
    assert set(ta["scaling_ms_per_page"]) == {b.scaling_name(n) for n in b.SCALING_SIZES}
    assert ta["growth_exponent"] is not None
