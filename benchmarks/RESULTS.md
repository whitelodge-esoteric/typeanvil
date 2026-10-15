# Engine benchmark — TypeAnvil vs PrinceXML vs WeasyPrint vs Headless Chrome

Generated 2026-10-15T01:12:43Z · TypeAnvil `9f5868f` (release profile) · Prince 16.2 · WeasyPrint version 70.0 · Google Chrome for Testing 154.0.8037.57

Environment: Apple M4, 16 GB RAM, macOS (macOS-26.5-arm64-arm-64bit-Mach-O )

Method: each fixture rendered at the standard demo geometry (5in x 3in pages, 0.5in margins). WeasyPrint uses a matching @page stylesheet; Headless Chrome receives a prepared copy of the fixture with a `<base href>` for relative assets and the same @page rule appended. 1 warmup run discarded, then 5 timed runs; wall-clock is the median, single-process memory is `/usr/bin/time -l`, process-tree memory is sampled over the whole process group. Process spawn cost included for all engines.

Skipped: WeasyPrint / Book Chapter — The Cartography of Rivers — WeasyPrint 70.0 AssertionError in float_layout (upstream)

## Render speed (median wall-clock) (s)

| Fixture | ta | prince | weasy | chrome |
|---|---|---|---|---|
| Invoice | 0.025 | 0.042 | 0.425 | 0.152 |
| Quarterly Report | 0.019 | 0.050 | 0.777 | 0.144 |
| Academic Paper | 0.031 | 0.059 | 0.864 | 0.140 |
| Letterhead | 0.019 | 0.049 | 0.798 | 0.144 |
| Inventory Ledger — Table Fragmentation Stress | 0.073 | 0.072 | 0.705 | 0.167 |
| Float Showcase — Figure & Pull Quote | 0.021 | 0.048 | 0.518 | 0.307 |
| Prose Showcase | 0.027 | 0.051 | 0.470 | 0.254 |
| Invoice — Statement of Account | 0.025 | 0.045 | 0.421 | 0.199 |
| Consolidated Financial Statements | 0.026 | 0.048 | 0.453 | 0.251 |
| Monthly Analytics Digest | 0.021 | 0.046 | 0.413 | 0.277 |
| Board Report — Q4 FY2026 | 0.021 | 0.048 | 0.419 | 0.282 |
| Sales Summary by Region | 0.024 | 0.044 | 0.445 | 0.264 |
| Pro-Forma Invoice | 0.019 | 0.047 | 0.487 | 0.300 |
| Credit Note | 0.019 | 0.044 | 0.441 | 0.281 |
| Multi-Currency Invoice | 0.021 | 0.045 | 0.445 | 0.133 |
| Recurring Billing Statement | 0.020 | 0.043 | 0.401 | 0.286 |
| Book Chapter — The Cartography of Rivers | 0.022 | 0.049 | — | 0.129 |
| Whitepaper — Deterministic Document Rendering | 0.022 | 0.051 | 0.445 | 0.127 |
| Thesis Excerpt — Sediment Transport | 0.021 | 0.049 | 0.581 | 0.130 |
| Annual Operations Report — Long Report | 0.020 | 0.053 | 0.527 | 0.141 |
| Journal Paper — Stage–discharge hysteresis | 0.017 | 0.054 | 0.507 | 0.245 |
| Conference Paper — Sparse Sampling of Bedload | 0.018 | 0.049 | 0.515 | 0.313 |
| Preprint — Convergence of Iterative Page Breaking | 0.018 | 0.047 | 0.567 | 0.282 |
| Technical Report — Capacity Planning | 0.020 | 0.046 | 0.523 | 0.280 |
| Product Manual — Northwind 2400 Pallet Truck | 0.018 | 0.054 | 1.035 | 0.319 |
| Spec Sheet — Northwind 2400 Series | 0.019 | 0.043 | 0.851 | 0.285 |
| Price Catalog 2026 — Meridian Trading | 0.021 | 0.043 | 0.473 | 0.306 |
| Data Sheet — Fernwood Cloud Platform | 0.019 | 0.045 | 0.449 | 0.190 |
| Business Letter — Meridian Trading Co. | 0.016 | 0.043 | 0.448 | 0.128 |
| Internal Memo — Platform Migration | 0.015 | 0.043 | 0.463 | 0.146 |
| Cover Letter — Application | 0.019 | 0.043 | 0.476 | 0.144 |
| Formal Notice — Service of Notice | 0.016 | 0.045 | 0.435 | 0.146 |

