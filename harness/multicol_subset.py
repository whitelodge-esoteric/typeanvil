"""Curated css-multicol subset for the harness (CORE-77).

css-multicol is the largest WPT conformance surface (744 files) but almost all
of it lives as ordinary *screen* reftests: the manifest's print-only gate
(``-print`` suffix / ``print/`` directory) picks up only 3 tests. CORE-63's
Done line names a deliberate css-multicol subset via the harness — basic
balance, then spanning, then nested — so this module curates that subset.

An entry here means all of:

* Script-free with resolvable references (the print-reftest machinery can run it).
* Exercises only engine features whose ``feat:`` commit is in main. Known-gap
  features stay OUT of the list until their tickets land:
  ``column-fill: auto``, column rules, floats/abspos/flex/grid inside multicol,
  percentage heights on multicol children (CORE-66 model), overflow/scroll
  containers, list-item markers.
* Both test and reference render through this engine, so a shared unsupported
  feature would pass falsely; entries are hand-triaged to avoid that trap.

The list grows as engine features land. Removals need a note in the issue.
"""

from __future__ import annotations

# Test paths relative to the WPT root, grouped by the multicol spec's growth
# order (multicol.spec.md AC 7): basic balance -> spanning -> nested.
MULTICOL_SUBSET: tuple[str, ...] = (
    # -- Balance + fragmentation ------------------------------------------
    # Paged (@page 5x3in) balancing with break-inside:avoid column sets.
    "css/css-multicol/moz-multicol3-column-balancing-break-inside-avoid-1.html",
    # -- Spanning -----------------------------------------------------------
    # Spanner inside a list item (outside/inside markers, nested spanner).
    "css/css-multicol/multicol-span-all-list-item-001.html",
)
