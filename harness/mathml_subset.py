"""Curated MathML subset for the harness (CORE-455, split from CORE-363 AC3).

WPT ``mathml/`` holds ordinary *screen* reftests; the manifest's print-only gate
(``-print`` suffix / ``print/`` directory) selects none of them. This module lists
the reftests for the elements CORE-363 implements (mi mn mo mfrac msqrt mroot
msub msup msubsup munder mover mtable).

Most MathML tests load ``/mathml/support/feature-detection.js`` through a
``<script>``; the manifest rejects any file containing ``<script`` (no JS runtime),
so those tests cannot be selected. Only the script-free reftests below qualify.

An entry means: script-free, resolvable reference, element family in scope.
Entries are selected, not promised to pass; the recorded baseline lives in
``docs/specifications/wpt-conformance-harness.spec.md``. Removals need a note
in the issue.
"""

from __future__ import annotations

_PM = "mathml/presentation-markup/"

MATHML_SUBSET: tuple[str, ...] = tuple(
    _PM + p
    for p in (
        # -- mfrac -----------------------------------------------------------
        "fractions/frac-invalid.html",
        "fractions/frac-invalid-2.html",
        "fractions/frac-invalid-3.html",
        # -- mtable ----------------------------------------------------------
        "tables/columnspan-rowspan-001.html",
        "tables/columnspan-rowspan-002.html",
        "tables/columnspan-rowspan-003.html",
        "tables/columnspan-rowspan-004.html",
        "tables/columnspan-rowspan-005.html",
        "tables/columnspan-rowspan-006.html",
        # -- munder / mover / munderover / msup -------------------------------
        "scripts/munder-mover-align-accent-true.html",
        "scripts/munder-mover-align-accent-false.html",
        "scripts/munderover-align-accent-true.html",
        "scripts/munderover-align-accent-false.html",
        "scripts/stretchy-mover-1a.html",
        "scripts/stretchy-mover-1b.html",
        "scripts/stretchy-mover-2a.html",
        "scripts/stretchy-mover-2b.html",
        "scripts/stretchy-mover-3.html",
        "scripts/stretchy-msup-1a.html",
        "scripts/stretchy-msup-1b.html",
        "scripts/stretchy-msup-1c.html",
        "scripts/stretchy-msup-1d.html",
        # -- mo --------------------------------------------------------------
        "operators/mo-invisibleoperators.html",
        "operators/mo-lspace-rspace.html",
        "operators/mo-lspace-rspace-2.html",
        "operators/mo-lspace-rspace-4.html",
    )
)