## Peak memory (MiB)

Largest single process. Under-reports multi-process engines (Chrome) — see the process-tree tables.

| Fixture | ta | prince | weasy | chrome |
|---|---|---|---|---|
| Invoice | 14.3 | 44.9 | 79.6 | 93.5 |
| Quarterly Report | 14.7 | 48.0 | 81.9 | 102.9 |
| Academic Paper | 15.3 | 49.1 | 93.2 | 91.7 |
| Letterhead | 15.2 | 50.3 | 88.7 | 95.3 |
| Inventory Ledger — Table Fragmentation Stress | 19.2 | 50.5 | 101.3 | 126.1 |
| Float Showcase — Figure & Pull Quote | 15.2 | 49.5 | 88.3 | 95.0 |
| Prose Showcase | 15.4 | 48.6 | 87.7 | 91.1 |
| Invoice — Statement of Account | 15.9 | 46.8 | 81.7 | 99.2 |
| Consolidated Financial Statements | 15.0 | 46.7 | 82.5 | 102.3 |
| Monthly Analytics Digest | 14.5 | 46.7 | 82.7 | 106.3 |
| Board Report — Q4 FY2026 | 15.6 | 47.6 | 80.8 | 93.4 |
| Sales Summary by Region | 14.6 | 45.8 | 81.5 | 94.2 |
| Pro-Forma Invoice | 14.4 | 45.9 | 82.3 | 96.9 |
| Credit Note | 14.1 | 45.9 | 79.7 | 93.6 |
| Multi-Currency Invoice | 14.4 | 45.9 | 80.6 | 96.3 |
| Recurring Billing Statement | 14.3 | 45.8 | 80.4 | 96.2 |
| Book Chapter — The Cartography of Rivers | 15.4 | 48.7 | — | 96.3 |
| Whitepaper — Deterministic Document Rendering | 14.9 | 48.4 | 88.0 | 95.4 |
| Thesis Excerpt — Sediment Transport | 14.5 | 47.8 | 88.2 | 96.5 |
| Annual Operations Report — Long Report | 14.4 | 46.4 | 80.5 | 95.8 |
| Journal Paper — Stage–discharge hysteresis | 14.0 | 48.5 | 88.6 | 102.0 |
| Conference Paper — Sparse Sampling of Bedload | 14.2 | 48.5 | 88.7 | 105.1 |
| Preprint — Convergence of Iterative Page Breaking | 15.3 | 48.6 | 88.6 | 99.8 |
| Technical Report — Capacity Planning | 14.3 | 46.8 | 82.5 | 105.0 |
| Product Manual — Northwind 2400 Pallet Truck | 14.1 | 48.8 | 88.8 | 112.4 |
| Spec Sheet — Northwind 2400 Series | 14.2 | 45.7 | 80.1 | 95.6 |
| Price Catalog 2026 — Meridian Trading | 14.5 | 45.9 | 80.2 | 99.2 |
| Data Sheet — Fernwood Cloud Platform | 14.2 | 46.5 | 82.0 | 104.6 |
| Business Letter — Meridian Trading Co. | 14.0 | 45.4 | 78.0 | 91.8 |
| Internal Memo — Platform Migration | 13.7 | 44.5 | 78.5 | 95.1 |
| Cover Letter — Application | 13.4 | 46.1 | 86.0 | 92.6 |
| Formal Notice — Service of Notice | 14.1 | 44.7 | 78.0 | 92.2 |

## Peak memory, whole process tree — summed RSS (MiB)

Sum of resident memory across every process the render spawned; shared pages are counted once per process, so this over-counts multi-process engines.

