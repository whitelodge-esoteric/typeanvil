"""Typeanvil WPT conformance harness.

Runs W3C Web Platform Tests print-reftests (``css/css-page``, ``css/css-break``,
``css/css-multicol``) against an HTML/CSS -> PDF engine and produces a conformance
scoreboard.

The design mirrors the wptrunner print-reftest model documented in
``research/typeanvil-wpt-harness-brief.md`` (section 2): render test and reference to
PDF at a fixed 5in x 3in page box with 0.5in margins, rasterize each page at 96 DPI,
then per-page pixel-compare honouring ``<meta name=fuzzy>`` and
``<meta name=reftest-pages>``.

There is no Typeanvil engine yet; the harness is built first and validated by running
Chromium (via Playwright) as the engine under test -- it should score near-100%, which
proves the oracle works.
"""

__all__ = ["__version__"]

__version__ = "0.1.0"
