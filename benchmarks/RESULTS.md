# Engine benchmark — TypeAnvil vs PrinceXML vs WeasyPrint

Generated 2026-10-14T00:54:37Z · TypeAnvil `1f392a9` (release profile) · Prince 16.2 · WeasyPrint version 70.0

Environment: Apple M4, 16 GB RAM, macOS (macOS-26.5-arm64-arm-64bit )

Method: each fixture rendered at the standard demo geometry (5in x 3in pages, 0.5in margins; WeasyPrint via a matching @page stylesheet); 1 warmup run discarded, then 5 timed runs; wall-clock is the median, memory is the max peak RSS across timed runs (`/usr/bin/time -l`). Process spawn cost included for all engines.

Skipped: WeasyPrint / Book Chapter — The Cartography of Rivers — WeasyPrint 70.0 AssertionError in float_layout (upstream)

## Render speed (median wall-clock) (s)

| Fixture | ta | prince | weasy |
|---|---|---|---|
| Invoice | 0.020 | 0.050 | 0.641 |
| Quarterly Report | 0.020 | 0.052 | 0.490 |
| Academic Paper | 0.029 | 0.053 | 0.718 |
| Letterhead | 0.020 | 0.052 | 0.516 |
| Inventory Ledger — Table Fragmentation Stress | 0.084 | 0.076 | 0.831 |
| Float Showcase — Figure & Pull Quote | 0.027 | 0.057 | 0.536 |
| Prose Showcase | 0.029 | 0.052 | 0.571 |
| Invoice — Statement of Account | 0.027 | 0.050 | 0.519 |
| Consolidated Financial Statements | 0.030 | 0.067 | 0.546 |
| Monthly Analytics Digest | 0.022 | 0.052 | 0.550 |
| Board Report — Q4 FY2026 | 0.023 | 0.051 | 0.554 |
| Sales Summary by Region | 0.026 | 0.050 | 0.478 |
| Pro-Forma Invoice | 0.025 | 0.053 | 0.546 |
| Credit Note | 0.021 | 0.050 | 0.524 |
| Multi-Currency Invoice | 0.023 | 0.050 | 0.473 |
| Recurring Billing Statement | 0.040 | 0.046 | 0.497 |
| Book Chapter — The Cartography of Rivers | 0.042 | 0.052 | — | (×0.80)
| Whitepaper — Deterministic Document Rendering | 0.023 | 0.067 | 0.569 |
| Thesis Excerpt — Sediment Transport | 0.024 | 0.052 | 0.541 |
| Annual Operations Report — Long Report | 0.021 | 0.048 | 0.469 |
| Journal Paper — Stage–discharge hysteresis | 0.019 | 0.053 | 0.524 |
| Conference Paper — Sparse Sampling of Bedload | 0.020 | 0.055 | 0.509 |
| Preprint — Convergence of Iterative Page Breaking | 0.020 | 0.052 | 0.495 |
| Technical Report — Capacity Planning | 0.022 | 0.057 | 0.437 |
| Product Manual — Northwind 2400 Pallet Truck | 0.019 | 0.070 | 0.513 |
| Spec Sheet — Northwind 2400 Series | 0.023 | 0.048 | 0.444 |
| Price Catalog 2026 — Meridian Trading | 0.022 | 0.058 | 0.428 |
| Data Sheet — Fernwood Cloud Platform | 0.021 | 0.051 | 0.425 |
| Business Letter — Meridian Trading Co. | 0.018 | 0.069 | 0.389 |
| Internal Memo — Platform Migration | 0.027 | 0.045 | 0.401 |
| Cover Letter — Application | 0.020 | 0.049 | 0.431 |
| Formal Notice — Service of Notice | 0.023 | 0.044 | 0.403 |

## Peak memory (MiB)