| Fixture | ta | prince | weasy | chrome |
|---|---|---|---|---|
| Invoice | 14.3 | 44.9 | 81.7 | 356.0 |
| Quarterly Report | 14.7 | 48.0 | 84.1 | 371.6 |
| Academic Paper | 15.3 | 49.1 | 95.4 | 355.5 |
| Letterhead | 15.2 | 50.3 | 90.9 | 360.8 |
| Inventory Ledger — Table Fragmentation Stress | 19.2 | 50.5 | 103.5 | 405.1 |
| Float Showcase — Figure & Pull Quote | 15.2 | 49.5 | 90.5 | 359.3 |
| Prose Showcase | 15.4 | 48.6 | 89.8 | 354.5 |
| Invoice — Statement of Account | 15.9 | 46.8 | 83.9 | 367.0 |
| Consolidated Financial Statements | 15.0 | 46.7 | 84.7 | 366.3 |
| Monthly Analytics Digest | 14.5 | 46.7 | 84.9 | 382.3 |
| Board Report — Q4 FY2026 | 15.6 | 47.6 | 83.0 | 359.5 |
| Sales Summary by Region | 14.6 | 45.8 | 83.8 | 361.1 |
| Pro-Forma Invoice | 14.4 | 45.9 | 84.5 | 362.0 |
| Credit Note | 14.1 | 45.9 | 81.9 | 358.8 |
| Multi-Currency Invoice | 14.4 | 45.9 | 82.8 | 365.1 |
| Recurring Billing Statement | 14.3 | 45.8 | 82.5 | 369.9 |
| Book Chapter — The Cartography of Rivers | 15.4 | 48.7 | — | 368.1 |
| Whitepaper — Deterministic Document Rendering | 14.9 | 48.4 | 90.2 | 367.9 |
| Thesis Excerpt — Sediment Transport | 14.5 | 47.8 | 90.4 | 367.4 |
| Annual Operations Report — Long Report | 14.4 | 46.4 | 82.6 | 360.0 |
| Journal Paper — Stage–discharge hysteresis | 14.0 | 48.5 | 90.8 | 372.1 |
| Conference Paper — Sparse Sampling of Bedload | 14.2 | 48.5 | 90.9 | 377.9 |
| Preprint — Convergence of Iterative Page Breaking | 15.3 | 48.6 | 90.8 | 371.1 |
| Technical Report — Capacity Planning | 14.3 | 46.8 | 84.6 | 380.5 |
| Product Manual — Northwind 2400 Pallet Truck | 14.1 | 48.8 | 91.0 | 384.4 |
| Spec Sheet — Northwind 2400 Series | 14.2 | 45.7 | 82.4 | 360.4 |
| Price Catalog 2026 — Meridian Trading | 14.5 | 45.9 | 82.3 | 369.5 |
| Data Sheet — Fernwood Cloud Platform | 14.2 | 46.5 | 84.2 | 376.3 |
| Business Letter — Meridian Trading Co. | 14.0 | 45.4 | 80.3 | 339.5 |
| Internal Memo — Platform Migration | 13.7 | 44.5 | 80.6 | 357.8 |
| Cover Letter — Application | 13.4 | 46.1 | 88.2 | 357.8 |
| Formal Notice — Service of Notice | 14.1 | 44.7 | 80.2 | 360.6 |

## Peak memory, whole process tree — physical footprint (MiB)

Sum of each process's private physical footprint (the figure Activity Monitor shows as Memory); excludes shared libraries. The fairest single cross-engine memory comparison.

| Fixture | ta | prince | weasy | chrome |
|---|---|---|---|---|
| Invoice | 8.3 | 20.4 | 92.6 | 76.1 |
| Quarterly Report | 8.6 | 23.2 | 95.3 | 90.5 |
| Academic Paper | 9.5 | 24.1 | 102.6 | 75.1 |
| Letterhead | 9.3 | 24.8 | 100.0 | 80.0 |
| Inventory Ledger — Table Fragmentation Stress | 13.2 | 25.9 | 115.3 | 127.2 |
| Float Showcase — Figure & Pull Quote | 9.2 | 23.6 | 99.8 | 78.9 |
| Prose Showcase | 9.5 | 23.6 | 98.1 | 74.2 |
| Invoice — Statement of Account | 9.7 | 21.5 | 94.8 | 87.9 |
| Consolidated Financial Statements | 9.1 | 22.1 | 95.8 | 92.7 |
| Monthly Analytics Digest | 8.4 | 21.9 | 95.0 | 97.5 |
| Board Report — Q4 FY2026 | 9.4 | 23.0 | 93.7 | 78.1 |
| Sales Summary by Region | 8.6 | 21.3 | 96.1 | 80.2 |
| Pro-Forma Invoice | 8.4 | 21.3 | 96.3 | 83.5 |
| Credit Note | 8.0 | 21.3 | 94.6 | 78.6 |
| Multi-Currency Invoice | 8.4 | 21.3 | 95.5 | 85.0 |
| Recurring Billing Statement | 8.3 | 21.2 | 93.3 | 84.0 |
| Book Chapter — The Cartography of Rivers | 9.4 | 23.5 | — | 82.7 |
| Whitepaper — Deterministic Document Rendering | 8.7 | 23.7 | 98.5 | 82.2 |
| Thesis Excerpt — Sediment Transport | 8.5 | 23.1 | 101.1 | 84.7 |
| Annual Operations Report — Long Report | 8.4 | 21.7 | 94.1 | 80.1 |
| Journal Paper — Stage–discharge hysteresis | 8.0 | 22.8 | 100.1 | 90.8 |
| Conference Paper — Sparse Sampling of Bedload | 8.2 | 23.6 | 100.1 | 92.5 |
| Preprint — Convergence of Iterative Page Breaking | 9.2 | 23.5 | 100.1 | 86.3 |
| Technical Report — Capacity Planning | 8.3 | 22.0 | 95.6 | 94.9 |
| Product Manual — Northwind 2400 Pallet Truck | 8.2 | 24.0 | 103.9 | 101.9 |
| Spec Sheet — Northwind 2400 Series | 8.2 | 21.1 | 95.4 | 78.7 |
| Price Catalog 2026 — Meridian Trading | 8.4 | 21.3 | 94.0 | 87.1 |
| Data Sheet — Fernwood Cloud Platform | 8.2 | 21.7 | 95.2 | 94.1 |
| Business Letter — Meridian Trading Co. | 8.0 | 21.0 | 91.8 | 69.1 |
| Internal Memo — Platform Migration | 7.8 | 20.0 | 92.1 | 78.1 |
| Cover Letter — Application | 7.6 | 21.6 | 98.2 | 79.3 |
| Formal Notice — Service of Notice | 8.0 | 20.3 | 91.0 | 78.2 |

Processes per render: ta 1, prince 1, weasy 4, chrome 5.

## Page counts (sanity)

| Fixture | ta | prince | weasy | chrome |
|---|---|---|---|---|
| Invoice | 5 | 5 | — | 5 |
| Quarterly Report | 13 | 11 | — | 11 |
| Academic Paper | 12 | 11 | — | 12 |
| Letterhead | 8 | 8 | — | 5 |
| Inventory Ledger — Table Fragmentation Stress | 52 | 45 | — | 35 |
| Float Showcase — Figure & Pull Quote | 8 | 8 | — | 8 |
| Prose Showcase | 11 | 11 | — | 11 |
| Invoice — Statement of Account | 5 | 5 | — | 6 |
| Consolidated Financial Statements | 11 | 11 | — | 12 |
| Monthly Analytics Digest | 8 | 8 | — | 8 |
| Board Report — Q4 FY2026 | 12 | 13 | — | 13 |
| Sales Summary by Region | 7 | 7 | — | 7 |
| Pro-Forma Invoice | 8 | 15 | — | 7 |
| Credit Note | 7 | 10 | — | 7 |
| Multi-Currency Invoice | 7 | 7 | — | 8 |
| Recurring Billing Statement | 6 | 7 | — | 7 |
| Book Chapter — The Cartography of Rivers | 9 | 8 | — | 9 |
| Whitepaper — Deterministic Document Rendering | 10 | 12 | — | 12 |
| Thesis Excerpt — Sediment Transport | 9 | 9 | — | 9 |
| Annual Operations Report — Long Report | 9 | 9 | — | 10 |
| Journal Paper — Stage–discharge hysteresis | 9 | 6 | — | 5 |
| Conference Paper — Sparse Sampling of Bedload | 8 | 9 | — | 8 |
| Preprint — Convergence of Iterative Page Breaking | 8 | 8 | — | 8 |
| Technical Report — Capacity Planning | 10 | 10 | — | 10 |
| Product Manual — Northwind 2400 Pallet Truck | 9 | 9 | — | 9 |
| Spec Sheet — Northwind 2400 Series | 5 | 5 | — | 5 |
| Price Catalog 2026 — Meridian Trading | 7 | 4 | — | 6 |
| Data Sheet — Fernwood Cloud Platform | 7 | 7 | — | 7 |
| Business Letter — Meridian Trading Co. | 6 | 6 | — | 7 |
| Internal Memo — Platform Migration | 6 | 6 | — | 6 |
| Cover Letter — Application | 6 | 6 | — | 6 |
| Formal Notice — Service of Notice | 7 | 6 | — | 6 |