| Fixture | ta | prince | weasy |
|---|---|---|---|
| Invoice | 13.9 | 45.1 | 79.7 |
| Quarterly Report | 14.6 | 47.3 | 82.6 |
| Academic Paper | 15.1 | 48.6 | 92.5 |
| Letterhead | 15.2 | 50.3 | 88.4 |
| Inventory Ledger — Table Fragmentation Stress | 19.4 | 51.3 | 101.5 |
| Float Showcase — Figure & Pull Quote | 15.0 | 50.6 | 88.0 |
| Prose Showcase | 15.1 | 48.7 | 87.7 |
| Invoice — Statement of Account | 15.8 | 47.7 | 82.0 |
| Consolidated Financial Statements | 15.1 | 47.4 | 82.7 |
| Monthly Analytics Digest | 14.6 | 46.8 | 82.6 |
| Board Report — Q4 FY2026 | 15.2 | 47.7 | 81.1 |
| Sales Summary by Region | 14.7 | 46.1 | 81.7 |
| Pro-Forma Invoice | 14.8 | 46.0 | 82.6 |
| Credit Note | 14.3 | 45.8 | 79.9 |
| Multi-Currency Invoice | 14.2 | 45.9 | 80.7 |
| Recurring Billing Statement | 14.4 | 45.8 | 80.5 |
| Book Chapter — The Cartography of Rivers | 15.5 | 49.2 | — | (×0.32)
| Whitepaper — Deterministic Document Rendering | 14.6 | 48.7 | 87.8 |
| Thesis Excerpt — Sediment Transport | 14.4 | 47.8 | 88.2 |
| Annual Operations Report — Long Report | 14.2 | 46.8 | 80.4 |
| Journal Paper — Stage–discharge hysteresis | 13.9 | 48.5 | 88.8 |
| Conference Paper — Sparse Sampling of Bedload | 14.2 | 48.8 | 88.7 |
| Preprint — Convergence of Iterative Page Breaking | 15.3 | 49.2 | 88.8 |
| Technical Report — Capacity Planning | 14.2 | 47.5 | 82.3 |
| Product Manual — Northwind 2400 Pallet Truck | 14.1 | 49.1 | 89.0 |
| Spec Sheet — Northwind 2400 Series | 14.1 | 46.2 | 79.9 |
| Price Catalog 2026 — Meridian Trading | 14.7 | 46.1 | 80.0 |
| Data Sheet — Fernwood Cloud Platform | 14.1 | 46.6 | 82.0 |
| Business Letter — Meridian Trading Co. | 13.9 | 44.8 | 77.9 |
| Internal Memo — Platform Migration | 13.9 | 44.7 | 78.4 |
| Cover Letter — Application | 13.3 | 46.2 | 86.1 |
| Formal Notice — Service of Notice | 14.1 | 44.7 | 78.1 |

## Page counts (sanity)

| Fixture | ta | prince | weasy |
|---|---|---|---|
| Invoice | 5 | 5 | 5 |
| Quarterly Report | 13 | 11 | 11 |
| Academic Paper | 12 | 11 | 13 |
| Letterhead | 8 | 8 | 8 |
| Inventory Ledger — Table Fragmentation Stress | 52 | 45 | 46 |
| Float Showcase — Figure & Pull Quote | 8 | 8 | 9 |
| Prose Showcase | 11 | 11 | 11 |
| Invoice — Statement of Account | 5 | 5 | 6 |
| Consolidated Financial Statements | 11 | 11 | 11 |
| Monthly Analytics Digest | 8 | 8 | 8 |
| Board Report — Q4 FY2026 | 12 | 13 | 13 |
| Sales Summary by Region | 7 | 7 | 7 |
| Pro-Forma Invoice | 17 | 15 | 17 |
| Credit Note | 11 | 10 | 11 |
| Multi-Currency Invoice | 7 | 7 | 8 |
| Recurring Billing Statement | 6 | 7 | 7 |
| Book Chapter — The Cartography of Rivers | 9 | 8 | — |
| Whitepaper — Deterministic Document Rendering | 10 | 12 | 12 |
| Thesis Excerpt — Sediment Transport | 9 | 9 | 10 |
| Annual Operations Report — Long Report | 9 | 9 | 10 |
| Journal Paper — Stage–discharge hysteresis | 9 | 6 | 6 |
| Conference Paper — Sparse Sampling of Bedload | 8 | 9 | 9 |
| Preprint — Convergence of Iterative Page Breaking | 8 | 8 | 8 |
| Technical Report — Capacity Planning | 10 | 10 | 10 |
| Product Manual — Northwind 2400 Pallet Truck | 9 | 9 | 9 |
| Spec Sheet — Northwind 2400 Series | 5 | 5 | 5 |
| Price Catalog 2026 — Meridian Trading | 7 | 4 | 4 |
| Data Sheet — Fernwood Cloud Platform | 7 | 7 | 7 |
| Business Letter — Meridian Trading Co. | 6 | 6 | 7 |
| Internal Memo — Platform Migration | 6 | 6 | 6 |
| Cover Letter — Application | 6 | 6 | 6 |
| Formal Notice — Service of Notice | 7 | 6 | 6 |

